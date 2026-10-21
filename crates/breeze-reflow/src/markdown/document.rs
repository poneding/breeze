//! The reading model: one mdast parse split into top-level display blocks.
//!
//! A block is the unit the reader virtualizes. It keeps its own source range,
//! the HTML projection the rich-text view draws, the plain text used for search
//! and the character count, and — for blocks containing code — the source slice
//! GPUI Kit's Markdown view needs for a real code card.
use super::{
    MAX_BLOCKS, MAX_DEPTH, MAX_RENDER_BYTES, MAX_SOURCE_BYTES,
    ast::{self, References},
    render,
};
use crate::{fingerprint, read_bounded};
use anyhow::{Context, Result, ensure};
use markdown::mdast::{self, Node};
use std::{
    collections::HashMap,
    ops::Range,
    path::{Path, PathBuf},
    sync::Arc,
};

/// How the reading surface should draw a top-level block.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RenderKind {
    /// Ordinary Markdown drawn from the HTML projection.
    #[default]
    Prose,
    /// A thematic break, or a line of only `=`/`-`/`*`/`_` characters.
    Rule,
    /// The document's YAML front matter, drawn as a metadata card.
    FrontMatter,
    /// A `[TOC]` placeholder, replaced by the document's heading list.
    Toc,
    /// A display formula. Breeze shows the TeX source; it does not typeset it.
    Math,
    /// A ```mermaid fence, drawn as a diagram when the native renderer can parse it.
    Mermaid,
    /// A paragraph made only of images, drawn as a centred row.
    Images,
    /// A list whose items carry GFM task state, drawn with rounded checkboxes.
    Tasks,
    /// A GFM table, drawn as a native grid so alignment and selection agree.
    Table,
    /// A fenced or indented code block, drawn with its own header and controls.
    Code,
}

/// One image reference in an image-only paragraph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageRef {
    pub url: String,
    pub alt: String,
}

/// One task row, with its inline content rendered to HTML.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskItem {
    pub checked: bool,
    pub depth: usize,
    pub html: String,
}

/// A table cell's inline HTML and the column alignment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColumnAlign {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TableData {
    pub aligns: Vec<ColumnAlign>,
    pub head: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CodeInfo {
    pub language: Option<String>,
    pub text: String,
}

/// The structured content a native reading block needs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum RenderPayload {
    #[default]
    None,
    Images(Vec<ImageRef>),
    Tasks(Vec<TaskItem>),
    Table(TableData),
    Code(CodeInfo),
}

#[derive(Clone, Debug)]
pub struct Block {
    pub source: Range<usize>,
    pub html: Arc<str>,
    pub text: Arc<str>,
    /// The block's own Markdown source, kept when the block contains a code
    /// block, a table or a task list.
    ///
    /// [`Self::html`] is handed to the rich-text view's HTML reader, which has
    /// no `<pre>` code card and cannot express table alignment or checkboxes.
    /// Those blocks are laid out from this source by the Markdown reader
    /// instead, so a reader still shows real code, aligned tables and checks.
    pub markdown: Option<Arc<str>>,
    /// The block's reading shape, for blocks the reader draws natively.
    pub kind: RenderKind,
    /// Native content: image references, task rows, table cells or code.
    pub payload: RenderPayload,
}

#[derive(Clone, Debug)]
pub struct Heading {
    pub title: String,
    pub level: usize,
    pub block: usize,
    pub anchor: String,
}

#[derive(Clone, Debug)]
pub struct Document {
    pub blocks: Vec<Block>,
    pub headings: Vec<Heading>,
    pub source_bytes: usize,
    pub characters: usize,
    pub fingerprint: String,
}

impl Document {
    pub fn open(path: &Path) -> Result<Self> {
        let bytes = read_bounded(path, MAX_SOURCE_BYTES)?;
        let source = std::str::from_utf8(&bytes)?;
        Self::parse(source)
    }

    pub fn parse(source: &str) -> Result<Self> {
        ensure!(
            source.len() <= MAX_SOURCE_BYTES,
            "Markdown exceeds the 16 MiB input limit"
        );
        // The parse itself enforces the smaller rendering ceiling.
        let root = ast::parse(source)?;
        let references = ast::References::new(&root);
        let mut document = Self {
            blocks: Vec::new(),
            headings: Vec::new(),
            source_bytes: source.len(),
            characters: 0,
            fingerprint: fingerprint(source.as_bytes()),
        };
        let mut anchors = HashMap::<String, usize>::new();
        let mut render_bytes = 0usize;
        let mut toc_blocks: Vec<usize> = Vec::new();
        for node in root.children().map(Vec::as_slice).unwrap_or_default() {
            check_depth(node)?;
            // A definition is an association, not content: the reader has never
            // shown one, and the block indexes are display indexes.
            if matches!(node, Node::Definition(_)) {
                continue;
            }
            // PHP-Markdown abbreviation definitions are declarations, not prose.
            if is_abbreviation_block(node, source) {
                continue;
            }
            let source_range = ast::span(node).context("A Markdown block has no position")?;
            ensure!(
                source_range.end <= source.len(),
                "A Markdown block points past the source"
            );
            let block = document.blocks.len();
            let (kind, payload) = shape(node, source, &source_range, &references);
            let mut html = String::new();
            if kind == RenderKind::Toc {
                toc_blocks.push(block);
            } else {
                render::html(node, source, &references, &mut html);
            }
            let text = ast::block_text(node);
            render_bytes += html.len() + text.len() + payload_bytes(&payload);
            ensure!(
                render_bytes <= MAX_RENDER_BYTES,
                "Markdown exceeds the 64 MiB render-text budget"
            );
            ensure!(
                document.blocks.len() < MAX_BLOCKS,
                "Markdown has too many blocks"
            );
            document.characters += text.chars().count();
            collect_headings(node, block, &mut document.headings, &mut anchors);
            let markdown = (kind == RenderKind::Mermaid
                || ast::contains_code(node)
                || ast::contains_table(node)
                || ast::contains_task(node))
            .then(|| Arc::from(&source[source_range.clone()]));
            document.blocks.push(Block {
                markdown,
                source: source_range,
                html: html.into(),
                text: text.into(),
                kind,
                payload,
            });
        }
        // A table of contents may precede the headings it lists, so its HTML is
        // filled only after every heading has been collected.
        if !toc_blocks.is_empty() {
            let toc = toc_html(&document.headings);
            for block in toc_blocks {
                render_bytes += toc.len();
                ensure!(
                    render_bytes <= MAX_RENDER_BYTES,
                    "Markdown exceeds the 64 MiB render-text budget"
                );
                document.blocks[block].html = Arc::from(toc.as_str());
            }
        }
        Ok(document)
    }

    pub fn block_at_offset(&self, offset: usize) -> usize {
        self.blocks
            .partition_point(|block| block.source.end <= offset)
            .min(self.blocks.len().saturating_sub(1))
    }

    pub fn anchor_block(&self, anchor: &str) -> Option<usize> {
        self.headings
            .iter()
            .find(|heading| heading.anchor == anchor)
            .map(|heading| heading.block)
    }

    /// Resolve a Markdown image reference against the document's directory.
    ///
    /// A Markdown image target is a URL, not a filesystem path: editors write
    /// spaces as `%20`, and a target may carry a query or fragment. Resolving
    /// the raw text opens the wrong file, which is why a note whose picture
    /// exists could still report that the image was unavailable.
    pub fn resolve_image(document: &Path, uri: &str) -> Result<PathBuf> {
        let target = uri.split(['?', '#']).next().unwrap_or(uri).trim();
        ensure!(!target.is_empty(), "The image reference is empty");
        // Windows calls `/notes/x.png` rooted rather than absolute, because it
        // carries no drive prefix, so the root check has to sit beside
        // `is_absolute` for a rooted reference to be refused there too.
        let reference = Path::new(target);
        ensure!(
            !target.contains(':')
                && !target.starts_with("//")
                && !reference.is_absolute()
                && !reference.has_root(),
            "Only relative local images are supported: {uri}"
        );
        let decoded = percent_encoding::percent_decode_str(target).decode_utf8()?;
        let parent = document.parent().context("The document has no directory")?;
        Ok(parent.join(decoded.as_ref()))
    }

    pub fn search(&self, query: &str) -> Vec<(usize, String)> {
        if query.is_empty() || query.len() > 256 {
            return Vec::new();
        }
        let needle = query.to_lowercase();
        self.blocks
            .iter()
            .enumerate()
            .filter(|(_, block)| block.text.to_lowercase().contains(&needle))
            .take(300)
            .map(|(index, block)| (index, block.text.chars().take(160).collect()))
            .collect()
    }
}

/// Refuse a tree nested deeper than the reader is willing to lay out.
fn check_depth(node: &Node) -> Result<()> {
    let mut stack = vec![(node, 1usize)];
    while let Some((node, depth)) = stack.pop() {
        ensure!(depth <= MAX_DEPTH, "Markdown nesting exceeds 64 levels");
        if let Some(children) = node.children() {
            stack.extend(children.iter().map(|child| (child, depth + 1)));
        }
    }
    Ok(())
}

/// Record every heading in source order, with anchors unique per document.
fn collect_headings(
    node: &Node,
    block: usize,
    headings: &mut Vec<Heading>,
    anchors: &mut HashMap<String, usize>,
) {
    for node in ast::preorder(node) {
        let Node::Heading(heading) = node else {
            continue;
        };
        let title = ast::heading_title(&heading.children);
        let base = slug(&title);
        let count = anchors.entry(base.clone()).or_default();
        let anchor = if *count == 0 {
            base
        } else {
            format!("{base}-{count}")
        };
        *count += 1;
        headings.push(Heading {
            title,
            level: usize::from(heading.depth),
            block,
            anchor,
        });
    }
}

/// Classify a top-level block and build what a native renderer needs.
fn shape(
    node: &Node,
    source: &str,
    span: &Range<usize>,
    references: &References,
) -> (RenderKind, RenderPayload) {
    match node {
        Node::ThematicBreak(_) => (RenderKind::Rule, RenderPayload::None),
        Node::Yaml(_) | Node::Toml(_) => (RenderKind::FrontMatter, RenderPayload::None),
        Node::Math(_) => (RenderKind::Math, RenderPayload::None),
        Node::Code(code) => {
            let info = CodeInfo {
                language: code.lang.clone(),
                text: code.value.clone(),
            };
            let kind = if code.lang.as_deref() == Some("mermaid") {
                RenderKind::Mermaid
            } else {
                RenderKind::Code
            };
            (kind, RenderPayload::Code(info))
        }
        Node::Table(table) => (
            RenderKind::Table,
            RenderPayload::Table(table_data(table, source, references)),
        ),
        Node::List(list) => match task_items(list, source, references) {
            Some(items) if !items.is_empty() => (RenderKind::Tasks, RenderPayload::Tasks(items)),
            _ => (RenderKind::Prose, RenderPayload::None),
        },
        Node::Paragraph(paragraph) => {
            let text = source.get(span.clone()).unwrap_or_default().trim();
            if is_rule_line(text) {
                return (RenderKind::Rule, RenderPayload::None);
            }
            if is_toc_marker(text) {
                return (RenderKind::Toc, RenderPayload::None);
            }
            match image_refs(&paragraph.children, references) {
                Some(images) if !images.is_empty() => {
                    (RenderKind::Images, RenderPayload::Images(images))
                }
                _ => (RenderKind::Prose, RenderPayload::None),
            }
        }
        _ => (RenderKind::Prose, RenderPayload::None),
    }
}

/// The images in a paragraph when every other child is whitespace.
fn image_refs(children: &[Node], references: &References) -> Option<Vec<ImageRef>> {
    let mut images = Vec::new();
    let mut only_images = true;
    collect_images(children, references, &mut images, &mut only_images);
    only_images.then_some(images)
}

fn collect_images(
    children: &[Node],
    references: &References,
    out: &mut Vec<ImageRef>,
    only_images: &mut bool,
) {
    for child in children {
        match child {
            Node::Image(image) => out.push(ImageRef {
                url: image.url.clone(),
                alt: image.alt.clone(),
            }),
            Node::ImageReference(image) => match references.definition(&image.identifier) {
                Some(definition) => out.push(ImageRef {
                    url: definition.url.clone(),
                    alt: image.alt.clone(),
                }),
                None => *only_images = false,
            },
            Node::Link(link) => collect_images(&link.children, references, out, only_images),
            Node::Text(text) => {
                if !text.value.trim().is_empty() {
                    *only_images = false;
                }
            }
            Node::Break(_) => {}
            _ => *only_images = false,
        }
    }
}

/// The task rows of a list, including nested tasks, or `None` for an ordinary
/// list. Non-task items in a mixed list become unchecked rows.
fn task_items(list: &mdast::List, source: &str, references: &References) -> Option<Vec<TaskItem>> {
    fn walk(
        list: &mdast::List,
        source: &str,
        references: &References,
        depth: usize,
        out: &mut Vec<TaskItem>,
        any_task: &mut bool,
    ) {
        for item in &list.children {
            let Node::ListItem(item) = item else {
                continue;
            };
            if item.checked.is_some() {
                *any_task = true;
            }
            let mut html = String::new();
            for (index, child) in item.children.iter().enumerate() {
                match child {
                    Node::Paragraph(paragraph) => {
                        if index > 0 {
                            html.push_str("<br>");
                        }
                        html.push_str(&render::inline_html(
                            &paragraph.children,
                            source,
                            references,
                        ));
                    }
                    Node::List(list) => walk(list, source, references, depth + 1, out, any_task),
                    _ => {}
                }
            }
            out.push(TaskItem {
                checked: item.checked.unwrap_or(false),
                depth,
                html,
            });
        }
    }
    let mut items = Vec::new();
    let mut any_task = false;
    walk(list, source, references, 0, &mut items, &mut any_task);
    any_task.then_some(items)
}

/// A GFM table as aligned header and body cells of inline HTML.
fn table_data(table: &mdast::Table, source: &str, references: &References) -> TableData {
    let aligns = table
        .align
        .iter()
        .map(|align| match align {
            mdast::AlignKind::Center => ColumnAlign::Center,
            mdast::AlignKind::Right => ColumnAlign::Right,
            _ => ColumnAlign::Left,
        })
        .collect();
    let mut rows: Vec<Vec<String>> = Vec::new();
    for row in &table.children {
        let Node::TableRow(row) = row else {
            continue;
        };
        let cells = row
            .children
            .iter()
            .map(|cell| {
                let Node::TableCell(cell) = cell else {
                    return String::new();
                };
                render::inline_html(&cell.children, source, references)
            })
            .collect();
        rows.push(cells);
    }
    let head = if rows.is_empty() {
        Vec::new()
    } else {
        rows.remove(0)
    };
    TableData { aligns, head, rows }
}

/// The bytes a native payload adds to the retained reading model.
fn payload_bytes(payload: &RenderPayload) -> usize {
    match payload {
        RenderPayload::None => 0,
        RenderPayload::Images(images) => images
            .iter()
            .map(|image| image.url.len() + image.alt.len())
            .sum(),
        RenderPayload::Tasks(tasks) => tasks.iter().map(|task| task.html.len()).sum(),
        RenderPayload::Table(table) => {
            table.head.iter().map(String::len).sum::<usize>()
                + table
                    .rows
                    .iter()
                    .flat_map(|row| row.iter().map(String::len))
                    .sum::<usize>()
        }
        RenderPayload::Code(code) => code.text.len() + code.language.as_deref().map_or(0, str::len),
    }
}

/// A line of only `=`/`-`/`*`/`_` characters. `---` reaches the AST as a
/// thematic break; `===` is a paragraph in CommonMark but a rule in most
/// hand-written notes.
fn is_rule_line(text: &str) -> bool {
    let mut characters = text.chars().filter(|character| !character.is_whitespace());
    let Some(first) = characters.next() else {
        return false;
    };
    matches!(first, '=' | '-' | '*' | '_')
        && text.chars().filter(|c| !c.is_whitespace()).count() >= 3
        && characters.all(|character| character == first)
}

/// The placeholders notes use for a table of contents.
fn is_toc_marker(text: &str) -> bool {
    matches!(
        text.to_ascii_lowercase().as_str(),
        "[toc]" | "[[toc]]" | "[[_toc_]]" | "{{toc}}" | "<toc>"
    )
}

/// A paragraph made only of `*[ABBR]: expansion` lines.
fn is_abbreviation_block(node: &Node, source: &str) -> bool {
    let Node::Paragraph(_) = node else {
        return false;
    };
    let Some(span) = ast::span(node) else {
        return false;
    };
    let text = source.get(span).unwrap_or_default();
    let mut any = false;
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let line = line.trim();
        if !line.starts_with("*[") || !line.contains("]: ") {
            return false;
        }
        any = true;
    }
    any
}

/// A flat, link-only table of contents. Headings stay in document order; the
/// destination matters more than nesting for a reading aid.
fn toc_html(headings: &[Heading]) -> String {
    let mut out = String::from("<nav>");
    // One open list per heading level, so a subheading sits inside its
    // parent's list item and inherits the reader's list indentation.
    let mut levels: Vec<u8> = Vec::new();
    for heading in headings {
        if heading.title.is_empty() {
            continue;
        }
        let level = heading.level.clamp(1, 6) as u8;
        while let Some(&top) = levels.last() {
            if top > level {
                out.push_str("</li></ul>");
                levels.pop();
            } else if top == level {
                out.push_str("</li>");
                break;
            } else {
                break;
            }
        }
        if levels.last().is_none_or(|top| *top < level) {
            out.push_str("<ul>");
            levels.push(level);
        }
        out.push_str("<li><a href=\"#");
        render::escape_attribute(&heading.anchor, &mut out);
        out.push_str("\">");
        render::escape_text(&heading.title, &mut out);
        out.push_str("</a>");
    }
    while levels.pop().is_some() {
        out.push_str("</li></ul>");
    }
    out.push_str("</nav>");
    out
}

fn slug(title: &str) -> String {
    title
        .to_lowercase()
        .chars()
        .filter_map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                Some(c)
            } else if c.is_whitespace() {
                Some('-')
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::ast::MAX_PARSE_BYTES;
    use super::*;

    #[test]
    fn semantic_blocks_keep_global_references_and_tables() {
        let doc = Document::parse("# Title\n\n[link][ref]\n\n| A | B |\n|---|---|\n| X | Y |\n\n[ref]: https://example.com\n").unwrap();
        assert_eq!(doc.headings.len(), 1);
        assert!(
            doc.blocks
                .iter()
                .any(|b| b.html.contains("href=\"https://example.com\""))
        );
        let table = doc
            .blocks
            .iter()
            .find(|b| b.html.contains("<table>"))
            .unwrap();
        assert!(table.html.contains("</table>"));
        assert_eq!(doc.search("TITLE")[0].0, 0);
        // The definition itself is not a display block.
        assert_eq!(doc.blocks.len(), 3);
    }

    #[test]
    fn code_is_not_an_outline_and_duplicate_titles_have_distinct_anchors() {
        let doc =
            Document::parse("# 标题\n\n```md\n# not a heading\n```\n\n# 标题\n\n- one\n  - two\n")
                .unwrap();
        assert_eq!(doc.headings.len(), 2);
        assert_eq!(doc.headings[1].anchor, "标题-1");
        assert_eq!(doc.anchor_block("标题-1"), Some(2));
        assert_eq!(doc.block_at_offset(doc.source_bytes), doc.blocks.len() - 1);
        assert!(doc.blocks.last().unwrap().html.contains("two"));
    }

    #[test]
    fn front_matter_toc_rules_and_typographic_marks_render() {
        let source = "---\ntitle: Test\nauthor: Me\n---\n\n# Heading\n\n[TOC]\n\n---\n\n===\n\nX<sub>2</sub> and O<sup>2</sup>, :smile: :rocket:\n\n*[HTML]: HyperText Markup Language\n\nHTML text\n";
        let document = Document::parse(source).unwrap();
        assert_eq!(document.blocks[0].kind, RenderKind::FrontMatter);
        assert!(document.blocks[0].text.contains("title: Test"));
        assert_eq!(document.blocks[1].kind, RenderKind::Prose);
        assert_eq!(document.blocks[2].kind, RenderKind::Toc);
        assert!(
            document.blocks[2]
                .html
                .contains("<a href=\"#heading\">Heading</a>"),
            "{}",
            document.blocks[2].html
        );
        // `---` is a thematic break; a `===` line is a rule extension.
        assert_eq!(document.blocks[3].kind, RenderKind::Rule);
        assert_eq!(document.blocks[4].kind, RenderKind::Rule);
        let text = document
            .blocks
            .iter()
            .find(|block| block.html.contains("X"))
            .expect("the typography block");
        assert!(text.html.contains("X₂"), "{}", text.html);
        assert!(text.html.contains("O²"), "{}", text.html);
        assert!(text.html.contains("😄"), "{}", text.html);
        assert!(text.html.contains("🚀"), "{}", text.html);
        // Abbreviation definitions are declarations, not reading content.
        assert!(
            document
                .blocks
                .iter()
                .all(|block| !block.html.contains("*[HTML]")),
            "an abbreviation definition leaked into the page"
        );
    }

    #[test]
    fn tables_tasks_math_and_mermaid_choose_their_reading_shape() {
        let source = "| A | B |\n|:-:|--:|\n| `help()` | `destroy()` |\n\n- [x] done\n\n$x^2$\n\n$$\nE = mc^2\n$$\n\n```mermaid\ngraph TD\nA-->B\n```\n";
        let document = Document::parse(source).unwrap();
        let table = &document.blocks[0];
        assert_eq!(table.kind, RenderKind::Table);
        assert!(
            matches!(&table.payload, RenderPayload::Table(data) if data.head.len() == 2 && data.rows.len() == 1),
            "{:?}",
            table.payload
        );
        assert!(table.html.contains("help()"), "{}", table.html);
        let tasks = &document.blocks[1];
        assert_eq!(tasks.kind, RenderKind::Tasks);
        assert!(
            matches!(&tasks.payload, RenderPayload::Tasks(items) if items.len() == 1 && items[0].checked),
            "{:?}",
            tasks.payload
        );
        let math = document
            .blocks
            .iter()
            .find(|block| block.kind == RenderKind::Math)
            .expect("a display formula");
        assert!(math.text.contains("E = mc^2"));
        let mermaid = document
            .blocks
            .iter()
            .find(|block| block.kind == RenderKind::Mermaid)
            .expect("a mermaid fence");
        assert!(mermaid.markdown.is_some());
        assert!(mermaid.text.contains("graph TD"));
    }

    #[test]
    fn a_setext_heading_is_still_a_heading() {
        let doc = Document::parse("Title\n=====\n\nbody\n").unwrap();
        assert_eq!(doc.headings.len(), 1);
        assert_eq!(doc.headings[0].title, "Title");
        assert_eq!(doc.headings[0].level, 1);
        assert!(
            doc.blocks[0].html.starts_with("<h1>"),
            "{:?}",
            doc.blocks[0].html
        );
    }

    #[test]
    fn code_blocks_keep_the_markdown_the_reader_needs() {
        let doc = Document::parse(
            "Para\n\n```rust\nfn main() {}\n```\n\n    indented\n\n- item\n  ```\n  nested\n  ```\n",
        )
        .unwrap();
        assert_eq!(doc.blocks.len(), 4);
        // A plain paragraph still renders from its HTML alone.
        assert!(doc.blocks[0].markdown.is_none());
        // The fence's HTML is the subset the reader cannot draw as a code
        // block, which is why the source is kept beside it.
        assert!(doc.blocks[1].html.contains("<pre>"));
        assert_eq!(
            doc.blocks[1].markdown.as_deref(),
            Some("```rust\nfn main() {}\n```")
        );
        // A fence nested in a list item keeps the item's own source, so the
        // list keeps its marker and indentation.
        let indented = doc.blocks[2].markdown.as_deref().expect("indented code");
        assert!(indented.trim_end().ends_with("indented"), "{indented:?}");
        let nested = doc.blocks[3].markdown.as_deref().expect("nested fence");
        assert!(nested.starts_with("- item"), "{nested:?}");
        assert!(nested.contains("nested"), "{nested:?}");
    }

    #[test]
    fn image_references_follow_markdown_uri_rules() {
        let document = Path::new("/notes/readme.md");
        assert_eq!(
            Document::resolve_image(document, "shots/first.png").unwrap(),
            PathBuf::from("/notes/shots/first.png")
        );
        assert_eq!(
            Document::resolve_image(document, "my%20shot.png").unwrap(),
            PathBuf::from("/notes/my shot.png")
        );
        assert_eq!(
            Document::resolve_image(document, "chart.png?raw=1#page=2").unwrap(),
            PathBuf::from("/notes/chart.png")
        );
        assert_eq!(
            Document::resolve_image(document, "%E5%9B%BE.png").unwrap(),
            PathBuf::from("/notes/图.png")
        );
        for rejected in [
            "https://example.com/x.png",
            "mailto:someone",
            "/absolute/x.png",
            "//host/x.png",
            "",
        ] {
            assert!(
                Document::resolve_image(document, rejected).is_err(),
                "{rejected} should not resolve to a local image"
            );
        }
    }

    #[test]
    fn empty_unicode_and_oversized_documents() {
        assert!(Document::parse("").unwrap().blocks.is_empty());
        assert_eq!(Document::parse("你好 🦀").unwrap().characters, 5);
        assert!(Document::parse(&"x".repeat(MAX_SOURCE_BYTES + 1)).is_err());
        assert!(
            Document::parse(&"x".repeat(MAX_PARSE_BYTES + 1)).is_err(),
            "the parse ceiling applies to reading too"
        );
    }

    #[test]
    fn an_image_block_keeps_its_markdown_source() {
        let doc = Document::parse("![shot](a.png)\n").unwrap();
        assert_eq!(doc.blocks.len(), 1);
        assert!(
            doc.blocks[0].html.contains("<img"),
            "{}",
            doc.blocks[0].html
        );
        assert!(
            doc.blocks[0].html.contains("alt=\"shot\""),
            "{}",
            doc.blocks[0].html
        );
        // An image is not a code block, so the HTML path draws it.
        assert!(doc.blocks[0].markdown.is_none());
    }
}
