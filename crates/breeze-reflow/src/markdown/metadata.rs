//! Reading metadata derived from Markdown source without owning its rendering.
//!
//! GPUI Kit's `TextView` parses and renders the document. Breeze still needs a
//! bounded source scan for the outline, block starts for reading progress,
//! character counts, and relative-image resolution. This module deliberately
//! retains no HTML, syntax tree, or rendering payload.

use super::{MAX_DEPTH, ast};
use crate::{fingerprint, read_bounded};
use anyhow::{Context, Result, ensure};
use markdown::mdast::Node;
use std::{
    collections::HashMap,
    ops::Range,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Heading {
    pub title: String,
    pub level: usize,
    /// Top-level display block containing the heading.
    pub block: usize,
    /// Byte range of the heading node in the Markdown source.
    pub source: Range<usize>,
    pub anchor: String,
}

#[derive(Clone, Debug)]
pub struct Metadata {
    pub headings: Vec<Heading>,
    /// Source offset of each top-level block, in TextView block order.
    pub block_starts: Vec<usize>,
    /// Full source range of each top-level block.
    pub block_ranges: Vec<Range<usize>>,
    /// Link and image definitions used to resolve reference-style images.
    pub image_definitions: HashMap<String, (String, Option<String>)>,
    pub source_bytes: usize,
    pub characters: usize,
    pub fingerprint: String,
}

impl Metadata {
    pub fn open(path: &Path) -> Result<Self> {
        let bytes = read_bounded(path, super::MAX_SOURCE_BYTES)?;
        let source = std::str::from_utf8(&bytes)?;
        Self::parse(source)
    }

    pub fn parse(source: &str) -> Result<Self> {
        ensure!(
            source.len() <= super::MAX_SOURCE_BYTES,
            "Markdown exceeds the 16 MiB input limit"
        );
        let root = ast::parse(source)?;
        check_depth(&root)?;

        let mut headings = Vec::new();
        let mut block_starts = Vec::new();
        let mut block_ranges = Vec::new();
        let mut image_definitions = HashMap::new();
        let mut anchors = HashMap::<String, usize>::new();
        for (block, node) in root.children().into_iter().flatten().enumerate() {
            let span = ast::span(node).context("A Markdown block has no position")?;
            block_starts.push(span.start);
            block_ranges.push(span.clone());
            if let Node::Definition(definition) = node {
                image_definitions.insert(
                    definition.identifier.clone(),
                    (definition.url.clone(), definition.title.clone()),
                );
            }
            collect_headings(node, block, source.len(), &mut headings, &mut anchors)?;
        }

        Ok(Self {
            headings,
            block_starts,
            block_ranges,
            image_definitions,
            source_bytes: source.len(),
            characters: ast::block_text(&root).chars().count(),
            fingerprint: fingerprint(source.as_bytes()),
        })
    }

    pub fn block_at_offset(&self, offset: usize) -> usize {
        self.block_starts
            .partition_point(|start| *start <= offset)
            .saturating_sub(1)
            .min(self.block_starts.len().saturating_sub(1))
    }

    pub fn anchor_block(&self, anchor: &str) -> Option<usize> {
        self.headings
            .iter()
            .find(|heading| heading.anchor == anchor)
            .map(|heading| heading.block)
    }
}

/// Resolve a Markdown image reference against the document's directory.
///
/// A Markdown image target is a URL, not a filesystem path: editors write
/// spaces as `%20`, and a target may carry a query or fragment.
pub fn resolve_image(document: &Path, uri: &str) -> Result<PathBuf> {
    let target = uri.split(['?', '#']).next().unwrap_or(uri).trim();
    ensure!(!target.is_empty(), "The image reference is empty");
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

fn collect_headings(
    node: &Node,
    block: usize,
    source_len: usize,
    headings: &mut Vec<Heading>,
    anchors: &mut HashMap<String, usize>,
) -> Result<()> {
    for node in ast::preorder(node) {
        let Node::Heading(heading) = node else {
            continue;
        };
        let title = ast::heading_title(&heading.children);
        if title.is_empty() {
            continue;
        }
        let base = slug(&title);
        let count = anchors.entry(base.clone()).or_default();
        let anchor = if *count == 0 {
            base
        } else {
            format!("{base}-{count}")
        };
        *count += 1;
        let source = ast::span(node).context("A Markdown heading has no position")?;
        ensure!(source.end <= source_len, "Heading points past the source");
        headings.push(Heading {
            title,
            level: usize::from(heading.depth),
            block,
            source,
            anchor,
        });
    }
    Ok(())
}

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

fn slug(title: &str) -> String {
    title
        .to_lowercase()
        .chars()
        .filter_map(|character| {
            if character.is_alphanumeric() || character == '_' || character == '-' {
                Some(character)
            } else if character.is_whitespace() {
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
    fn metadata_tracks_headings_blocks_and_characters() {
        let source = "# Title\n\nbody\n\n## Section\n\nmore\n";
        let metadata = Metadata::parse(source).unwrap();
        assert_eq!(metadata.block_starts, vec![0, 9, 15, 27]);
        assert_eq!(metadata.block_ranges[1], 9..13);
        assert_eq!(metadata.headings.len(), 2);
        assert_eq!(metadata.headings[1].title, "Section");
        assert_eq!(metadata.headings[1].level, 2);
        assert_eq!(metadata.headings[1].source, 15..25);
        assert_eq!(
            metadata.characters,
            "Titlebody\nSectionmore\n".chars().count()
        );
    }

    #[test]
    fn duplicate_headings_get_stable_anchors() {
        let metadata = Metadata::parse("# Same\n\n# Same\n").unwrap();
        assert_eq!(metadata.headings[0].anchor, "same");
        assert_eq!(metadata.headings[1].anchor, "same-1");
    }

    #[test]
    fn image_resolution_handles_url_encoding_and_query_strings() {
        let document = Path::new("/notes/entry.md");
        assert_eq!(
            resolve_image(document, "shots/my%20shot.png?raw=1#page=2").unwrap(),
            PathBuf::from("/notes/shots/my shot.png")
        );
    }
}
