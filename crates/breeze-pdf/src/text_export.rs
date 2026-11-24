//! Structured conversion with editable positioned text and page image regions.
use crate::{MAX_PAGES, PdfError, Source, open_document, page_geometry, render_page_region};
use anyhow::{Context, Result, ensure};
use breeze_core::{
    Rotation,
    annotations::{Rect, ViewGeometry},
};
use docx_rs::*;
use image::ImageEncoder as _;
use mupdf::{TextCharFlags, TextPageFlags};
use std::{fs, path::Path};

const MAX_TEXT_BYTES: usize = 16 * 1024 * 1024;
const MAX_ASSET_BYTES: usize = 32 * 1024 * 1024;
const MAX_ELEMENTS: usize = 100_000;

#[derive(Clone, PartialEq)]
struct Style {
    font: String,
    size: f32,
    color: u32,
    bold: bool,
    italic: bool,
    strike: bool,
    underline: bool,
}
struct TextRun {
    text: String,
    style: Style,
}
enum Element {
    Text {
        bounds: Rect,
        runs: Vec<TextRun>,
        paragraph_end: bool,
    },
    Image {
        bounds: Rect,
        png: Vec<u8>,
        width: u32,
        height: u32,
    },
}
struct PageContent {
    size: (f32, f32),
    elements: Vec<Element>,
}
pub struct TextExport {
    pages: Vec<PageContent>,
    pub empty_pages: usize,
    pub images: usize,
}

/// Engine objects remain on the calling worker. Image regions preserve source
/// density, composition and rotation; vector marks use 288 DPI region rasters.
pub fn extract(source: &Source, fingerprint: &str) -> Result<TextExport> {
    let opened = open_document(source)?;
    ensure!(
        opened.fingerprint == fingerprint,
        "Source changed; export refused"
    );
    let doc = opened.doc;
    ensure!(!doc.needs_password()?, PdfError::Encrypted);
    ensure!(
        doc.trailer()?.get_dict("Encrypt")?.is_none(),
        PdfError::Encrypted
    );
    let count = usize::try_from(doc.page_count()?).context("Invalid PDF page count")?;
    ensure!(
        count > 0 && count <= MAX_PAGES,
        PdfError::PageCountUnsupported { max: MAX_PAGES }
    );
    let mut result = TextExport {
        pages: Vec::with_capacity(count),
        empty_pages: 0,
        images: 0,
    };
    let (mut text_bytes, mut asset_bytes, mut elements) = (0usize, 0usize, 0usize);
    let mut run_count = 0usize;
    let mut integrity = crate::integrity::IntegrityCache::default();
    for index in 0..count {
        let page = doc.load_pdf_page(index as i32)?;
        crate::integrity::verify_page_content(&doc, &page, &mut integrity)
            .with_context(|| format!("Cannot export damaged page {}", index + 1))?;
        let geometry = page_geometry(&doc, index)?;
        let view = ViewGeometry {
            page: geometry,
            rotation: Rotation::default(),
        };
        let size = geometry.size();
        let text = page.to_text_page(
            TextPageFlags::PRESERVE_IMAGES
                | TextPageFlags::COLLECT_STYLES
                | TextPageFlags::COLLECT_VECTORS,
        )?;
        let mut content = PageContent {
            size,
            elements: Vec::new(),
        };
        let mut has_text = false;
        for block in text.blocks() {
            if block.lines().next().is_some() {
                for line in block.lines() {
                    let mut runs: Vec<TextRun> = Vec::new();
                    for character in line.chars() {
                        let Some(ch) = character.char() else {
                            continue;
                        };
                        text_bytes += ch.len_utf8();
                        ensure!(
                            text_bytes <= MAX_TEXT_BYTES,
                            "PDF text exceeds the 16 MiB export limit"
                        );
                        let font = character.font();
                        let name = font.as_ref().map(|font| font.name()).unwrap_or("Arial");
                        let flags = character.flags();
                        let style = Style {
                            font: name
                                .split_once('+')
                                .map(|(_, name)| name)
                                .unwrap_or(name)
                                .to_owned(),
                            size: character.size().clamp(1., 400.),
                            color: character.argb() & 0xffffff,
                            bold: flags.contains(TextCharFlags::BOLD)
                                || font.as_ref().is_some_and(|font| font.is_bold()),
                            italic: font.as_ref().is_some_and(|font| font.is_italic()),
                            strike: flags.contains(TextCharFlags::STRIKEOUT),
                            underline: flags.contains(TextCharFlags::UNDERLINE),
                        };
                        if let Some(last) = runs.last_mut().filter(|run| run.style == style) {
                            last.text.push(ch);
                        } else {
                            run_count += 1;
                            ensure!(
                                run_count <= MAX_ELEMENTS,
                                "PDF exceeds the 100000 text run export limit"
                            );
                            runs.push(TextRun {
                                text: ch.to_string(),
                                style,
                            });
                        }
                        has_text |= !ch.is_whitespace();
                    }
                    if !runs.is_empty() {
                        content.elements.push(Element::Text {
                            bounds: rect(line.bounds()),
                            runs,
                            paragraph_end: false,
                        });
                    }
                }
                if let Some(Element::Text { paragraph_end, .. }) = content.elements.last_mut() {
                    *paragraph_end = true;
                }
            } else {
                let raw = rect(block.bounds());
                // A PDF hairline still needs nonzero device coverage.
                let bounds = Rect {
                    x: (raw.x - 0.5).max(0.),
                    y: (raw.y - 0.5).max(0.),
                    width: (raw.width + 1.).min(size.0 - raw.x.max(0.)),
                    height: (raw.height + 1.).min(size.1 - raw.y.max(0.)),
                };
                if !bounds.valid() {
                    continue;
                }
                let scale = if let Some(image) = block.image() {
                    (image.width() as f32 / bounds.width)
                        .max(image.height() as f32 / bounds.height)
                        .max(1.)
                } else {
                    4.
                };
                let pixels = render_page_region(&page, view, scale, bounds)?;
                let mut rgba = pixels.bgra;
                for pixel in rgba.as_chunks_mut::<4>().0 {
                    pixel.swap(0, 2);
                }
                let mut png = Vec::new();
                image::codecs::png::PngEncoder::new(&mut png).write_image(
                    &rgba,
                    pixels.width,
                    pixels.height,
                    image::ExtendedColorType::Rgba8,
                )?;
                asset_bytes += png.len();
                ensure!(
                    asset_bytes <= MAX_ASSET_BYTES,
                    "PDF images exceed the 32 MiB export budget"
                );
                content.elements.push(Element::Image {
                    bounds,
                    png,
                    width: pixels.width,
                    height: pixels.height,
                });
                result.images += 1;
            }
        }
        elements += content.elements.len();
        ensure!(
            elements <= MAX_ELEMENTS,
            "PDF exceeds the 100000 element export limit"
        );
        result.empty_pages += usize::from(!has_text);
        result.pages.push(content);
    }
    Ok(result)
}

fn rect(bounds: mupdf::Rect) -> Rect {
    Rect {
        x: bounds.x0,
        y: bounds.y0,
        width: bounds.x1 - bounds.x0,
        height: bounds.y1 - bounds.y0,
    }
}
fn twips(points: f32) -> u32 {
    (points.max(0.) * 20.).round() as u32
}
fn margin() -> PageMargin {
    PageMargin {
        top: 0,
        left: 0,
        bottom: 0,
        right: 0,
        header: 0,
        footer: 0,
        gutter: 0,
    }
}

/// PDF's Base 14 names are aliases, not installed Office font families.
/// Use their metric-compatible families to avoid arbitrary missing-font
/// substitution changing line widths in positioned text frames.
fn word_font(font: &str) -> &str {
    match font {
        "Helvetica" | "Helvetica-Bold" | "Helvetica-Oblique" | "Helvetica-BoldOblique" => "Arial",
        "Times-Roman" | "Times-Bold" | "Times-Italic" | "Times-BoldItalic" => "Times New Roman",
        "Courier" | "Courier-Bold" | "Courier-Oblique" | "Courier-BoldOblique" => "Courier New",
        _ => font,
    }
}

fn word_element(element: &Element) -> Paragraph {
    match element {
        Element::Text { bounds, runs, .. } => {
            let mut paragraph = Paragraph::new()
                .h_anchor("page")
                .v_anchor("page")
                .wrap("none")
                .frame_x(twips(bounds.x) as i32)
                .frame_y(twips(bounds.y) as i32)
                .frame_width(twips(bounds.width + 4.))
                .h_rule("auto")
                .line_spacing(LineSpacing::new().before(0).after(0));
            for run in runs {
                let style = &run.style;
                let font = word_font(&style.font);
                let mut value = Run::new()
                    .add_text(&run.text)
                    .size((style.size * 2.).round() as usize)
                    .color(format!("{:06X}", style.color))
                    .fonts(RunFonts::new().ascii(font).hi_ansi(font).east_asia(font));
                if style.bold {
                    value = value.bold();
                }
                if style.italic {
                    value = value.italic();
                }
                if style.strike {
                    value = value.strike();
                }
                if style.underline {
                    value = value.underline("single");
                }
                paragraph = paragraph.add_run(value);
            }
            paragraph
        }
        Element::Image {
            bounds,
            png,
            width,
            height,
        } => {
            let pic = Pic::new_with_dimensions(png.clone(), *width, *height)
                .floating()
                .overlapping()
                .size(
                    (bounds.width * 12700.).round() as u32,
                    (bounds.height * 12700.).round() as u32,
                )
                .relative_from_h(RelativeFromHType::Page)
                .relative_from_v(RelativeFromVType::Page)
                .offset_x((bounds.x * 12700.).round() as i32)
                .offset_y((bounds.y * 12700.).round() as i32);
            Paragraph::new()
                .add_run(Run::new().add_image(pic))
                .line_spacing(LineSpacing::new().line(1).before(0).after(0))
        }
    }
}

impl TextExport {
    pub fn write_word(&self, destination: &Path) -> Result<()> {
        let mut word = Docx::new();
        for (index, page) in self.pages.iter().enumerate() {
            ensure!(
                page.size.0 <= 1584. && page.size.1 <= 1584.,
                "PDF page {} exceeds Word's 22 inch page limit",
                index + 1
            );
            if index + 1 == self.pages.len() {
                word = word
                    .page_size(twips(page.size.0), twips(page.size.1))
                    .page_margin(margin());
            }
            // The pinned writer collects media from body paragraphs, but not
            // Section children. Keep page content in the body and terminate
            // each preceding page with its own paragraph section properties.
            for element in &page.elements {
                word = word.add_paragraph(word_element(element));
            }
            if index + 1 < self.pages.len() {
                let mut section = SectionProperty::new()
                    .page_size(PageSize::new().size(twips(page.size.0), twips(page.size.1)))
                    .page_margin(margin());
                section.section_type = Some(SectionType::NextPage);
                word = word.add_paragraph(Paragraph::new().section_property(section));
            }
        }
        let mut package = word.build();
        // PDF image regions form the page background. The pinned writer has
        // no behind-text setter; set the required attributes on our own OOXML
        // before packaging so images cannot cover editable positioned text.
        package.document = String::from_utf8(package.document)?
            .replace("behindDoc=\"0\"", "behindDoc=\"1\"")
            .replace("allowOverlap=\"0\"", "allowOverlap=\"1\"")
            .into_bytes();
        package
            .pack(fs::File::create(destination)?)
            .context("Cannot write Word document")
    }

    /// Markdown reflows the extracted lines; image assets are sibling files.
    pub fn write_markdown(
        &self,
        destination: &Path,
        assets: &Path,
        asset_link: &str,
    ) -> Result<()> {
        let mut output = String::new();
        let mut image_index = 0;
        for (index, page) in self.pages.iter().enumerate() {
            if index > 0 {
                output.push_str("\n\n---\n\n");
            }
            output.push_str(&format!("<!-- Page {} -->\n\n", index + 1));
            let mut sizes = page
                .elements
                .iter()
                .filter_map(|element| {
                    if let Element::Text { runs, .. } = element {
                        runs.first().map(|run| run.style.size)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            sizes.sort_by(f32::total_cmp);
            let body = sizes.get(sizes.len() / 2).copied().unwrap_or(12.);
            // MuPDF groups each column into its source text block. Sorting all
            // lines by y would interleave left and right column paragraphs.
            for element in &page.elements {
                match element {
                    Element::Text {
                        runs,
                        paragraph_end,
                        ..
                    } => {
                        let size = runs.first().map(|run| run.style.size).unwrap_or(body);
                        if size > body * 1.4 {
                            output.push_str(if size > body * 1.8 { "# " } else { "## " });
                        }
                        for run in runs {
                            if run.style.bold {
                                output.push_str("**");
                            }
                            if run.style.italic {
                                output.push('*');
                            }
                            if run.style.strike {
                                output.push_str("~~");
                            }
                            output.push_str(&escape_markdown(&run.text));
                            if run.style.strike {
                                output.push_str("~~");
                            }
                            if run.style.italic {
                                output.push('*');
                            }
                            if run.style.bold {
                                output.push_str("**");
                            }
                        }
                        output.push_str(if *paragraph_end { "\n\n" } else { "\n" });
                    }
                    Element::Image { png, .. } => {
                        image_index += 1;
                        let name = format!("image-{image_index}.png");
                        fs::write(assets.join(&name), png)?;
                        output.push_str(&format!(
                            "![Page {} image {}]({asset_link}/{name})\n\n",
                            index + 1,
                            image_index
                        ));
                    }
                }
            }
        }
        fs::write(destination, output)?;
        Ok(())
    }
}
fn escape_markdown(text: &str) -> String {
    let mut result = String::new();
    for character in text.chars() {
        if "\\`*_{}[]<>()#+-.!|~=".contains(character) {
            result.push('\\');
        }
        result.push(character);
    }
    result
}
#[cfg(test)]
mod tests;
