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
use super::{Affinity, MAX_SOURCE_BYTES};
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
        let source = &source[..source.len().min(MAX_SOURCE_BYTES)];
        let mut builder = Builder {
            source,
            mode,
            selection,
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
}

/// Appends source-derived display text while recording the mapping segments.
struct LineBuilder<'a> {
    line: &'a str,
    base: usize,
    selection: Range<usize>,
    text: String,
    runs: Vec<DisplayRun>,
    segments: Vec<Segment>,
}

impl<'a> LineBuilder<'a> {
    fn new(line: &'a str, base: usize, selection: Range<usize>) -> Self {
        Self {
            line,
            base,
            selection,
            text: String::with_capacity(line.len()),
            runs: Vec::new(),
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
        if let Some(last) = self.runs.last_mut()
            && last.style == style
            && last.range.end == display.start
        {
            last.range.end = display.end;
            return;
        }
        self.runs.push(DisplayRun {
            range: display,
            style,
        });
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
        let mut line_start = 0;
        while line_start < self.source.len() {
            let (line_end, next) = line_bounds(self.source, line_start);
            let display_start = self.text.len();
            let rendered = self.render_line(line_start, line_end);
            self.text.push_str(&rendered.text);
            for segment in rendered.segments {
                self.segments.push(offset_segment(segment, display_start));
            }
            if line_end > line_start && self.text.len() > display_start {
                // The renderer already covered the whole line.
            }
            self.lines.push(DisplayLine {
                source: line_start..line_end,
                display: display_start..self.text.len(),
                kind: rendered.kind,
                runs: rendered.runs,
                revealed: rendered.revealed,
            });
            if next > line_end {
                self.text.push_str(&self.source[line_end..next]);
                self.segments.push(Segment::Copy {
                    source: line_end..next,
                    display: self.text.len() - (next - line_end),
                });
            }
            line_start = next;
        }
    }

    fn render_line(&mut self, start: usize, end: usize) -> Rendered {
        let line = &self.source[start..end];
        let mut local = LineBuilder::new(line, start, self.selection.clone());

        if self.mode == ProjectionMode::Source {
            local.copy(0..line.len());
            return finish(local, BlockKind::Paragraph, false);
        }

        let trimmed = line.trim_start();
        let leading = line.len() - trimmed.len();

        // Fenced code is a line-level state machine: P2 only needs the fence
        // boundaries, not a parsed document tree.
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

        if let Some((level, prefix_end)) = heading_prefix(line) {
            let revealed = local.selection_touches(&(0..prefix_end));
            if revealed {
                local.copy(0..prefix_end);
            } else {
                local.replace(0..prefix_end, "");
            }
            inline(&mut local, prefix_end..line.len());
            return finish(local, BlockKind::Heading(level), revealed);
        }

        if let Some((prefix_end, depth)) = quote_prefix(line) {
            let revealed = local.selection_touches(&(0..prefix_end));
            if revealed {
                local.copy(0..prefix_end);
            } else {
                local.replace(0..prefix_end, "");
            }
            inline(&mut local, prefix_end..line.len());
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
            inline(&mut local, prefix.end..line.len());
            return finish(
                local,
                BlockKind::List {
                    ordered: prefix.ordered,
                    depth: prefix.depth,
                },
                false,
            );
        }

        if leading > 0 {
            local.copy(0..leading);
        }
        inline(&mut local, leading..line.len());
        finish(local, BlockKind::Paragraph, false)
    }
}

fn finish(local: LineBuilder<'_>, kind: BlockKind, revealed: bool) -> Rendered {
    Rendered {
        text: local.text,
        runs: local.runs,
        segments: local.segments,
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
        let search_start = index + marker.len();
        let search = &line[search_start..limit];
        let mut offset = 0;
        while let Some(found) = search[offset..].find(marker) {
            let candidate = search_start + offset + found;
            if !escaped(line, candidate) {
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
