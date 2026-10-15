//! One pull-parse per source revision; complete semantic blocks can be laid out lazily.
use crate::{fingerprint, read_bounded};
use anyhow::{Context, Result, ensure};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd, html};
use std::{collections::HashMap, ops::Range, path::Path, sync::Arc};

pub const MAX_SOURCE_BYTES: usize = 16 * 1024 * 1024;
const MAX_RENDER_BYTES: usize = 64 * 1024 * 1024;
const MAX_BLOCKS: usize = 100_000;
const MAX_DEPTH: usize = 64;

#[derive(Clone, Debug)]
pub struct Block {
    pub source: Range<usize>,
    pub html: Arc<str>,
    pub text: Arc<str>,
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
        let options = Options::ENABLE_TABLES
            | Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_TASKLISTS
            | Options::ENABLE_FOOTNOTES
            | Options::ENABLE_HEADING_ATTRIBUTES;
        let mut document = Self {
            blocks: Vec::new(),
            headings: Vec::new(),
            source_bytes: source.len(),
            characters: 0,
            fingerprint: fingerprint(source.as_bytes()),
        };
        let mut events = Vec::new();
        let mut text = String::new();
        let mut depth = 0usize;
        let mut start = 0;
        let mut heading = None;
        let mut anchors = HashMap::<String, usize>::new();
        let mut render_bytes = 0;
        // References are resolved by the whole-source parser before events are split.
        for (event, range) in Parser::new_ext(source, options).into_offset_iter() {
            if events.is_empty() {
                start = range.start;
            }
            match &event {
                Event::Start(tag) => {
                    depth += 1;
                    ensure!(depth <= MAX_DEPTH, "Markdown nesting exceeds 64 levels");
                    if let Tag::Heading { level, id, .. } = tag {
                        heading = Some((*level as usize, id.as_ref().map(ToString::to_string)));
                    }
                }
                Event::End(_) => depth = depth.saturating_sub(1),
                Event::Text(value)
                | Event::Code(value)
                | Event::InlineMath(value)
                | Event::DisplayMath(value) => text.push_str(value),
                Event::SoftBreak | Event::HardBreak | Event::Rule => text.push('\n'),
                _ => {}
            }
            if matches!(
                event,
                Event::End(TagEnd::Paragraph | TagEnd::Item | TagEnd::TableCell)
            ) {
                text.push('\n');
            }
            events.push(event);
            if depth != 0 {
                continue;
            }
            if let Some((level, explicit)) = heading.take() {
                let base = explicit.unwrap_or_else(|| slug(text.trim()));
                let count = anchors.entry(base.clone()).or_default();
                let anchor = if *count == 0 {
                    base
                } else {
                    format!("{base}-{count}")
                };
                *count += 1;
                document.headings.push(Heading {
                    title: text.trim().to_owned(),
                    level,
                    block: document.blocks.len(),
                    anchor,
                });
            }
            let mut output = String::new();
            html::push_html(&mut output, events.drain(..));
            render_bytes += output.len() + text.len();
            ensure!(
                render_bytes <= MAX_RENDER_BYTES,
                "Markdown exceeds the 64 MiB render-text budget"
            );
            ensure!(
                document.blocks.len() < MAX_BLOCKS,
                "Markdown has too many blocks"
            );
            document.characters += text.chars().count();
            document.blocks.push(Block {
                source: start..range.end,
                html: output.into(),
                text: std::mem::take(&mut text).into(),
            });
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
    pub fn resolve_image(document: &Path, uri: &str) -> Result<std::path::PathBuf> {
        let target = uri.split(['?', '#']).next().unwrap_or(uri).trim();
        ensure!(!target.is_empty(), "The image reference is empty");
        ensure!(
            !target.contains(':') && !target.starts_with("//") && !Path::new(target).is_absolute(),
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
    fn image_references_follow_markdown_uri_rules() {
        let document = Path::new("/notes/readme.md");
        assert_eq!(
            Document::resolve_image(document, "shots/first.png").unwrap(),
            std::path::PathBuf::from("/notes/shots/first.png")
        );
        assert_eq!(
            Document::resolve_image(document, "my%20shot.png").unwrap(),
            std::path::PathBuf::from("/notes/my shot.png")
        );
        assert_eq!(
            Document::resolve_image(document, "chart.png?raw=1#page=2").unwrap(),
            std::path::PathBuf::from("/notes/chart.png")
        );
        assert_eq!(
            Document::resolve_image(document, "%E5%9B%BE.png").unwrap(),
            std::path::PathBuf::from("/notes/图.png")
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
    }
}
