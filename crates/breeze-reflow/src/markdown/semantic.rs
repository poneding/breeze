//! Editor semantics from one mdast parse: line kinds, block markers and inline
//! spans.
//!
//! The parser is the semantic authority for what the source *means*; the
//! projection remains the authority for how it is displayed. Keeping the two
//! apart is what lets the editor style reference links, nested emphasis,
//! character entities and setext headings correctly without ever rewriting the
//! document: every span below is a source range, never a serialized fragment.
//!
//! Runs on a worker. The caller materializes the source once and keeps the
//! returned value keyed by the editing revision that produced it.
use super::{
    BlockKind, MAX_DEPTH, RunStyle,
    ast::{self, References},
};
use anyhow::{Result, ensure};
use markdown::mdast::Node;
use std::{ops::Range, sync::Arc};

/// Bound on styled inline spans, generous for prose and hostile-input safe.
pub const MAX_SPANS: usize = 200_000;

/// A styled span with exact source geometry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    /// The whole construct, delimiters included; hidden unless revealed.
    pub full: Range<usize>,
    /// The visible content inside `full`.
    pub content: Range<usize>,
    /// Nesting depth, outermost first.
    pub depth: usize,
    pub style: RunStyle,
    /// Display text when it differs from the source bytes, as for `&amp;`.
    pub decoded: Option<Arc<str>>,
}

/// What a leading block marker is drawn as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkerKind {
    /// ATX heading hashes and the spaces after them.
    Heading,
    /// A list item's bullet or number, with the spaces after it.
    Bullet { ordered: bool },
}

/// The block marker that opens a source line, if any.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Marker {
    /// The marker bytes only: indentation before it is preserved.
    pub source: Range<usize>,
    pub kind: MarkerKind,
}

/// The block context of one source line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineKind {
    pub source: Range<usize>,
    /// The innermost structural or leaf block: heading, list, fence, table or
    /// paragraph. A paragraph inside a list item reports the list.
    pub kind: BlockKind,
    /// Enclosing block-quote markers, which stack independently of `kind`.
    pub quote_depth: usize,
    /// A setext underline carries no text of its own and has no display row.
    pub hidden: bool,
    /// The heading hash run or list bullet that opens this line, from the AST.
    pub marker: Option<Marker>,
}

#[derive(Clone, Debug, Default)]
pub struct Semantic {
    /// One entry per source line, in source order.
    pub lines: Vec<LineKind>,
    /// Styled spans, ordered outermost-first.
    pub spans: Vec<Span>,
}

impl Semantic {
    pub fn parse(source: &str) -> Result<Self> {
        let root = ast::parse(source)?;
        let references = References::new(&root);
        let mut builder = Builder {
            source,
            references,
            frames: Vec::new(),
            spans: Vec::new(),
        };
        for node in root.children().map(Vec::as_slice).unwrap_or_default() {
            builder.block(node, 0)?;
        }
        builder.finish()
    }
}

/// A block that classifies the lines it covers.
#[derive(Clone, Debug)]
struct BlockFrame {
    source: Range<usize>,
    kind: BlockKind,
    quote: bool,
    leaf: bool,
    /// The marker that opens the block on its first line.
    marker: Option<Marker>,
    /// The line start of a setext underline, which displays no text.
    underline: Option<usize>,
    /// The fence lines of a code block, which stay visible while editing.
    fence: Option<Fence>,
}

#[derive(Clone, Copy, Debug)]
struct Fence {
    opener: usize,
    closer: Option<usize>,
}

struct Builder<'a> {
    source: &'a str,
    references: References,
    frames: Vec<BlockFrame>,
    spans: Vec<Span>,
}

impl Builder<'_> {
    fn finish(mut self) -> Result<Semantic> {
        ensure!(
            self.frames.len() <= super::MAX_BLOCKS,
            "Markdown has too many blocks"
        );
        // Outermost first so a consumer can walk parents before children.
        self.spans.sort_by(|a, b| {
            (a.full.start, std::cmp::Reverse(a.full.end))
                .cmp(&(b.full.start, std::cmp::Reverse(b.full.end)))
        });
        self.assert_disjoint_siblings()?;
        let lines = lines_with_kinds(self.source, &self.frames);
        Ok(Semantic {
            lines,
            spans: self.spans,
        })
    }

    fn block(&mut self, node: &Node, depth: usize) -> Result<()> {
        ensure!(depth <= MAX_DEPTH, "Markdown nesting exceeds 64 levels");
        ensure!(
            self.frames.len() < super::MAX_BLOCKS,
            "Markdown has too many blocks"
        );
        match node {
            Node::Heading(heading) => {
                let Some(source) = ast::span(node) else {
                    return Ok(());
                };
                self.frames.push(BlockFrame {
                    marker: heading_marker(&source, &heading.children, self.source),
                    underline: setext_underline(&source, self.source),
                    source,
                    kind: BlockKind::Heading(heading.depth),
                    quote: false,
                    leaf: true,
                    fence: None,
                });
                self.inlines(&heading.children, depth, 0)?;
            }
            Node::Blockquote(quote) => {
                if let Some(source) = ast::span(node) {
                    self.frames.push(BlockFrame {
                        source,
                        kind: BlockKind::Paragraph,
                        quote: true,
                        leaf: false,
                        marker: None,
                        underline: None,
                        fence: None,
                    });
                }
                for child in &quote.children {
                    self.block(child, depth + 1)?;
                }
            }
            Node::List(list) => {
                if let Some(source) = ast::span(node) {
                    self.frames.push(BlockFrame {
                        source,
                        kind: BlockKind::List {
                            ordered: list.ordered,
                            depth: 0,
                        },
                        quote: false,
                        leaf: true,
                        marker: None,
                        underline: None,
                        fence: None,
                    });
                }
                for item in &list.children {
                    let Node::ListItem(item) = item else {
                        continue;
                    };
                    // The item carries only its marker; the list owns the kind,
                    // so nesting depth counts list frames alone.
                    if let Some(source) = item
                        .position
                        .as_ref()
                        .map(|at| at.start.offset..at.end.offset)
                        .filter(|span| span.start <= span.end)
                    {
                        self.frames.push(BlockFrame {
                            marker: item_marker(&source, self.source),
                            source,
                            kind: BlockKind::Paragraph,
                            quote: false,
                            leaf: false,
                            underline: None,
                            fence: None,
                        });
                    }
                    for child in &item.children {
                        self.block(child, depth + 1)?;
                    }
                }
            }
            Node::Code(_) => {
                if let Some(source) = ast::span(node) {
                    self.frames.push(BlockFrame {
                        fence: fence_lines(&source, self.source),
                        marker: None,
                        underline: None,
                        source,
                        kind: BlockKind::Code,
                        quote: false,
                        leaf: true,
                    });
                }
            }
            Node::Table(table) => {
                if let Some(source) = ast::span(node) {
                    self.frames.push(BlockFrame {
                        source,
                        kind: BlockKind::Table,
                        quote: false,
                        leaf: true,
                        marker: None,
                        underline: None,
                        fence: None,
                    });
                }
                for row in &table.children {
                    let Node::TableRow(row) = row else {
                        continue;
                    };
                    for cell in &row.children {
                        let Node::TableCell(cell) = cell else {
                            continue;
                        };
                        self.inlines(&cell.children, depth + 1, 0)?;
                    }
                }
            }
            Node::Paragraph(paragraph) => {
                if let Some(source) = ast::span(node) {
                    self.frames.push(BlockFrame {
                        source,
                        kind: BlockKind::Paragraph,
                        quote: false,
                        leaf: false,
                        marker: None,
                        underline: None,
                        fence: None,
                    });
                }
                self.inlines(&paragraph.children, depth, 0)?;
            }
            Node::FootnoteDefinition(definition) => {
                for child in &definition.children {
                    self.block(child, depth + 1)?;
                }
            }
            // Rules, raw HTML, definitions and front matter have no editable
            // structure; their lines keep the surrounding paragraph context.
            Node::ThematicBreak(_)
            | Node::Html(_)
            | Node::Definition(_)
            | Node::Yaml(_)
            | Node::Toml(_)
            | Node::Math(_) => {}
            other => {
                if let Some(children) = other.children() {
                    for child in children {
                        self.block(child, depth + 1)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn inlines(&mut self, nodes: &[Node], block_depth: usize, style_depth: usize) -> Result<()> {
        for node in nodes {
            self.inline(node, block_depth, style_depth)?;
        }
        Ok(())
    }

    fn inline(&mut self, node: &Node, block_depth: usize, style_depth: usize) -> Result<()> {
        match node {
            Node::Text(text) => {
                if let Some(span) = ast::span(node) {
                    self.note_text(span, &text.value);
                }
            }
            Node::InlineCode(code) => {
                if let Some(span) = ast::span(node) {
                    let content = code_content(&span, self.source);
                    self.push_span(
                        span,
                        content,
                        RunStyle {
                            code: true,
                            ..Default::default()
                        },
                        style_depth,
                        Some(Arc::from(code.value.as_str())),
                    )?;
                }
            }
            Node::Strong(strong) => {
                self.styled(node, &strong.children, bold(), block_depth, style_depth)?;
            }
            Node::Emphasis(emphasis) => {
                self.styled(node, &emphasis.children, italic(), block_depth, style_depth)?;
            }
            Node::Delete(delete) => {
                self.styled(node, &delete.children, strike(), block_depth, style_depth)?;
            }
            Node::Link(link) => {
                self.styled(node, &link.children, link_style(), block_depth, style_depth)?;
            }
            Node::LinkReference(reference) => {
                if self.references.definition(&reference.identifier).is_some() {
                    self.styled(
                        node,
                        &reference.children,
                        link_style(),
                        block_depth,
                        style_depth,
                    )?;
                } else {
                    // An unresolved reference is literal source: keep its inner
                    // emphasis but never hide the brackets around it.
                    self.inlines(&reference.children, block_depth, style_depth)?;
                }
            }
            // Images, raw HTML, math, breaks and footnote references stay
            // literal; an image's alt text is not editable inline content.
            Node::Image(_)
            | Node::ImageReference(_)
            | Node::Html(_)
            | Node::InlineMath(_)
            | Node::Break(_)
            | Node::FootnoteReference(_) => {}
            other => {
                if let Some(children) = other.children() {
                    self.inlines(children, block_depth, style_depth)?;
                }
            }
        }
        Ok(())
    }

    fn styled(
        &mut self,
        node: &Node,
        children: &[Node],
        style: RunStyle,
        block_depth: usize,
        style_depth: usize,
    ) -> Result<()> {
        let Some(full) = ast::span(node) else {
            return self.inlines(children, block_depth, style_depth);
        };
        let content = children_span(children).unwrap_or_else(|| full.clone());
        self.push_span(full, content, style, style_depth, None)?;
        self.inlines(children, block_depth, style_depth + 1)
    }

    fn push_span(
        &mut self,
        full: Range<usize>,
        content: Range<usize>,
        style: RunStyle,
        depth: usize,
        decoded: Option<Arc<str>>,
    ) -> Result<()> {
        ensure!(self.spans.len() < MAX_SPANS, "Too many styled spans");
        let content = if content.start <= content.end
            && content.start >= full.start
            && content.end <= full.end
        {
            content
        } else {
            full.clone()
        };
        self.spans.push(Span {
            full,
            content,
            depth,
            style,
            decoded,
        });
        Ok(())
    }

    /// Record escaped and entity-decoded runs inside one text node.
    ///
    /// The parser decodes character references and escapes into the node's
    /// value, so the source bytes and the display text differ. Mapping the two
    /// back together needs the exact source range of each decoded run; a whole
    /// node would make a caret next to `&amp;` land at the wrong offset.
    fn note_text(&mut self, range: Range<usize>, value: &str) {
        let Some(slice) = self.source.get(range.clone()) else {
            return;
        };
        if slice == value {
            return;
        }
        let Some(runs) = decoded_runs(self.source, &range, value) else {
            // No per-run mapping: substitute the whole node. This keeps the
            // text correct at the cost of coarse caret mapping, which is safer
            // than guessing.
            self.push_decoded(range.clone(), value);
            return;
        };
        for (source, decoded) in runs {
            self.push_decoded(source, &decoded);
        }
    }

    fn push_decoded(&mut self, source: Range<usize>, decoded: &str) {
        if self.spans.len() >= MAX_SPANS || decoded.is_empty() {
            return;
        }
        self.spans.push(Span {
            full: source.clone(),
            content: source,
            depth: 0,
            style: RunStyle::default(),
            decoded: Some(Arc::from(decoded)),
        });
    }

    /// Sibling spans must not overlap: an overlap would make the display
    /// mapping ambiguous, so it is treated as a parser contract violation.
    fn assert_disjoint_siblings(&self) -> Result<()> {
        let mut stack: Vec<Range<usize>> = Vec::new();
        for span in &self.spans {
            while let Some(open) = stack.last() {
                if open.end <= span.full.start {
                    stack.pop();
                } else {
                    break;
                }
            }
            if let Some(open) = stack.last() {
                ensure!(
                    span.full.end <= open.end,
                    "Nested Markdown spans must be properly nested"
                );
            }
            stack.push(span.full.clone());
        }
        Ok(())
    }
}

fn bold() -> RunStyle {
    RunStyle {
        bold: true,
        ..Default::default()
    }
}

fn italic() -> RunStyle {
    RunStyle {
        italic: true,
        ..Default::default()
    }
}

fn strike() -> RunStyle {
    RunStyle {
        strike: true,
        ..Default::default()
    }
}

fn link_style() -> RunStyle {
    RunStyle {
        link: true,
        ..Default::default()
    }
}

/// The source range that covers every child with a position.
fn children_span(children: &[Node]) -> Option<Range<usize>> {
    let mut start: Option<usize> = None;
    let mut end: Option<usize> = None;
    for child in children {
        let Some(span) = ast::span(child) else {
            continue;
        };
        start = Some(start.map_or(span.start, |start| start.min(span.start)));
        end = Some(end.map_or(span.end, |end| end.max(span.end)));
    }
    match (start, end) {
        (Some(start), Some(end)) if start <= end => Some(start..end),
        _ => None,
    }
}

/// The ATX hash run and the spaces after it, or nothing for a setext heading.
fn heading_marker(span: &Range<usize>, children: &[Node], source: &str) -> Option<Marker> {
    let text = source.get(span.clone())?;
    if !text.trim_start_matches([' ', '\t']).starts_with('#') {
        return None;
    }
    let end = match children_span(children) {
        Some(content) if !source[span.start..content.start].contains(['\n', '\r']) => content.start,
        // A heading with no inline content hides its whole written form.
        _ => span.end,
    };
    let end = end.clamp(span.start, span.end);
    Some(Marker {
        source: span.start..end,
        kind: MarkerKind::Heading,
    })
}

/// The setext underline's line start, which draws no row of its own.
fn setext_underline(span: &Range<usize>, source: &str) -> Option<usize> {
    let text = source.get(span.clone())?;
    if text.trim_start_matches([' ', '\t']).starts_with('#') || span.end <= span.start {
        return None;
    }
    let start = line_start(source, span.end - 1);
    if start <= span.start {
        return None;
    }
    let (end, _) = line_end(source, start);
    let underline = source.get(start..end)?.trim();
    let first = underline.chars().next()?;
    (matches!(first, '=' | '-') && underline.chars().all(|c| c == first)).then_some(start)
}

/// The bullet or number that opens a list item, without its indentation.
fn item_marker(span: &Range<usize>, source: &str) -> Option<Marker> {
    let (line_end, _) = line_end(source, span.start);
    let end = line_end.min(span.end).max(span.start);
    let text = source.get(span.start..end)?;
    let bytes = text.as_bytes();
    let (marker_len, ordered) = match bytes.first()? {
        b'-' | b'+' | b'*' => (1, false),
        byte if byte.is_ascii_digit() => {
            let digits = bytes
                .iter()
                .take_while(|byte| byte.is_ascii_digit())
                .count();
            if digits == 0 || digits > 9 {
                return None;
            }
            match bytes.get(digits)? {
                b'.' | b')' => (digits + 1, true),
                _ => return None,
            }
        }
        _ => return None,
    };
    let mut end = marker_len;
    while matches!(bytes.get(end), Some(b' ' | b'\t')) {
        end += 1;
    }
    if end == marker_len && text.len() > marker_len {
        // A marker must be followed by whitespace or end the line.
        return None;
    }
    Some(Marker {
        source: span.start..(span.start + end),
        kind: MarkerKind::Bullet { ordered },
    })
}

/// The fence lines of a fenced code block; indented code has none.
fn fence_lines(span: &Range<usize>, source: &str) -> Option<Fence> {
    let opener = line_start(source, span.start);
    let (end, _) = line_end(source, span.start);
    let text = source.get(span.start..end)?;
    let marker = text.chars().next()?;
    if !matches!(marker, '`' | '~') {
        return None;
    }
    let count = text.chars().take_while(|c| *c == marker).count();
    if count < 3 || span.end <= span.start {
        return None;
    }
    let last = line_start(source, span.end - 1);
    if last <= span.start {
        return Some(Fence {
            opener,
            closer: None,
        });
    }
    let (last_end, _) = line_end(source, last);
    let closing = source.get(last..last_end)?.trim();
    let closing_count = closing.chars().take_while(|c| *c == marker).count();
    let closer =
        (closing_count >= count && closing[closing_count..].trim().is_empty()).then_some(last);
    Some(Fence { opener, closer })
}

/// The source range inside an inline code span's backticks.
fn code_content(span: &Range<usize>, source: &str) -> Range<usize> {
    let Some(slice) = source.get(span.clone()) else {
        return span.clone();
    };
    let open = slice.chars().take_while(|c| *c == '`').count();
    let inner = slice.get(open..).unwrap_or_default();
    let close = inner.chars().rev().take_while(|c| *c == '`').count();
    let start = span.start + open;
    let end = span.end.saturating_sub(close);
    if start <= end && end <= span.end {
        start..end
    } else {
        span.clone()
    }
}

/// Decode one text node into copied and replaced runs, in source order.
///
/// `None` means the source and the parser's value do not line up run by run.
fn decoded_runs(
    source: &str,
    range: &Range<usize>,
    value: &str,
) -> Option<Vec<(Range<usize>, String)>> {
    let mut runs = Vec::new();
    let mut index = range.start;
    let mut value_at = 0usize;
    while index < range.end {
        let rest = source.get(index..range.end)?;
        let character = rest.chars().next()?;
        // The parser keeps the document's own line endings in the text value;
        // a normalized `\n` is accepted too so this never depends on that.
        if character == '\r' {
            let windows = rest.as_bytes().get(1) == Some(&b'\n');
            let rest_value = &value[value_at..];
            let (source_length, value_length) = if windows {
                if rest_value.starts_with("\r\n") {
                    (2, 2)
                } else if rest_value.starts_with('\n') {
                    (2, 1)
                } else {
                    return None;
                }
            } else if rest_value.starts_with(['\r', '\n']) {
                (1, 1)
            } else {
                return None;
            };
            index += source_length;
            value_at += value_length;
            continue;
        }
        if character == '\\'
            && let Some(escaped) = rest[1..].chars().next()
            && escaped.is_ascii_punctuation()
        {
            let length = 1 + escaped.len_utf8();
            if !value[value_at..].starts_with(escaped) {
                return None;
            }
            runs.push((index..index + length, escaped.to_string()));
            index += length;
            value_at += escaped.len_utf8();
            continue;
        }
        if character == '&'
            && let Some((length, decoded)) = character_reference(rest)
        {
            if !value[value_at..].starts_with(&decoded) {
                return None;
            }
            runs.push((index..index + length, decoded.clone()));
            index += length;
            value_at += decoded.len();
            continue;
        }
        if !value[value_at..].starts_with(character) {
            return None;
        }
        index += character.len_utf8();
        value_at += character.len_utf8();
    }
    (value_at == value.len()).then_some(runs)
}

/// A character reference as `markdown-rs` decodes it: `(length, text)`.
fn character_reference(rest: &str) -> Option<(usize, String)> {
    let semicolon = rest.find(';')?;
    if semicolon > 33 {
        return None;
    }
    let name = rest.get(1..semicolon)?;
    let decoded = if let Some(hex) = name.strip_prefix("#x").or_else(|| name.strip_prefix("#X")) {
        if hex.is_empty() || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        markdown::decode_numeric(hex, 16)
    } else if let Some(decimal) = name.strip_prefix('#') {
        if decimal.is_empty() || !decimal.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        markdown::decode_numeric(decimal, 10)
    } else {
        markdown::decode_named(name, true)?
    };
    (!decoded.is_empty()).then_some((semicolon + 1, decoded))
}

/// Resolve every source line's block context from the parsed block frames.
///
/// One sweep over lines and frames: a document with 200 000 lines and 40 000
/// blocks must not cost their product, so frames are pushed when the sweep
/// reaches them and dropped when they end.
fn lines_with_kinds(source: &str, blocks: &[BlockFrame]) -> Vec<LineKind> {
    // Pre-order: a parent starts before its children, so pushing in this order
    // leaves the innermost frame last, which is what the leaf lookup needs.
    let mut ordered: Vec<&BlockFrame> = blocks.iter().collect();
    ordered.sort_by_key(|frame| (frame.source.start, std::cmp::Reverse(frame.source.end)));

    let mut lines = Vec::new();
    let mut frames: Vec<&BlockFrame> = Vec::new();
    let mut pending = 0usize;
    let mut start = 0usize;
    while start <= source.len() {
        let (end, next) = line_end(source, start);
        if start == source.len() && (end != start || !lines.is_empty()) {
            break;
        }
        frames.retain(|frame| frame.source.end > start);
        while pending < ordered.len() && ordered[pending].source.start < end {
            if ordered[pending].source.end > start {
                frames.push(ordered[pending]);
            }
            pending += 1;
        }
        let leaf = frames.iter().rev().find(|frame| frame.leaf).copied();
        let mut kind = leaf
            .map(|frame| frame.kind.clone())
            .unwrap_or(BlockKind::Paragraph);
        if let BlockKind::List { depth, .. } = &mut kind {
            *depth = frames
                .iter()
                .filter(|frame| matches!(frame.kind, BlockKind::List { .. }))
                .count()
                .saturating_sub(1)
                .min(8);
        }
        if let Some(fence) = leaf.and_then(|frame| frame.fence.as_ref()) {
            kind = if start == fence.opener || Some(start) == fence.closer {
                BlockKind::Fence
            } else {
                BlockKind::Code
            };
        }
        let hidden = frames.iter().any(|frame| frame.underline == Some(start));
        let marker = frames
            .iter()
            .filter_map(|frame| frame.marker.as_ref())
            .filter(|marker| marker.source.start >= start && marker.source.start < end)
            .min_by_key(|marker| marker.source.start)
            .cloned();
        lines.push(LineKind {
            source: start..end,
            kind,
            quote_depth: frames.iter().filter(|frame| frame.quote).count(),
            hidden,
            marker,
        });
        if next == start {
            break;
        }
        start = next;
    }
    lines
}

/// The line terminator: `\r\n`, `\n`, `\r` or the end of the document.
pub(crate) fn line_end(source: &str, start: usize) -> (usize, usize) {
    let bytes = source.as_bytes();
    let mut index = start;
    while index < bytes.len() {
        match bytes[index] {
            b'\n' => return (index, index + 1),
            b'\r' => {
                return if bytes.get(index + 1) == Some(&b'\n') {
                    (index, index + 2)
                } else {
                    (index, index + 1)
                };
            }
            _ => index += 1,
        }
    }
    (bytes.len(), bytes.len())
}

/// The start of the line containing `offset`.
fn line_start(source: &str, offset: usize) -> usize {
    source.as_bytes()[..offset.min(source.len())]
        .iter()
        .rposition(|byte| matches!(byte, b'\n' | b'\r'))
        .map_or(0, |index| index + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn semantic(source: &str) -> Semantic {
        Semantic::parse(source).expect("parses")
    }

    #[test]
    fn markers_come_from_the_ast_and_leave_task_checks_visible() {
        let parsed = semantic("# Title\n\n- item\n- [x] done\n\n3. three\n");
        let line = &parsed.lines[0];
        assert_eq!(line.kind, BlockKind::Heading(1));
        assert_eq!(line.marker.as_ref().unwrap().source, 0..2);
        assert_eq!(
            parsed.lines[3].marker.as_ref().unwrap().source,
            16..18,
            "the bullet hides, the checkbox stays"
        );
        assert_eq!(
            parsed.lines[3].marker.as_ref().unwrap().kind,
            MarkerKind::Bullet { ordered: false }
        );
        assert_eq!(
            parsed.lines[5].marker.as_ref().unwrap().kind,
            MarkerKind::Bullet { ordered: true }
        );
        assert!(
            parsed.lines[1].marker.is_none(),
            "a blank line has no marker"
        );
    }

    #[test]
    fn fences_and_setext_underlines_are_recognized() {
        let parsed = semantic("Title\n=====\n\n```rs\nlet x = 1;\n```\n");
        assert_eq!(parsed.lines[0].kind, BlockKind::Heading(1));
        assert!(parsed.lines[1].hidden);
        assert_eq!(parsed.lines[3].kind, BlockKind::Fence);
        assert_eq!(parsed.lines[4].kind, BlockKind::Code);
        assert_eq!(parsed.lines[5].kind, BlockKind::Fence);
    }

    #[test]
    fn entities_and_escapes_keep_their_own_source_ranges() {
        let parsed = semantic("a &amp; b \\* c\n");
        let decoded: Vec<(Range<usize>, &str)> = parsed
            .spans
            .iter()
            .filter_map(|span| {
                span.decoded
                    .as_deref()
                    .map(|text| (span.full.clone(), text))
            })
            .collect();
        assert_eq!(
            decoded,
            [(2..7, "&"), (10..12, "*")],
            "each decoded run maps back to its own bytes"
        );
    }

    #[test]
    fn nested_emphasis_and_resolved_references_get_nested_spans() {
        let parsed = semantic("x **b *i*** and [see][r]\n\n[r]: https://example.com\n");
        let bold = parsed
            .spans
            .iter()
            .find(|span| span.style.bold)
            .expect("bold span");
        assert_eq!(&"x **b *i*** and [see][r]"[bold.full.clone()], "**b *i***");
        let italic = parsed
            .spans
            .iter()
            .find(|span| span.style.italic)
            .expect("italic span");
        assert_eq!(italic.depth, 1);
        let link = parsed
            .spans
            .iter()
            .find(|span| span.style.link)
            .expect("link span");
        assert_eq!(link.content, 17..20);
    }
}
