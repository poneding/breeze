//! Source-to-display projection for the live editor.
//!
//! The projection is a pure function of the source text, the current selection
//! and the view mode. It never rewrites the source: hidden markers stay in the
//! buffer and are only collapsed in this display mapping, so every edit remains
//! an ordinary source-range transaction.
//!
//! P2 scope: paragraphs, ATX headings, quotes, list markers, fenced code and
//! non-nested inline emphasis, code, strike and link labels. Anything else is
//! shown as literal source.
use super::{Affinity, MAX_SOURCE_BYTES, Semantic};
use std::ops::Range;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RunStyle {
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub strike: bool,
    pub link: bool,
}

impl RunStyle {
    pub fn is_plain(self) -> bool {
        self == Self::default()
    }

    /// Compose nested spans: an emphasized word inside strong text is both.
    pub fn union(self, other: Self) -> Self {
        Self {
            bold: self.bold || other.bold,
            italic: self.italic || other.italic,
            code: self.code || other.code,
            strike: self.strike || other.strike,
            link: self.link || other.link,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayRun {
    pub range: Range<usize>,
    pub style: RunStyle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockKind {
    Paragraph,
    Heading(u8),
    Quote { depth: usize },
    List { ordered: bool, depth: usize },
    Fence,
    Code,
    Table,
}

#[derive(Clone, Debug)]
pub struct DisplayLine {
    /// The source line's bytes, excluding its terminator.
    pub source: Range<usize>,
    /// The display line's bytes in [`Projection::text`], excluding its terminator.
    pub display: Range<usize>,
    pub kind: BlockKind,
    /// Inline style runs over the display range.
    pub runs: Vec<DisplayRun>,
    /// Whether this line's block markers are currently visible.
    pub revealed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectionMode {
    Live,
    Source,
}

/// One piece of the source-to-display mapping.
///
/// A `Copy` run advances one source byte per display byte. A `Replace` run has
/// an arbitrary display width, including zero for a hidden marker, so several
/// source offsets collapse onto one display offset.
#[derive(Clone, Debug)]
enum Segment {
    Copy {
        source: Range<usize>,
        display: usize,
    },
    Replace {
        source: Range<usize>,
        display: Range<usize>,
    },
}

impl Segment {
    fn source(&self) -> Range<usize> {
        match self {
            Self::Copy { source, .. } => source.clone(),
            Self::Replace { source, .. } => source.clone(),
        }
    }

    fn display_start(&self) -> usize {
        match self {
            Self::Copy { display, .. } => *display,
            Self::Replace { display, .. } => display.start,
        }
    }

    fn display_end(&self) -> usize {
        match self {
            Self::Copy { source, display } => display + source.len(),
            Self::Replace { display, .. } => display.end,
        }
    }

    fn contains_display(&self, offset: usize) -> bool {
        self.display_start() <= offset && offset <= self.display_end()
    }

    fn source_to_display(&self, offset: usize, affinity: Affinity) -> usize {
        let source = self.source();
        match self {
            Self::Copy { display, .. } => {
                let delta = offset.clamp(source.start, source.end) - source.start;
                display + delta
            }
            Self::Replace { display, .. } => {
                if offset <= source.start || display.is_empty() {
                    display.start
                } else if offset >= source.end {
                    display.end
                } else if affinity == Affinity::Before {
                    display.start
                } else {
                    display.end
                }
            }
        }
    }

    fn display_to_source(&self, offset: usize, affinity: Affinity) -> usize {
        let source = self.source();
        match self {
            Self::Copy { display, .. } => {
                let delta = offset.clamp(*display, self.display_end()) - display;
                source.start + delta
            }
            Self::Replace { display, .. } => {
                if offset <= display.start || display.is_empty() {
                    source.start
                } else if offset >= display.end {
                    source.end
                } else if affinity == Affinity::Before {
                    source.start
                } else {
                    source.end
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct Projection {
    pub mode: ProjectionMode,
    pub text: String,
    pub lines: Vec<DisplayLine>,
    segments: Vec<Segment>,
    /// `(display start, segment index)`, ascending, for display lookups.
    display_index: Vec<(usize, usize)>,
}

impl Projection {
    pub fn source(source: &str) -> Self {
        Self::build(source, 0..0, ProjectionMode::Source)
    }

    pub fn live(source: &str, selection: Range<usize>) -> Self {
        Self::build(source, selection, ProjectionMode::Live)
    }

    pub fn build(source: &str, selection: Range<usize>, mode: ProjectionMode) -> Self {
        Self::build_with_literal(source, selection, mode, None)
    }

    /// Build from the parser's semantic layer.
    ///
    /// Line kinds and inline spans come from the background parse, so reference
    /// links, nested emphasis, character entities and setext headings render
    /// correctly. The projection still decides what to hide: the parser says
    /// what a construct means, this layer says what the reader sees.
    pub fn build_semantic(
        source: &str,
        selection: Range<usize>,
        mode: ProjectionMode,
        literal: Option<Range<usize>>,
        semantic: &Semantic,
    ) -> Self {
        Self::build_inner(source, selection, mode, literal, Some(semantic))
    }

    /// Build the projection while keeping `literal` rendered as source text.
    /// Native composition uses this so a half-typed delimiter never restyles
    /// or hides text under the input method.
    pub fn build_with_literal(
        source: &str,
        selection: Range<usize>,
        mode: ProjectionMode,
        literal: Option<Range<usize>>,
    ) -> Self {
        Self::build_inner(source, selection, mode, literal, None)
    }

    fn build_inner(
        source: &str,
        selection: Range<usize>,
        mode: ProjectionMode,
        literal: Option<Range<usize>>,
        semantic: Option<&Semantic>,
    ) -> Self {
        let source = &source[..source.len().min(MAX_SOURCE_BYTES)];
        // A semantic layer parsed from a different source would index out of
        // bounds; refuse it rather than render nonsense.
        let semantic = semantic.filter(|semantic| {
            semantic
                .lines
                .iter()
                .all(|line| line.source.end <= source.len())
                && semantic
                    .spans
                    .iter()
                    .all(|span| span.full.end <= source.len() && span.content.end <= source.len())
        });
        let mut builder = Builder {
            source,
            mode,
            selection,
            literal,
            semantic,
            text: String::with_capacity(source.len()),
            lines: Vec::new(),
            segments: Vec::new(),
            fence: None,
        };
        builder.run();
        let segments = std::mem::take(&mut builder.segments);
        let mut display_index: Vec<(usize, usize)> = segments
            .iter()
            .enumerate()
            .map(|(index, segment)| (segment.display_start(), index))
            .collect();
        display_index.sort_unstable();
        Projection {
            mode,
            text: builder.text,
            lines: builder.lines,
            segments,
            display_index,
        }
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// The line containing a source offset, or the last line for EOF.
    pub fn line_at_source(&self, offset: usize) -> usize {
        self.lines
            .partition_point(|line| line.source.end < offset)
            .min(self.lines.len().saturating_sub(1))
    }

    /// The display offset for a source offset. Offsets inside a hidden span
    /// collapse onto the span's display position.
    pub fn source_to_display(&self, offset: usize) -> usize {
        let Some(index) = self
            .segments
            .partition_point(|segment| segment.source().start <= offset)
            .checked_sub(1)
        else {
            return 0;
        };
        self.segments[index].source_to_display(offset, Affinity::After)
    }

    /// The source offset for a display offset. `Before`/`After` decide which
    /// side of a collapsed hidden span a caret at that position belongs to.
    pub fn display_to_source(&self, offset: usize, affinity: Affinity) -> usize {
        let mut earliest = None;
        let mut latest = None;
        // Start from the last segment whose display range starts at or before
        // the offset, then widen over directly adjacent segments so a collapsed
        // zero-width span on either side is considered.
        let start = self
            .display_index
            .partition_point(|(display, _)| *display <= offset)
            .saturating_sub(1);
        for (display, index) in
            self.display_index[start.min(self.display_index.len().saturating_sub(1))..].iter()
        {
            if *display > offset {
                break;
            }
            let segment = &self.segments[*index];
            if segment.contains_display(offset) {
                earliest.get_or_insert(*index);
                latest = Some(*index);
            }
        }
        // Walk backwards for segments that end exactly at the offset.
        let mut cursor = start;
        while cursor > 0 {
            cursor -= 1;
            let index = self.display_index[cursor].1;
            if self.segments[index].contains_display(offset) {
                earliest = Some(index);
            } else if self.segments[index].display_end() < offset {
                break;
            }
        }
        let Some(index) = (match affinity {
            Affinity::Before => earliest,
            Affinity::After => latest,
        }) else {
            return self
                .segments
                .first()
                .map(|segment| segment.source().start)
                .unwrap_or(0);
        };
        self.segments[index].display_to_source(offset, affinity)
    }

    pub fn source_range_to_display(&self, range: Range<usize>) -> Range<usize> {
        self.source_to_display(range.start)..self.source_to_display(range.end)
    }

    /// The source range a display range covers, expanded to include any hidden
    /// markers the range touches. Editing a visible selection must never
    /// silently leave half of a hidden delimiter behind.
    pub fn display_range_to_source(&self, range: Range<usize>) -> Range<usize> {
        let start = self.display_to_source(range.start, Affinity::Before);
        let end = self.display_to_source(range.end, Affinity::After);
        start.min(end)..start.max(end)
    }
}

struct Builder<'a> {
    source: &'a str,
    mode: ProjectionMode,
    selection: Range<usize>,
    text: String,
    lines: Vec<DisplayLine>,
    segments: Vec<Segment>,
    fence: Option<char>,
    literal: Option<Range<usize>>,
    semantic: Option<&'a Semantic>,
}

/// Appends source-derived display text while recording the mapping segments.
struct LineBuilder<'a> {
    line: &'a str,
    base: usize,
    selection: Range<usize>,
    text: String,
    /// Style applications in paint order; flattened into disjoint runs later so
    /// nested spans compose instead of producing overlapping text runs.
    paints: Vec<(Range<usize>, RunStyle)>,
    segments: Vec<Segment>,
}

impl<'a> LineBuilder<'a> {
    fn new(line: &'a str, base: usize, selection: Range<usize>) -> Self {
        Self {
            line,
            base,
            selection,
            text: String::with_capacity(line.len()),
            paints: Vec::new(),
            segments: Vec::new(),
        }
    }

    fn absolute(&self, range: Range<usize>) -> Range<usize> {
        self.base + range.start..self.base + range.end
    }

    fn selection_touches(&self, range: &Range<usize>) -> bool {
        let range = self.absolute(range.clone());
        self.selection.start <= range.end && self.selection.end >= range.start
    }

    /// Copy a source span verbatim into the display text.
    fn copy(&mut self, range: Range<usize>) {
        if range.is_empty() {
            return;
        }
        debug_assert!({
            let slice = &self.line[range.clone()];
            !slice.contains('\n') && !slice.contains('\r')
        });
        let start = self.text.len();
        self.text.push_str(&self.line[range.clone()]);
        self.segments.push(Segment::Copy {
            source: self.base + range.start..self.base + range.end,
            display: start,
        });
    }

    /// Replace a source span with display text, collapsing it to zero width
    /// when the display text is empty.
    fn replace(&mut self, range: Range<usize>, display: &str) {
        debug_assert!(range.end <= self.line.len());
        let start = self.text.len();
        self.text.push_str(display);
        self.segments.push(Segment::Replace {
            source: self.base + range.start..self.base + range.end,
            display: start..self.text.len(),
        });
    }

    fn style(&mut self, style: RunStyle, display: Range<usize>) {
        if style.is_plain() || display.is_empty() {
            return;
        }
        self.paints.push((display, style));
    }

    /// Flatten painted styles into sorted, disjoint display runs.
    fn into_runs(self) -> (String, Vec<DisplayRun>, Vec<Segment>) {
        let mut boundaries: Vec<usize> = self
            .paints
            .iter()
            .flat_map(|(range, _)| [range.start, range.end])
            .collect();
        boundaries.push(0);
        boundaries.push(self.text.len());
        boundaries.sort_unstable();
        boundaries.dedup();
        let mut runs: Vec<DisplayRun> = Vec::new();
        for window in boundaries.windows(2) {
            let range = window[0]..window[1];
            if range.is_empty() {
                continue;
            }
            let mut style = RunStyle::default();
            for (paint, painted) in &self.paints {
                if paint.start <= range.start && paint.end >= range.end {
                    style = style.union(*painted);
                }
            }
            if style.is_plain() {
                continue;
            }
            if let Some(last) = runs.last_mut()
                && last.style == style
                && last.range.end == range.start
            {
                last.range.end = range.end;
                continue;
            }
            runs.push(DisplayRun { range, style });
        }
        // A run must never split a UTF-8 character.
        runs.retain(|run| {
            self.text.is_char_boundary(run.range.start) && self.text.is_char_boundary(run.range.end)
        });
        (self.text, runs, self.segments)
    }
}

struct Rendered {
    text: String,
    runs: Vec<DisplayRun>,
    segments: Vec<Segment>,
    kind: BlockKind,
    revealed: bool,
}

struct ListPrefix {
    indent: usize,
    end: usize,
    ordered: bool,
    depth: usize,
}

impl Builder<'_> {
    fn run(&mut self) {
        // The semantic layer already owns line splitting, so its lines are used
        // verbatim; otherwise the projection falls back to its own scanner.
        // Each entry keeps the line's terminator end so the bytes between lines
        // are copied exactly once.
        let ranges: Vec<(Range<usize>, usize)> = match self.semantic {
            Some(semantic) => semantic
                .lines
                .iter()
                .enumerate()
                .map(|(index, line)| {
                    let next = semantic
                        .lines
                        .get(index + 1)
                        .map(|line| line.source.start)
                        .unwrap_or(self.source.len());
                    (line.source.clone(), next)
                })
                .collect(),
            None => {
                let mut ranges = Vec::new();
                let mut start = 0;
                while start < self.source.len() {
                    let (end, next) = line_bounds(self.source, start);
                    ranges.push((start..end, next));
                    start = next;
                }
                if ranges.is_empty() {
                    ranges.push((0..0, 0));
                }
                ranges
            }
        };
        for (index, (range, next)) in ranges.into_iter().enumerate() {
            let line_start = range.start;
            let line_end = range.end;
            let display_start = self.text.len();
            let rendered = self.render_line(index, line_start, line_end);
            self.text.push_str(&rendered.text);
            for segment in rendered.segments {
                self.segments.push(offset_segment(segment, display_start));
            }
            self.lines.push(DisplayLine {
                source: line_start..line_end,
                display: display_start..self.text.len(),
                kind: rendered.kind,
                runs: rendered.runs,
                revealed: rendered.revealed,
            });
            // The terminator runs to the start of the next line. Copy it even
            // when the next line is hidden, so a setext underline keeps its
            // bytes reachable from the mapping.
            if next > line_end {
                self.text.push_str(&self.source[line_end..next]);
                self.segments.push(Segment::Copy {
                    source: line_end..next,
                    display: self.text.len() - (next - line_end),
                });
            }
        }
    }

    fn render_line(&mut self, index: usize, start: usize, end: usize) -> Rendered {
        let line = &self.source[start..end];
        let mut local = LineBuilder::new(line, start, self.selection.clone());

        let literal = self
            .literal
            .as_ref()
            .is_some_and(|literal| literal.start <= end && literal.end >= start);
        if self.mode == ProjectionMode::Source || literal {
            local.copy(0..line.len());
            return finish(local, BlockKind::Paragraph, false);
        }

        // A setext underline belongs to the heading above it and has no row.
        let semantic_line = self
            .semantic
            .and_then(|semantic| semantic.lines.get(index))
            .cloned();
        if semantic_line.as_ref().is_some_and(|line| line.hidden) {
            let kind = semantic_line
                .map(|line| line.kind)
                .unwrap_or(BlockKind::Paragraph);
            return finish(local, kind, true);
        }
        // Copy the reference out of `self` so the mutable `self.fence` state
        // below does not conflict with the borrowed spans.
        let semantic = self.semantic;
        let spans = spans_for(semantic, start, end);

        let trimmed = line.trim_start();
        let leading = line.len() - trimmed.len();
        let semantic_kind = semantic_line.as_ref().map(|line| line.kind.clone());
        let quote_depth = semantic_line
            .as_ref()
            .map(|line| line.quote_depth)
            .unwrap_or(0);

        // With the parser's block kinds, fences and code are known without a
        // line-level state machine; the fallback keeps its own scanner.
        let fenced = matches!(semantic_kind, Some(BlockKind::Code | BlockKind::Fence));
        if !fenced {
            if let Some(marker) = self.fence {
                if trimmed.starts_with(&marker.to_string().repeat(3)) {
                    self.fence = None;
                    local.copy(0..line.len());
                    return finish(local, BlockKind::Fence, true);
                }
                local.copy(0..line.len());
                return finish(local, BlockKind::Code, false);
            }
            if let Some(marker) = fence_opener(trimmed) {
                self.fence = Some(marker);
                local.copy(0..line.len());
                return finish(local, BlockKind::Fence, true);
            }
        } else {
            self.fence = None;
            local.copy(0..line.len());
            // The fence lines themselves are the delimiters being shown.
            let kind = if fence_opener(trimmed).is_some() {
                BlockKind::Fence
            } else {
                BlockKind::Code
            };
            let revealed = kind == BlockKind::Fence;
            return finish(local, kind, revealed);
        }

        if let Some((level, prefix_end)) = heading_prefix(line) {
            let revealed = local.selection_touches(&(0..prefix_end));
            if revealed {
                local.copy(0..prefix_end);
            } else {
                local.replace(0..prefix_end, "");
            }
            inline_styled(&mut local, prefix_end..line.len(), spans);
            return finish(local, BlockKind::Heading(level), revealed);
        }

        // A setext heading's title sits on its own line with no marker to hide.
        if let Some(BlockKind::Heading(level)) = semantic_kind
            && semantic_kind_is_setext(semantic, index)
        {
            inline_styled(&mut local, 0..line.len(), spans);
            return finish(local, BlockKind::Heading(level), false);
        }

        // Quote markers are hidden one per enclosing block, not by shape.
        let prefix_end = if quote_depth > 0 {
            quote_markers(line, quote_depth).unwrap_or(0)
        } else {
            quote_prefix(line).map(|(end, _)| end).unwrap_or(0)
        };
        if prefix_end > 0 {
            let revealed = local.selection_touches(&(0..prefix_end));
            if revealed {
                local.copy(0..prefix_end);
            } else {
                local.replace(0..prefix_end, "");
            }
            inline_styled(&mut local, prefix_end..line.len(), spans);
            let depth = quote_depth.max(1);
            return finish(local, BlockKind::Quote { depth }, revealed);
        }

        if let Some(prefix) = list_prefix(line) {
            if prefix.ordered {
                local.copy(0..prefix.end);
            } else {
                // Keep the indentation, then substitute a visible bullet for the
                // source marker so the caret never sits inside an invisible dash.
                local.copy(0..prefix.indent);
                local.replace(prefix.indent..prefix.end, "\u{2022} ");
            }
            inline_styled(&mut local, prefix.end..line.len(), spans);
            let depth = semantic_kind
                .as_ref()
                .and_then(|kind| match kind {
                    BlockKind::List { depth, .. } => Some(*depth),
                    _ => None,
                })
                .unwrap_or(prefix.depth);
            return finish(
                local,
                BlockKind::List {
                    ordered: prefix.ordered,
                    depth,
                },
                false,
            );
        }

        if leading > 0 {
            local.copy(0..leading);
        }
        inline_styled(&mut local, leading..line.len(), spans);
        finish(local, semantic_kind.unwrap_or(BlockKind::Paragraph), false)
    }
}

/// The semantic spans that start on this line.
///
/// A span that opened on an earlier line keeps its delimiters there; this line
/// renders its own slice literally rather than re-deriving a style.
fn spans_for(semantic: Option<&Semantic>, start: usize, end: usize) -> Option<&[super::Span]> {
    let semantic = semantic?;
    let first = semantic
        .spans
        .partition_point(|span| span.full.start < start);
    let last = semantic.spans.partition_point(|span| span.full.start < end);
    // `Some(&[])` is meaningful: the parser saw nothing to style on this line,
    // so the line is literal and the fallback scanner must not second-guess it.
    Some(&semantic.spans[first..last.max(first)])
}

/// Whether the heading at `index` was written as `Title` + an underline.
fn semantic_kind_is_setext(semantic: Option<&Semantic>, index: usize) -> bool {
    let Some(semantic) = semantic else {
        return false;
    };
    semantic
        .lines
        .get(index..index + 2)
        .is_some_and(|lines| lines[0].kind == lines[1].kind && lines[1].hidden)
}

/// Hide at most `depth` block-quote markers and the spaces that follow them.
fn quote_markers(line: &str, depth: usize) -> Option<usize> {
    let mut index = 0;
    for _ in 0..depth {
        if !line[index..].starts_with('>') {
            break;
        }
        index += 1;
        if line[index..].starts_with(' ') {
            index += 1;
        }
    }
    Some(index)
}

fn finish(local: LineBuilder<'_>, kind: BlockKind, revealed: bool) -> Rendered {
    let (text, runs, segments) = local.into_runs();
    Rendered {
        text,
        runs,
        segments,
        kind,
        revealed,
    }
}

/// The line's terminator: `\r\n`, `\n`, `\r` or the end of the document.
fn line_bounds(source: &str, start: usize) -> (usize, usize) {
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

fn fence_opener(trimmed: &str) -> Option<char> {
    for marker in ['`', '~'] {
        let count = trimmed.chars().take_while(|c| *c == marker).count();
        if count >= 3 {
            return Some(marker);
        }
    }
    None
}

fn heading_prefix(line: &str) -> Option<(u8, usize)> {
    let hashes = line.chars().take_while(|c| *c == '#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let rest = &line[hashes..];
    if rest.is_empty() {
        return Some((hashes as u8, hashes));
    }
    if !rest.starts_with(' ') {
        return None;
    }
    let spaces = rest.chars().take_while(|c| *c == ' ').count();
    Some((hashes as u8, hashes + spaces))
}

fn quote_prefix(line: &str) -> Option<(usize, usize)> {
    let mut index = 0;
    let mut depth = 0;
    loop {
        let rest = &line[index..];
        if !rest.starts_with('>') {
            break;
        }
        depth += 1;
        index += 1;
        if line[index..].starts_with(' ') {
            index += 1;
        }
    }
    // A bare `>` with no content is still a quote marker.
    (depth > 0).then_some((index, depth))
}

fn list_prefix(line: &str) -> Option<ListPrefix> {
    let indent = line
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .map(|c| if c == '\t' { 4 } else { 1 })
        .sum::<usize>();
    let rest = &line[indent..];
    let mut depth = indent / 2;
    let (marker_len, ordered): (usize, bool) = if rest.starts_with(['-', '+', '*']) {
        (1, false)
    } else {
        let digits = rest.chars().take_while(char::is_ascii_digit).count();
        if digits == 0 || digits > 9 {
            return None;
        }
        let after = &rest[digits..];
        let punctuation = after.chars().next()?;
        if !matches!(punctuation, '.' | ')') {
            return None;
        }
        (digits + 1, true)
    };
    let after_marker = &rest[marker_len..];
    if !after_marker.is_empty() && !after_marker.starts_with(' ') {
        return None;
    }
    let spaces = after_marker
        .chars()
        .take_while(|c| *c == ' ')
        .count()
        .max(1);
    if indent > 0 {
        depth = depth.saturating_sub(1);
    }
    Some(ListPrefix {
        indent,
        end: indent + marker_len + spaces,
        ordered,
        depth: depth.min(8),
    })
}

/// Style non-nested inline spans, hiding their delimiters unless the selection
/// touches them. Everything unrecognised is copied verbatim.
fn inline(local: &mut LineBuilder<'_>, range: Range<usize>) {
    let line = local.line;
    let mut index = range.start;
    while index < range.end {
        if let Some(span) = inline_span(line, range.end, index) {
            let full = span.full.clone();
            if local.selection_touches(&full) {
                local.copy(full);
            } else {
                local.replace(span.full.start..span.content.start, "");
                let style = span.style;
                let display_start = local.text.len();
                local.copy(span.content.clone());
                let display_end = local.text.len();
                local.style(style, display_start..display_end);
                local.replace(span.content.end..span.full.end, "");
            }
            index = span.full.end;
            continue;
        }
        // Copy one character so multi-byte text is never split.
        let character = line[index..].chars().next().expect("index is in range");
        local.copy(index..index + character.len_utf8());
        index += character.len_utf8();
    }
}

struct InlineSpan {
    full: Range<usize>,
    content: Range<usize>,
    style: RunStyle,
}

/// Find the inline span that starts exactly at `index`, if any.
fn inline_span(line: &str, limit: usize, index: usize) -> Option<InlineSpan> {
    let rest = &line[index..limit];
    if escaped(line, index) {
        return None;
    }
    // Code first: inside a code span, markers are literal text.
    if rest.starts_with('`') {
        let ticks = rest.chars().take_while(|c| *c == '`').count();
        let open = "`".repeat(ticks);
        let search = &line[index + ticks..limit];
        let end = search.find(&open)?;
        let content = index + ticks..index + ticks + end;
        return Some(InlineSpan {
            full: index..content.end + ticks,
            content,
            style: RunStyle {
                code: true,
                ..Default::default()
            },
        });
    }
    for (marker, style) in [
        (
            "**",
            RunStyle {
                bold: true,
                ..Default::default()
            },
        ),
        (
            "__",
            RunStyle {
                bold: true,
                ..Default::default()
            },
        ),
        (
            "~~",
            RunStyle {
                strike: true,
                ..Default::default()
            },
        ),
        (
            "*",
            RunStyle {
                italic: true,
                ..Default::default()
            },
        ),
        (
            "_",
            RunStyle {
                italic: true,
                ..Default::default()
            },
        ),
    ] {
        if !rest.starts_with(marker) {
            continue;
        }
        // A marker only opens when its run is exactly that marker: otherwise
        // `**x` would be read as an empty `*` span and lose its asterisks.
        let run = rest
            .chars()
            .take_while(|c| *c == marker.chars().next().unwrap_or('*'))
            .count();
        if run != marker.len() {
            continue;
        }
        let search_start = index + marker.len();
        let search = &line[search_start..limit];
        let mut offset = 0;
        while let Some(found) = search[offset..].find(marker) {
            let candidate = search_start + offset + found;
            let closing_run = line[candidate..]
                .chars()
                .take_while(|c| *c == marker.chars().next().unwrap_or('*'))
                .count();
            if closing_run == marker.len() && !escaped(line, candidate) {
                return Some(InlineSpan {
                    full: index..candidate + marker.len(),
                    content: search_start..candidate,
                    style,
                });
            }
            offset += found + marker.len();
        }
    }
    // `[label](destination)`: only the label is displayed for now, because the
    // destination stays editable as source until P5 adds a link editor.
    if rest.starts_with('[') {
        let close = line[index + 1..limit].find(']')? + index + 1;
        if line[close + 1..limit].starts_with('(') {
            let end = line[close + 2..limit].find(')')? + close + 2;
            let content = index + 1..close;
            return Some(InlineSpan {
                full: index..end + 1,
                content,
                style: RunStyle {
                    link: true,
                    ..Default::default()
                },
            });
        }
    }
    None
}

/// A delimiter preceded by an odd number of backslashes is literal text.
fn escaped(line: &str, index: usize) -> bool {
    let mut backslashes = 0;
    let mut cursor = index;
    while cursor > 0 && line.as_bytes()[cursor - 1] == b'\\' {
        backslashes += 1;
        cursor -= 1;
    }
    backslashes % 2 == 1
}

/// Shift a segment's display range into the document-wide display text.
fn offset_segment(segment: Segment, by: usize) -> Segment {
    match segment {
        Segment::Copy { source, display } => Segment::Copy {
            source,
            display: display + by,
        },
        Segment::Replace { source, display } => Segment::Replace {
            source,
            display: display.start + by..display.end + by,
        },
    }
}

/// Emit a line range using the parser's inline spans.
///
/// Each span hides its own delimiters unless the selection touches it, and
/// nested spans recurse so every level hides what belongs to it. Without a
/// semantic layer this delegates to the line-local scanner.
fn inline_styled(local: &mut LineBuilder<'_>, range: Range<usize>, spans: Option<&[super::Span]>) {
    let Some(spans) = spans else {
        return inline(local, range);
    };
    let mut cursor = 0usize;
    emit_spans(local, range, spans, &mut cursor);
}

fn emit_spans(
    local: &mut LineBuilder<'_>,
    range: Range<usize>,
    spans: &[super::Span],
    cursor: &mut usize,
) {
    let base = local.base;
    let mut position = range.start;
    while position < range.end {
        if let Some(span) = spans.get(*cursor) {
            let full = span.full.start.saturating_sub(base)..span.full.end.saturating_sub(base);
            let content =
                span.content.start.saturating_sub(base)..span.content.end.saturating_sub(base);
            let usable = span.full.start >= base
                && span.full.end <= base + local.line.len()
                && full.start == position
                && full.end <= range.end
                && full.start <= content.start
                && content.end <= full.end;
            if usable {
                *cursor += 1;
                if local.selection_touches(&full) {
                    // Revealing the construct shows its source, delimiters included.
                    local.copy(full.clone());
                } else {
                    local.replace(full.start..content.start, "");
                    let style = span.style;
                    let display_start = local.text.len();
                    if let Some(decoded) = &span.decoded {
                        local.replace(content.clone(), decoded);
                    } else {
                        emit_spans(local, content.clone(), spans, cursor);
                    }
                    let display_end = local.text.len();
                    local.style(style, display_start..display_end);
                    local.replace(content.end..full.end, "");
                }
                position = full.end;
                continue;
            }
        }
        let Some(character) = local.line[position..].chars().next() else {
            break;
        };
        local.copy(position..position + character.len_utf8());
        position += character.len_utf8();
    }
}
