//! Native document conversion from the reader's bounded Markdown AST.
//!
//! Callers own destination publication. This adapter writes only to their
//! temporary files and keeps every MuPDF object on the calling worker thread.

use crate::markdown::{MAX_DEPTH, ast};
use anyhow::{Context, Result, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use docx_rs::*;
use markdown::mdast::Node;
use std::{io::Cursor, path::Path};

const MAX_ASSET_BYTES: usize = 32 * 1024 * 1024;
const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;
const MAX_IMAGE_PIXELS: u64 = 16_000_000;
const MAX_EXPORT_PAGES: i32 = 20_000;

pub struct MarkdownExport {
    html: String,
    word: Docx,
    /// Constructs retained as readable source or image links, rather than pixels.
    pub simplified: usize,
}

impl MarkdownExport {
    pub fn write_word(self, destination: &Path) -> Result<()> {
        let file =
            std::fs::File::create(destination).context("Cannot create temporary Word file")?;
        self.word
            .build()
            .pack(file)
            .context("Cannot write Word document")
    }

    pub fn write_pdf(&self, destination: &Path) -> Result<()> {
        let path = destination
            .to_str()
            .context("The PDF export path is not valid UTF-8")?;
        let mut document = mupdf::Document::from_bytes(self.html.as_bytes(), "text/html")
            .context("Cannot lay out Markdown for PDF")?;
        // Physical A4 points; export typography does not depend on window zoom.
        document.layout(595., 842., 12.)?;
        let pages = document.page_count()?;
        ensure!(
            pages > 0 && pages <= MAX_EXPORT_PAGES,
            "Markdown PDF exceeds the page limit"
        );
        {
            let mut writer = mupdf::DocumentWriter::new(path, "pdf", "compress=yes")?;
            for index in 0..pages {
                let page = document.load_page(index)?;
                let device = writer.begin_page(page.bounds()?)?;
                page.run(&device, &mupdf::Matrix::IDENTITY)?;
                writer.end_page(device)?;
            }
        }
        // The pinned writer finalizes on Drop without a fallible close API.
        // Reopening the completed file is required before reporting success.
        let result = mupdf::pdf::PdfDocument::open(path)?;
        ensure!(
            result.page_count()? == pages,
            "Generated PDF has an invalid page count"
        );
        Ok(())
    }
}

pub fn prepare_markdown(source: &str, base: &Path) -> Result<MarkdownExport> {
    let root = ast::parse(source)?;
    ensure!(
        ast::preorder(&root).count() <= 100_000,
        "Markdown exceeds the export node limit"
    );
    let mut renderer = Renderer {
        word: new_word(),
        html: String::from(
            "<!doctype html><html><head><meta charset=\"utf-8\"><style>body{margin:36pt;font-family:sans-serif;font-size:12pt;line-height:1.5;color:#111}h1{font-size:24pt}h2{font-size:20pt}h3{font-size:16pt}h4,h5,h6{font-size:14pt}pre{white-space:pre-wrap;font-size:10pt;background:#f5f5f5;padding:8pt}table{border-collapse:collapse;width:100%}td,th{border:1px solid #aaa;padding:4pt}blockquote{margin-left:18pt;color:#444}img{max-width:100%;height:auto}</style></head><body>",
        ),
        references: ast::References::new(&root),
        base,
        assets: 0,
        simplified: 0,
        next_numbering: 1,
    };
    renderer.blocks(
        root.children().map(Vec::as_slice).unwrap_or_default(),
        0,
        None,
    )?;
    renderer.html.push_str("</body></html>");
    Ok(MarkdownExport {
        html: renderer.html,
        word: renderer.word,
        simplified: renderer.simplified,
    })
}

fn new_word() -> Docx {
    let mut word = Docx::new().add_style(
        Style::new("Normal", StyleType::Paragraph)
            .name("Normal")
            .size(24)
            .fonts(
                RunFonts::new()
                    .ascii("Arial")
                    .hi_ansi("Arial")
                    .east_asia("Microsoft YaHei"),
            ),
    );
    for (level, size) in [48, 40, 32, 28, 26, 24].into_iter().enumerate() {
        let name = format!("Heading{}", level + 1);
        word = word.add_style(
            Style::new(&name, StyleType::Paragraph)
                .name(name)
                .size(size)
                .bold()
                .outline_lvl(level),
        );
    }
    word
}

#[derive(Clone, Copy, Default)]
struct InlineStyle {
    bold: bool,
    italic: bool,
    strike: bool,
    code: bool,
}

struct Renderer<'a> {
    word: Docx,
    html: String,
    references: ast::References,
    base: &'a Path,
    assets: usize,
    simplified: usize,
    next_numbering: usize,
}

impl Renderer<'_> {
    fn paragraph(&mut self, paragraph: Paragraph) {
        self.word = std::mem::take(&mut self.word).add_paragraph(paragraph);
    }

    fn blocks(&mut self, nodes: &[Node], depth: usize, numbering: Option<usize>) -> Result<()> {
        ensure!(
            depth <= MAX_DEPTH,
            "Markdown containers exceed the export depth limit"
        );
        for node in nodes {
            match node {
                Node::Heading(heading) => {
                    self.html.push_str(&format!("<h{}>", heading.depth));
                    let paragraph = self.inline(
                        &heading.children,
                        Paragraph::new().style(&format!("Heading{}", heading.depth)),
                        InlineStyle::default(),
                        depth,
                    )?;
                    self.paragraph(paragraph);
                    self.html.push_str(&format!("</h{}>", heading.depth));
                }
                Node::Paragraph(value) => {
                    self.html.push_str("<p>");
                    let mut paragraph = Paragraph::new();
                    if let Some(id) = numbering {
                        paragraph = paragraph.numbering(NumberingId::new(id), IndentLevel::new(0));
                    }
                    let paragraph =
                        self.inline(&value.children, paragraph, InlineStyle::default(), depth)?;
                    self.paragraph(paragraph);
                    self.html.push_str("</p>");
                }
                Node::List(list) => {
                    let id = self.next_numbering;
                    self.next_numbering += 1;
                    let start = list.start.unwrap_or(1) as usize;
                    let format = if list.ordered { "decimal" } else { "bullet" };
                    self.word = std::mem::take(&mut self.word)
                        .add_abstract_numbering(
                            AbstractNumbering::new(id).add_level(
                                Level::new(
                                    0,
                                    Start::new(start),
                                    NumberFormat::new(format),
                                    LevelText::new(if list.ordered { "%1." } else { "•" }),
                                    LevelJc::new("left"),
                                )
                                .indent(
                                    Some(((depth + 1) * 360) as i32),
                                    Some(SpecialIndentType::Hanging(240)),
                                    None,
                                    None,
                                ),
                            ),
                        )
                        .add_numbering(Numbering::new(id, id));
                    self.html.push_str(&if list.ordered {
                        format!("<ol start=\"{start}\">")
                    } else {
                        "<ul>".into()
                    });
                    self.blocks(&list.children, depth + 1, Some(id))?;
                    self.html
                        .push_str(if list.ordered { "</ol>" } else { "</ul>" });
                }
                Node::ListItem(item) => {
                    self.html.push_str("<li>");
                    if let Some(checked) = item.checked {
                        let text = if checked { "☑ " } else { "☐ " };
                        self.html.push_str(text);
                        self.paragraph(Paragraph::new().add_run(text_run(text)));
                    }
                    // Only the first paragraph carries the item's marker.
                    for (index, child) in item.children.iter().enumerate() {
                        self.blocks(
                            std::slice::from_ref(child),
                            depth + 1,
                            if index == 0 { numbering } else { None },
                        )?;
                    }
                    self.html.push_str("</li>");
                }
                Node::Blockquote(quote) => {
                    self.html.push_str("<blockquote>");
                    self.blocks(&quote.children, depth + 1, None)?;
                    self.html.push_str("</blockquote>");
                }
                Node::Table(table) => {
                    let mut rows = Vec::new();
                    self.html.push_str("<table>");
                    for (row_index, row) in table.children.iter().enumerate() {
                        let mut cells = Vec::new();
                        self.html.push_str("<tr>");
                        for (column, cell) in row.children().into_iter().flatten().enumerate() {
                            let align = table
                                .align
                                .get(column)
                                .copied()
                                .unwrap_or(markdown::mdast::AlignKind::None);
                            let (css, word_align) = match align {
                                markdown::mdast::AlignKind::Center => {
                                    ("center", AlignmentType::Center)
                                }
                                markdown::mdast::AlignKind::Right => {
                                    ("right", AlignmentType::Right)
                                }
                                _ => ("left", AlignmentType::Left),
                            };
                            let tag = if row_index == 0 { "th" } else { "td" };
                            self.html
                                .push_str(&format!("<{tag} style=\"text-align:{css}\">"));
                            let paragraph = self.inline(
                                cell.children().map(Vec::as_slice).unwrap_or_default(),
                                Paragraph::new().align(word_align),
                                InlineStyle {
                                    bold: row_index == 0,
                                    ..Default::default()
                                },
                                depth + 1,
                            )?;
                            cells.push(TableCell::new().add_paragraph(paragraph));
                            self.html.push_str(&format!("</{tag}>"));
                        }
                        self.html.push_str("</tr>");
                        rows.push(TableRow::new(cells));
                    }
                    self.word = std::mem::take(&mut self.word).add_table(Table::new(rows));
                    self.html.push_str("</table>");
                }
                Node::Code(code) => {
                    if code
                        .lang
                        .as_deref()
                        .is_some_and(|lang| matches!(lang, "mermaid" | "math"))
                    {
                        self.simplified += 1;
                    }
                    self.code(&code.value);
                }
                Node::Math(math) => {
                    self.simplified += 1;
                    self.code(&math.value);
                }
                Node::Html(html) => {
                    self.simplified += 1;
                    self.code(&html.value);
                }
                Node::ThematicBreak(_) => {
                    self.html.push_str("<hr>");
                    self.paragraph(Paragraph::new().add_run(text_run("────────")));
                }
                Node::Definition(_) | Node::Yaml(_) | Node::Toml(_) => {}
                Node::FootnoteDefinition(note) => {
                    self.simplified += 1;
                    self.paragraph(
                        Paragraph::new().add_run(text_run(&format!("[{}]", note.identifier))),
                    );
                    self.blocks(&note.children, depth + 1, None)?;
                }
                _ => {
                    if let Some(children) = node.children() {
                        self.blocks(children, depth + 1, numbering)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn code(&mut self, value: &str) {
        self.html
            .push_str(&format!("<pre><code>{}</code></pre>", escape(value)));
        self.paragraph(
            Paragraph::new().add_run(
                text_run(value).size(20).fonts(
                    RunFonts::new()
                        .ascii("Consolas")
                        .east_asia("Microsoft YaHei"),
                ),
            ),
        );
    }

    fn inline(
        &mut self,
        nodes: &[Node],
        mut paragraph: Paragraph,
        style: InlineStyle,
        depth: usize,
    ) -> Result<Paragraph> {
        ensure!(
            depth <= MAX_DEPTH,
            "Markdown inline content exceeds the export depth limit"
        );
        for node in nodes {
            match node {
                Node::Strong(value) => {
                    self.html.push_str("<strong>");
                    paragraph = self.inline(
                        &value.children,
                        paragraph,
                        InlineStyle {
                            bold: true,
                            ..style
                        },
                        depth + 1,
                    )?;
                    self.html.push_str("</strong>");
                }
                Node::Emphasis(value) => {
                    self.html.push_str("<em>");
                    paragraph = self.inline(
                        &value.children,
                        paragraph,
                        InlineStyle {
                            italic: true,
                            ..style
                        },
                        depth + 1,
                    )?;
                    self.html.push_str("</em>");
                }
                Node::Delete(value) => {
                    self.html.push_str("<del>");
                    paragraph = self.inline(
                        &value.children,
                        paragraph,
                        InlineStyle {
                            strike: true,
                            ..style
                        },
                        depth + 1,
                    )?;
                    self.html.push_str("</del>");
                }
                Node::Link(link) => {
                    paragraph = self.link(paragraph, &ast::block_text(node), &link.url);
                }
                Node::LinkReference(link) => {
                    if let Some(url) = self
                        .references
                        .definition(&link.identifier)
                        .map(|definition| definition.url.clone())
                    {
                        paragraph = self.link(paragraph, &ast::block_text(node), &url);
                    } else {
                        paragraph = self.inline(&link.children, paragraph, style, depth + 1)?;
                    }
                }
                Node::Image(image) => {
                    paragraph = self.image(paragraph, &image.url, &image.alt)?;
                }
                Node::ImageReference(image) => {
                    if let Some(url) = self
                        .references
                        .definition(&image.identifier)
                        .map(|definition| definition.url.clone())
                    {
                        paragraph = self.image(paragraph, &url, &image.alt)?;
                    }
                }
                Node::Break(_) => {
                    self.html.push_str("<br>");
                    paragraph = paragraph.add_run(Run::new().add_break(BreakType::TextWrapping));
                }
                _ => {
                    let (value, code) = match node {
                        Node::Text(text) => (text.value.clone(), false),
                        Node::InlineCode(text) => (text.value.clone(), true),
                        Node::InlineMath(text) => {
                            self.simplified += 1;
                            (text.value.clone(), true)
                        }
                        Node::Html(text) => {
                            self.simplified += 1;
                            (text.value.clone(), true)
                        }
                        Node::FootnoteReference(note) => (format!("[{}]", note.identifier), false),
                        _ => (node.to_string(), false),
                    };
                    if code {
                        self.html.push_str("<code>");
                    }
                    self.html.push_str(&escape(&value));
                    if code {
                        self.html.push_str("</code>");
                    }
                    let mut run = text_run(&value);
                    if style.bold {
                        run = run.bold();
                    }
                    if style.italic {
                        run = run.italic();
                    }
                    if style.strike {
                        run = run.strike();
                    }
                    if code || style.code {
                        run = run.fonts(
                            RunFonts::new()
                                .ascii("Consolas")
                                .east_asia("Microsoft YaHei"),
                        );
                    }
                    paragraph = paragraph.add_run(run);
                }
            }
        }
        Ok(paragraph)
    }

    fn link(&mut self, paragraph: Paragraph, label: &str, url: &str) -> Paragraph {
        self.html.push_str(&format!(
            "<a href=\"{}\">{}</a>",
            escape(url),
            escape(label)
        ));
        paragraph
            .add_hyperlink(Hyperlink::new(url, HyperlinkType::External).add_run(text_run(label)))
    }

    fn image(&mut self, paragraph: Paragraph, url: &str, alt: &str) -> Result<Paragraph> {
        // Network and SVG assets remain disclosed links. No network operation
        // or unbounded external renderer is introduced by document export.
        let Ok(path) = crate::markdown::resolve_image(self.base, url) else {
            self.simplified += 1;
            return Ok(self.link(paragraph, &format!("[{alt}]"), url));
        };
        if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"))
        {
            self.simplified += 1;
            return Ok(self.link(paragraph, &format!("[{alt}]"), url));
        }
        let bytes = crate::read_bounded(&path, MAX_IMAGE_BYTES)
            .with_context(|| format!("Cannot export image {}", path.display()))?;
        let mut reader = image::ImageReader::new(Cursor::new(&bytes)).with_guessed_format()?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(16_000);
        limits.max_image_height = Some(16_000);
        limits.max_alloc = Some(MAX_IMAGE_PIXELS * 4);
        reader.limits(limits);
        let image = reader.decode().context("Cannot decode export image")?;
        let (width, height) = (image.width(), image.height());
        ensure!(
            u64::from(width) * u64::from(height) <= MAX_IMAGE_PIXELS,
            "Export image exceeds the pixel limit"
        );
        let mut png = Cursor::new(Vec::new());
        image.write_to(&mut png, image::ImageFormat::Png)?;
        let png = png.into_inner();
        self.assets = self.assets.saturating_add(png.len());
        ensure!(
            self.assets <= MAX_ASSET_BYTES,
            "Markdown images exceed the 32 MiB export budget"
        );
        self.html.push_str(&format!(
            "<img src=\"data:image/png;base64,{}\" alt=\"{}\">",
            STANDARD.encode(&png),
            escape(alt)
        ));
        let scale = (480. / f64::from(width))
            .min(680. / f64::from(height))
            .min(1.);
        let pic = Pic::new_with_dimensions(png, width, height).size(
            (f64::from(width) * scale * 9525.) as u32,
            (f64::from(height) * scale * 9525.) as u32,
        );
        Ok(paragraph.add_run(Run::new().add_image(pic)))
    }
}

fn text_run(text: &str) -> Run {
    let mut run = Run::new();
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            run = run.add_break(BreakType::TextWrapping);
        }
        run = run.add_text(line.trim_end_matches('\r'));
    }
    run
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests;
