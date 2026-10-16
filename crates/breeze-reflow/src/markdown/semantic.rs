//! One bounded pull-parse per source revision: line kinds and inline spans.
//!
//! The parser is the semantic authority for what the source *means*; the
//! projection remains the authority for how it is displayed. Keeping the two
//! apart is what lets the editor style reference links, nested emphasis,
//! character entities and setext headings correctly without ever rewriting the
//! document: every span below is a source range, never a serialized fragment.
//!
//! Runs on a worker. The caller materializes the source once and keeps the
//! returned value keyed by the editing revision that produced it.
use super::{BlockKind, RunStyle};
use anyhow::{Result, ensure};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::{ops::Range, sync::Arc};

/// Bound on styled inline spans, generous for prose and hostile-input safe.
pub const MAX_SPANS: usize = 200_000;
/// Bound on the block frames one document may nest.
const MAX_FRAMES: usize = 512;

/// The reader's shared CommonMark extension set.
///
/// One function so the reader, the editor and the worker cannot drift: a
/// document parsed for reading and the same document parsed while editing must
/// agree on what a construct means.
pub fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_HEADING_ATTRIBUTES
}

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
        let mut builder = Builder {
            source,
            frames: Vec::new(),
            blocks: Vec::new(),
            spans: Vec::new(),
        };
        builder.run()?;
        Ok(Self {
            lines: lines_with_kinds(source, &builder.blocks),
            spans: builder.spans,
        })
    }
}

struct Frame {
    tag: FrameTag,
    start: usize,
}

enum FrameTag {
    /// A block that classifies the lines it covers.
    Block {
        kind: BlockKind,
        quote: bool,
        leaf: bool,
    },
    /// An inline style that produces a span.
    Inline {
        style: RunStyle,
        content: Option<Range<usize>>,
    },
    /// A construct the projection renders literally, such as an image.
    Ignored,
}

/// A resolved block frame: `source` covers every line the block touches.
struct BlockFrame {
    source: Range<usize>,
    kind: BlockKind,
    quote: bool,
    leaf: bool,
}

struct Builder<'a> {
    source: &'a str,
    frames: Vec<Frame>,
    blocks: Vec<BlockFrame>,
    spans: Vec<Span>,
}

impl Builder<'_> {
    fn run(&mut self) -> Result<()> {
        for (event, range) in Parser::new_ext(self.source, options()).into_offset_iter() {
            match event {
                Event::Start(tag) => {
                    ensure!(
                        self.frames.len() < MAX_FRAMES,
                        "Markdown nesting exceeds {MAX_FRAMES} frames"
                    );
                    let frame = match &tag {
                        Tag::Heading { level, .. } => Frame {
                            tag: FrameTag::Block {
                                kind: BlockKind::Heading(*level as u8),
                                quote: false,
                                leaf: true,
                            },
                            start: range.start,
                        },
                        Tag::BlockQuote(_) => Frame {
                            tag: FrameTag::Block {
                                kind: BlockKind::Paragraph,
                                quote: true,
                                leaf: false,
                            },
                            start: range.start,
                        },
                        Tag::List(start) => Frame {
                            tag: FrameTag::Block {
                                kind: BlockKind::List {
                                    ordered: start.is_some(),
                                    depth: 0,
                                },
                                quote: false,
                                leaf: true,
                            },
                            start: range.start,
                        },
                        Tag::CodeBlock(_) => Frame {
                            tag: FrameTag::Block {
                                kind: BlockKind::Code,
                                quote: false,
                                leaf: true,
                            },
                            start: range.start,
                        },
                        Tag::Table(_) => Frame {
                            tag: FrameTag::Block {
                                kind: BlockKind::Table,
                                quote: false,
                                leaf: true,
                            },
                            start: range.start,
                        },
                        Tag::Paragraph => Frame {
                            tag: FrameTag::Block {
                                kind: BlockKind::Paragraph,
                                quote: false,
                                leaf: false,
                            },
                            start: range.start,
                        },
                        Tag::Emphasis => inline_frame(range.start, italic()),
                        Tag::Strong => inline_frame(range.start, bold()),
                        Tag::Strikethrough => inline_frame(range.start, strike()),
                        Tag::Link { .. } => inline_frame(range.start, link()),
                        Tag::Image { .. } => Frame {
                            tag: FrameTag::Ignored,
                            start: range.start,
                        },
                        _ => Frame {
                            tag: FrameTag::Ignored,
                            start: range.start,
                        },
                    };
                    self.frames.push(frame);
                }
                Event::End(tag) => {
                    if let Some(frame) = self.frames.pop() {
                        match frame.tag {
                            FrameTag::Block { kind, quote, leaf } => {
                                let end = if matches!(tag, TagEnd::CodeBlock) {
                                    // The fenced code block's own range covers its
                                    // fences; the closing fence's newline is part
                                    // of the block, so the range is kept whole.
                                    range.end
                                } else {
                                    range.end
                                };
                                self.blocks.push(BlockFrame {
                                    source: frame.start..end,
                                    kind,
                                    quote,
                                    leaf,
                                });
                            }
                            FrameTag::Inline { style, content } => {
                                if let Some(content) = content {
                                    ensure!(self.spans.len() < MAX_SPANS, "Too many spans");
                                    self.spans.push(Span {
                                        full: frame.start..range.end,
                                        content,
                                        depth: 0,
                                        style,
                                        decoded: None,
                                    });
                                }
                            }
                            FrameTag::Ignored => {}
                        }
                    }
                }
                Event::Text(text) => self.note_text(range, &text),
                Event::Code(text) => {
                    // A code span is a single event, so its content is derived
                    // from the delimiter run rather than from an inner frame.
                    let run = self.source[range.start..]
                        .chars()
                        .take_while(|c| *c == '`')
                        .count();
                    let content = range.start + run..range.start + run + text.len();
                    if content.end <= range.end {
                        ensure!(self.spans.len() < MAX_SPANS, "Too many spans");
                        self.spans.push(Span {
                            full: range.clone(),
                            content,
                            depth: 0,
                            style: RunStyle {
                                code: true,
                                ..Default::default()
                            },
                            decoded: None,
                        });
                    }
                }
                // Raw HTML stays literal: the projection shows its source bytes.
                Event::Html(_) | Event::InlineHtml(_) => {}
                _ => {}
            }
        }
        ensure!(
            self.frames.is_empty(),
            "The Markdown event stream is unbalanced"
        );
        // Outermost first so a consumer can walk parents before children.
        self.spans.sort_by(|a, b| {
            (a.full.start, std::cmp::Reverse(a.full.end))
                .cmp(&(b.full.start, std::cmp::Reverse(b.full.end)))
        });
        self.assert_disjoint_siblings()?;
        Ok(())
    }

    /// Record text into every enclosing inline frame.
    fn note_text(&mut self, range: Range<usize>, text: &str) {
        for frame in self.frames.iter_mut().rev() {
            match &mut frame.tag {
                FrameTag::Inline { content, .. } => match content {
                    Some(existing) => {
                        existing.start = existing.start.min(range.start);
                        existing.end = existing.end.max(range.end);
                    }
                    None => *content = Some(range.clone()),
                },
                FrameTag::Ignored | FrameTag::Block { .. } => {}
            }
        }
        // Character entities and other rewrites are one source range whose
        // display text differs from its bytes, so the projection substitutes
        // rather than copies it.
        if text.is_empty() || self.source.get(range.clone()).is_none() {
            return;
        }
        if self.source[range.clone()] == *text {
            return;
        }
        if self.spans.len() >= MAX_SPANS {
            return;
        }
        self.spans.push(Span {
            full: range.clone(),
            content: range,
            depth: 0,
            style: RunStyle::default(),
            decoded: Some(Arc::from(text)),
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

fn inline_frame(start: usize, style: RunStyle) -> Frame {
    Frame {
        tag: FrameTag::Inline {
            style,
            content: None,
        },
        start,
    }
}

fn italic() -> RunStyle {
    RunStyle {
        italic: true,
        ..Default::default()
    }
}

fn bold() -> RunStyle {
    RunStyle {
        bold: true,
        ..Default::default()
    }
}

fn strike() -> RunStyle {
    RunStyle {
        strike: true,
        ..Default::default()
    }
}

fn link() -> RunStyle {
    RunStyle {
        link: true,
        ..Default::default()
    }
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
        let quote_depth = frames.iter().filter(|frame| frame.quote).count();
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
        let hidden = leaf.is_some_and(|frame| {
            matches!(frame.kind, BlockKind::Heading(_))
                && frame.source.end > end
                && is_setext_underline(&source[start..end])
        });
        lines.push(LineKind {
            source: start..end,
            kind,
            quote_depth,
            hidden,
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

/// A line of only `=` or only `-`, which marks the heading above it.
fn is_setext_underline(line: &str) -> bool {
    let trimmed = line.trim_end_matches(['\r', '\n']);
    let trimmed = trimmed.trim();
    (trimmed.starts_with('=') && trimmed.chars().all(|c| c == '='))
        || (trimmed.starts_with('-') && trimmed.chars().all(|c| c == '-'))
}
