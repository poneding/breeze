//! One markdown-rs parse, and the source geometry every consumer shares.
//!
//! Reading, editing and the source view must agree about what a construct is
//! and where its bytes live. This module is the only place that calls the
//! parser: it owns the option set, the parse ceiling, document-wide link and
//! footnote resolution, and the plain text used for search and character
//! counts. It never rewrites the source.
use super::MAX_DEPTH;
use anyhow::{Result, ensure};
use markdown::{
    ParseOptions,
    mdast::{self, Node},
};
use std::{collections::HashMap, ops::Range};

/// One parse may hold at most this much source.
///
/// markdown-rs is superlinear in node count (upstream issue #113), so a whole
/// document is parsed only up to a size that stays bounded on a worker. Larger
/// Markdown files remain editable as source. See
/// `docs/specs/15-markdown-rs-parser.md`.
pub const MAX_PARSE_BYTES: usize = 2 * 1024 * 1024;

/// The reader, editor and worker share this one extension set.
pub fn options() -> ParseOptions {
    ParseOptions::gfm()
}

/// Parse one immutable snapshot.
pub fn parse(source: &str) -> Result<Node> {
    ensure!(
        source.len() <= MAX_PARSE_BYTES,
        "Markdown exceeds the {} MiB parse limit",
        MAX_PARSE_BYTES / 1024 / 1024
    );
    check_nesting(source)?;
    markdown::to_mdast(source, &options()).map_err(|error| anyhow::anyhow!("{error}"))
}

/// Refuse impossible container nesting before paying for a parse.
///
/// markdown-rs is superlinear in container count: a line with thousands of
/// `>` or `- ` markers occupies the worker for minutes before the finished
/// tree could be depth-checked. Counting the leading container chain is cheap
/// and matches the depth policy the tree itself is held to.
fn check_nesting(source: &str) -> Result<()> {
    for line in source.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        let mut rest = line;
        let mut depth = 0usize;
        loop {
            let trimmed = rest.trim_start_matches(' ');
            // Four spaces begin an indented code block, never another container.
            if rest.len() - trimmed.len() > 3 {
                break;
            }
            if let Some(after) = trimmed.strip_prefix('>') {
                depth += 1;
                rest = after.strip_prefix(' ').unwrap_or(after);
            } else if let Some(after) = strip_list_marker(trimmed) {
                depth += 1;
                rest = after;
            } else {
                break;
            }
            ensure!(depth <= MAX_DEPTH, "Markdown nesting exceeds 64 levels");
        }
    }
    Ok(())
}

/// Strip one list marker and the whitespace after it.
fn strip_list_marker(text: &str) -> Option<&str> {
    let bytes = text.as_bytes();
    let marker = match bytes.first()? {
        b'-' | b'+' | b'*' => 1,
        byte if byte.is_ascii_digit() => {
            let digits = bytes
                .iter()
                .take_while(|byte| byte.is_ascii_digit())
                .count();
            if digits == 0 || digits > 9 {
                return None;
            }
            match bytes.get(digits)? {
                b'.' | b')' => digits + 1,
                _ => return None,
            }
        }
        _ => return None,
    };
    let rest = text.get(marker..)?;
    if rest.is_empty() {
        return Some(rest);
    }
    rest.starts_with([' ', '\t'])
        .then(|| rest.trim_start_matches([' ', '\t']))
}

/// The source bytes a parsed node spans.
pub fn span(node: &Node) -> Option<Range<usize>> {
    let position = node.position()?;
    let range = position.start.offset..position.end.offset;
    (range.start <= range.end).then_some(range)
}

/// A link or image destination from the document's definitions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Definition {
    pub url: String,
    pub title: Option<String>,
}

/// Document-wide associations: references resolve across blocks.
#[derive(Clone, Debug, Default)]
pub struct References {
    definitions: HashMap<String, Definition>,
    notes: HashMap<String, usize>,
}

impl References {
    /// Collect definitions and number footnotes in first-reference order.
    pub fn new(root: &Node) -> Self {
        let mut references = Self::default();
        for node in preorder(root) {
            match node {
                Node::Definition(definition) => {
                    references.definitions.insert(
                        definition.identifier.clone(),
                        Definition {
                            url: definition.url.clone(),
                            title: definition.title.clone(),
                        },
                    );
                }
                Node::FootnoteReference(reference) => {
                    references.note_number(&reference.identifier);
                }
                _ => {}
            }
        }
        // A definition that is never referenced still needs a label to show.
        for node in preorder(root) {
            if let Node::FootnoteDefinition(definition) = node {
                references.note_number(&definition.identifier);
            }
        }
        references
    }

    fn note_number(&mut self, identifier: &str) -> usize {
        let next = self.notes.len() + 1;
        *self.notes.entry(identifier.to_owned()).or_insert(next)
    }

    pub fn definition(&self, identifier: &str) -> Option<&Definition> {
        self.definitions.get(identifier)
    }

    /// The 1-based label a footnote reference and its definition display.
    pub fn note(&self, identifier: &str) -> Option<usize> {
        self.notes.get(identifier).copied()
    }
}

/// Every node under `root`, itself first, in source order.
pub fn preorder(root: &Node) -> impl Iterator<Item = &Node> {
    let mut stack = vec![root];
    std::iter::from_fn(move || {
        let node = stack.pop()?;
        if let Some(children) = node.children() {
            stack.extend(children.iter().rev());
        }
        Some(node)
    })
}

/// Whether any code block is nested in `node`.
pub fn contains_code(node: &Node) -> bool {
    preorder(node).any(|node| matches!(node, Node::Code(_)))
}

/// The rendered text of one block, as the reader counts and searches it.
///
/// The counting rule follows the reader's earlier event stream: a paragraph
/// contributes a trailing newline, a heading does not, and each list item adds
/// its own. Keeping it stable avoids changing the status-bar count for the same
/// document.
pub fn block_text(node: &Node) -> String {
    let mut out = String::new();
    blocks(node, &mut out);
    out
}

fn blocks(node: &Node, out: &mut String) {
    match node {
        Node::Root(root) => {
            for child in &root.children {
                blocks(child, out);
            }
        }
        Node::Paragraph(paragraph) => {
            inlines(&paragraph.children, out);
            out.push('\n');
        }
        Node::Heading(heading) => inlines(&heading.children, out),
        Node::Blockquote(quote) => {
            for child in &quote.children {
                blocks(child, out);
            }
        }
        Node::List(list) => {
            for item in &list.children {
                let Node::ListItem(item) = item else {
                    continue;
                };
                for child in &item.children {
                    blocks(child, out);
                }
                out.push('\n');
            }
        }
        Node::Code(code) => out.push_str(&code.value),
        Node::Table(table) => {
            for row in &table.children {
                let Node::TableRow(row) = row else {
                    continue;
                };
                for cell in &row.children {
                    let Node::TableCell(cell) = cell else {
                        continue;
                    };
                    inlines(&cell.children, out);
                    out.push('\n');
                }
            }
        }
        Node::FootnoteDefinition(definition) => {
            for child in &definition.children {
                blocks(child, out);
            }
        }
        Node::ThematicBreak(_) => out.push('\n'),
        // Definitions, raw HTML and front matter carry no searchable words.
        _ => {}
    }
}

fn inlines(nodes: &[Node], out: &mut String) {
    for node in nodes {
        match node {
            Node::Text(text) => out.push_str(&text.value),
            Node::InlineCode(code) => out.push_str(&code.value),
            Node::InlineMath(math) => out.push_str(&math.value),
            Node::Break(_) => out.push('\n'),
            Node::Image(image) => out.push_str(&image.alt),
            Node::ImageReference(image) => out.push_str(&image.alt),
            Node::Html(_) | Node::FootnoteReference(_) => {}
            other => {
                if let Some(children) = other.children() {
                    inlines(children, out);
                }
            }
        }
    }
}

/// The heading's own text, without markers or surrounding whitespace.
pub fn heading_title(children: &[mdast::Node]) -> String {
    let mut out = String::new();
    inlines(children, &mut out);
    out.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(source: &str) -> Node {
        parse(source).expect("parses")
    }

    #[test]
    fn references_resolve_across_blocks_and_footnotes_number_in_order() {
        let parsed = root(
            "See [site][ref] and note[^b].\n\n[ref]: https://example.com \"t\"\n\n[^b]: later\n\n[^a]: earlier\n",
        );
        let references = References::new(&parsed);
        let definition = references.definition("ref").expect("resolved");
        assert_eq!(definition.url, "https://example.com");
        assert_eq!(definition.title.as_deref(), Some("t"));
        assert_eq!(references.note("b"), Some(1));
        assert_eq!(references.note("a"), Some(2));
    }

    #[test]
    fn impossible_container_nesting_is_refused_before_parsing() {
        for source in [
            format!("{}x\n", "> ".repeat(200)),
            format!("{}x\n", "- ".repeat(200)),
            format!(
                "{}x\n",
                ">>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>\n"
            ),
        ] {
            let error = parse(&source).expect_err("too deep");
            assert!(error.to_string().contains("64 levels"), "{error}");
        }
        assert!(parse("- - - three deep\n> > quoted twice\n").is_ok());
    }

    #[test]
    fn a_parse_above_the_ceiling_is_refused() {
        let error = parse(&"x".repeat(MAX_PARSE_BYTES + 1)).expect_err("oversized");
        assert!(error.to_string().contains("2 MiB"), "{error}");
    }

    #[test]
    fn block_text_keeps_the_reader_character_rule() {
        let parsed = root("# Title\n\nbody\n\n## Section\n\nmore\n");
        let text: String = parsed
            .children()
            .expect("children")
            .iter()
            .map(block_text)
            .collect();
        assert_eq!(text, "Titlebody\nSectionmore\n");
        assert_eq!(text.chars().count(), 22);
    }

    #[test]
    fn a_list_counts_its_item_end_and_nested_blocks_carry_code() {
        let parsed = root("- one\n- two\n");
        let text: String = parsed
            .children()
            .expect("children")
            .iter()
            .map(block_text)
            .collect();
        assert_eq!(text, "one\n\ntwo\n\n");
        let nested = root("- item\n  ```\n  x\n  ```\n");
        assert!(contains_code(&nested));
        assert!(!contains_code(&root("plain\n")));
    }
}
