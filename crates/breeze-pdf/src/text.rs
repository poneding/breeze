//! Page text in reading order, and the geometry of a selection over it.
//!
//! MuPDF reports characters line by line in reading order. A selection is the
//! run of that order between two carets, as in a text editor, rather than every
//! glyph inside a dragged rectangle. Lines that share a baseline form one visual
//! row, and consecutive rows of a paragraph meet on one shared edge: a
//! translucent highlight then neither breaks between rows nor doubles where tall
//! CJK font boxes overlap the next row.
//!
//! Geometry stays in unrotated PDF user space. Each row records the user-space
//! axes it advances and descends along, so reading order also holds on
//! `/Rotate`d pages, where upright text runs along a user-space y axis.
use anyhow::{Context, Result};
use breeze_core::annotations::{Point, Rect};
use mupdf::{
    Matrix, TextPageFlags,
    pdf::PdfPage,
    text_page::{StructuredTextLine, TextBlockContent},
};
use std::ops::Range;

/// Characters retained for one page; a denser page is not selectable.
const MAX_GLYPHS: usize = 50_000;
/// Cosine of the steepest angle, about 5 degrees, at which a line still reads
/// along a page axis. Skewed scans with a text layer stay selectable; steeper
/// runs are not guessed.
const AXIS_COS: f32 = 0.996;
/// Successive lines of a paragraph overlap by at most this share of the shorter
/// row; tall CJK boxes on tight leading reach about a fifth.
const MAX_ROW_OVERLAP: f32 = 0.5;
/// Rows further apart than this many row heights do not continue each other.
const MAX_LINE_GAP: f32 = 2.;
/// A gap wider than its neighbouring line gaps by this many row heights is a
/// paragraph break, which keeps its visual break in a highlight.
const PARAGRAPH_GAP: f32 = 0.35;
/// A gap inside a row wider than this many row heights splits its highlight, so
/// separate runs such as a footer and its page number are not bridged.
const SEGMENT_GAP: f32 = 3.;

#[derive(Clone, Debug)]
pub struct Glyph {
    pub character: char,
    /// Axis-aligned bounds in unrotated PDF user space.
    pub bounds: Rect,
}

/// Selectable text of one page: glyphs in reading order and the rows they form.
#[derive(Clone, Debug, Default)]
pub struct PageText {
    glyphs: Vec<Glyph>,
    rows: Vec<Row>,
}

/// One line as the engine reports it, before lines are gathered into rows.
pub(crate) struct Line {
    pub glyphs: Vec<Glyph>,
    /// Sum of the glyphs' advance vectors, in user space.
    pub advance: (f32, f32),
    /// Sum of the glyphs' ascender-to-descender vectors, in user space.
    pub descent: (f32, f32),
}

impl PageText {
    pub fn glyphs(&self) -> &[Glyph] {
        &self.glyphs
    }

    pub fn is_empty(&self) -> bool {
        self.glyphs.is_empty()
    }

    /// Heap bytes retained, for cache budgets.
    pub fn bytes(&self) -> usize {
        self.glyphs.len() * std::mem::size_of::<Glyph>()
            + self
                .rows
                .iter()
                .map(|row| {
                    std::mem::size_of::<Row>() + row.joins.len() * std::mem::size_of::<usize>()
                })
                .sum::<usize>()
    }

    /// Glyphs between the carets nearest two user-space points, in reading
    /// order. Empty when both points share a caret or nothing is selectable.
    pub fn select(&self, from: Point, to: Point) -> Range<usize> {
        match (self.caret(from), self.caret(to)) {
            (Some(a), Some(b)) => a.min(b)..a.max(b),
            _ => 0..0,
        }
    }

    /// Text of a range: rows end with a newline, and engine lines continuing a
    /// row across a wide gap are separated by a space.
    pub fn text(&self, range: Range<usize>) -> String {
        let mut text = String::new();
        for (index, (row, selected)) in self.rows_in(range).enumerate() {
            if index > 0 {
                text.push('\n');
            }
            for i in selected.clone() {
                let character = self.glyphs[i].character;
                if i > selected.start
                    && row.joins.binary_search(&i).is_ok()
                    && !character.is_whitespace()
                    && !self.glyphs[i - 1].character.is_whitespace()
                {
                    text.push(' ');
                }
                text.push(character);
            }
        }
        text
    }

    /// User-space rectangles a highlight of the range fills: one per row,
    /// spanning the selected glyphs. Successive rows of a paragraph share an
    /// edge, so a translucent fill neither breaks nor doubles between them.
    pub fn highlight_rects(&self, range: Range<usize>) -> Vec<Rect> {
        self.rects(range, |row| row.band)
    }

    /// User-space rectangles hugging the glyph boxes of each row of the range,
    /// for marks drawn at the text's own edges such as underline and strikeout.
    pub fn line_rects(&self, range: Range<usize>) -> Vec<Rect> {
        self.rects(range, |row| row.extent)
    }

    /// Gathers engine lines into rows and settles their highlight geometry.
    /// Lines too steep to read along a page axis are left out.
    pub(crate) fn from_lines(lines: impl IntoIterator<Item = Line>) -> Self {
        let mut text = Self::default();
        for line in lines {
            let Some(frame) = Frame::of(line.advance, line.descent) else {
                continue;
            };
            let start = text.glyphs.len();
            text.glyphs
                .extend(line.glyphs.into_iter().filter(|glyph| glyph.bounds.valid()));
            let glyphs = &text.glyphs[start..];
            let (Some(along), Some(extent)) = (
                glyphs
                    .iter()
                    .map(|glyph| frame.along.span(glyph.bounds))
                    .reduce(Span::union),
                glyphs
                    .iter()
                    .map(|glyph| frame.down.span(glyph.bounds))
                    .reduce(Span::union),
            ) else {
                continue;
            };
            let end = text.glyphs.len();
            match text.rows.last_mut() {
                Some(row) if row.continues(frame, along, extent) => {
                    row.joins.push(start);
                    row.glyphs.end = end;
                    row.along = row.along.union(along);
                    row.extent = row.extent.union(extent);
                }
                _ => text.rows.push(Row {
                    glyphs: start..end,
                    joins: Vec::new(),
                    frame,
                    along,
                    extent,
                    band: extent,
                    block: along,
                }),
            }
        }
        settle(&mut text.rows);
        text
    }

    /// Rows a range touches, each with the part of the range inside it.
    fn rows_in(&self, range: Range<usize>) -> impl Iterator<Item = (&Row, Range<usize>)> {
        let (start, end) = (range.start, range.end);
        let first = self.rows.partition_point(|row| row.glyphs.end <= start);
        self.rows[first..]
            .iter()
            .take_while(move |row| row.glyphs.start < end)
            .map(move |row| (row, start.max(row.glyphs.start)..end.min(row.glyphs.end)))
            .filter(|(_, selected)| !selected.is_empty())
    }

    fn rects(&self, range: Range<usize>, across: fn(&Row) -> Span) -> Vec<Rect> {
        let mut rects = Vec::new();
        for (row, selected) in self.rows_in(range) {
            let limit = SEGMENT_GAP * row.extent.len();
            let mut open: Option<Span> = None;
            for glyph in &self.glyphs[selected] {
                let span = row.frame.along.span(glyph.bounds);
                open = Some(match open {
                    Some(segment) if span.low - segment.high > limit => {
                        rects.push(row.frame.rect(segment, across(row)));
                        span
                    }
                    Some(segment) => segment.union(span),
                    None => span,
                });
            }
            if let Some(segment) = open {
                rects.push(row.frame.rect(segment, across(row)));
            }
        }
        rects
    }

    /// The caret nearest a point: the index of the glyph it precedes. A point
    /// above or below its nearest row lands at that row's start or end, so the
    /// gap between paragraphs and the page margins select whole lines. A point
    /// beyond the last row, or before the first, lands at the page's own end or
    /// start, which is how a drag past the page edge clamps to the page.
    pub fn caret(&self, point: Point) -> Option<usize> {
        let row = self.rows.iter().min_by(|a, b| {
            let (a, b) = (a.reach(point), b.reach(point));
            a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1))
        })?;
        let across = row.frame.down.coordinate(point);
        if across < row.band.low {
            return Some(row.glyphs.start);
        }
        if across > row.band.high {
            return Some(row.glyphs.end);
        }
        let along = row.frame.along.coordinate(point);
        Some(
            row.glyphs
                .clone()
                .find(|&i| along < row.frame.along.span(self.glyphs[i].bounds).middle())
                .unwrap_or(row.glyphs.end),
        )
    }
}

impl Row {
    /// Whether a following engine line continues this row on its baseline.
    fn continues(&self, frame: Frame, along: Span, extent: Span) -> bool {
        let shorter = self.extent.len().min(extent.len());
        frame == self.frame
            && self.extent.overlap(extent) >= shorter / 2.
            && along.low >= self.along.high - shorter / 2.
    }

    /// How far a point is from this row: across to its band plus along to its
    /// paragraph, so a short last line still owns the space after it; ties go
    /// to the row the point is beside.
    fn reach(&self, point: Point) -> (f32, f32) {
        let along = self.frame.along.coordinate(point);
        let across = self.frame.down.coordinate(point);
        (
            self.band.distance(across) + self.block.distance(along),
            self.along.distance(along),
        )
    }
}

/// Settles each row's highlight band and the extent of its paragraph.
///
/// The line gap between successive rows is split at its middle, which closes
/// loose leading and trims overlapping font boxes alike. A gap clearly wider
/// than the line gaps around it is a paragraph break and stays open. A row
/// without a neighbour on one side mirrors the half leading of its other side,
/// so every row of a paragraph is equally tall, but it never trims its own glyph
/// boxes or reaches past the middle of an open gap.
fn settle(rows: &mut [Row]) {
    let gaps: Vec<Option<f32>> = rows
        .windows(2)
        .map(|pair| successor_gap(&pair[0], &pair[1]))
        .collect();
    let edges: Vec<Option<f32>> = (0..gaps.len())
        .map(|i| {
            let gap = gaps[i]?;
            let around = gaps[i.saturating_sub(1)..(i + 2).min(gaps.len())]
                .iter()
                .flatten()
                .fold(gap, |least, &other| least.min(other));
            let shorter = rows[i].extent.len().min(rows[i + 1].extent.len());
            (gap <= around + PARAGRAPH_GAP * shorter)
                .then(|| (rows[i].extent.high + rows[i + 1].extent.low) / 2.)
        })
        .collect();
    for (i, row) in rows.iter_mut().enumerate() {
        let extent = row.extent;
        let above = i.checked_sub(1).and_then(|j| edges[j]);
        let below = edges.get(i).copied().flatten();
        let open_above = i.checked_sub(1).and_then(|j| gaps[j]);
        let open_below = gaps.get(i).copied().flatten();
        // A loose edge that is not a paragraph break may take only the half
        // leading its own side leaves open.
        let mirrored = |edge: f32, open: Option<f32>| {
            edge.max(0.)
                .min(open.map_or(f32::INFINITY, |gap| gap.max(0.) / 2.))
        };
        row.band = Span {
            low: above.unwrap_or_else(|| {
                extent.low - mirrored(below.map_or(0., |edge| edge - extent.high), open_above)
            }),
            high: below.unwrap_or_else(|| {
                extent.high + mirrored(above.map_or(0., |edge| extent.low - edge), open_below)
            }),
        };
    }
    let mut start = 0;
    for i in 0..rows.len() {
        if edges.get(i).copied().flatten().is_some() {
            continue;
        }
        if let Some(block) = rows[start..=i]
            .iter()
            .map(|row| row.along)
            .reduce(Span::union)
        {
            for row in &mut rows[start..=i] {
                row.block = block;
            }
        }
        start = i + 1;
    }
}

/// The gap from a row to the next one when the next is the following line of
/// the same text: equally oriented, lower, sharing some of its extent along the
/// advance, and close enough to continue it.
fn successor_gap(row: &Row, next: &Row) -> Option<f32> {
    let shorter = row.extent.len().min(next.extent.len());
    let taller = row.extent.len().max(next.extent.len());
    let gap = next.extent.low - row.extent.high;
    (next.frame == row.frame
        && next.extent.low > row.extent.low
        && next.extent.high > row.extent.high
        && row.along.overlap(next.along) > 0.
        && gap >= -MAX_ROW_OVERLAP * shorter
        && gap <= MAX_LINE_GAP * taller)
        .then_some(gap)
}

/// Selectable text of a page, from MuPDF's structured text in reading order.
pub(crate) fn extract(page: &PdfPage) -> Result<(PageText, Option<String>)> {
    let text = page.to_text_page(TextPageFlags::empty())?.structured();
    let inverse = page
        .ctm()?
        .invert()
        .context("Cannot invert PDF page transform")?;
    let mut lines = Vec::new();
    let mut count = 0;
    for block in text.blocks {
        let TextBlockContent::Text { lines: block_lines } = block.content else {
            continue;
        };
        for line in block_lines {
            count += line.chars.len();
            if count > MAX_GLYPHS {
                return Ok((
                    PageText::default(),
                    Some("Page exceeds the text-selection budget".into()),
                ));
            }
            lines.push(engine_line(line, &inverse));
        }
    }
    Ok((PageText::from_lines(lines), None))
}

/// One structured-text line in user space. Its advance and descent vectors are
/// summed from the character quads, which MuPDF lays out along the baseline
/// and from ascender to descender.
fn engine_line(line: StructuredTextLine, inverse: &Matrix) -> Line {
    let mut advance = (0., 0.);
    let mut descent = (0., 0.);
    let glyphs = line
        .chars
        .into_iter()
        .map(|character| {
            let quad = character.quad;
            let [ul, ur, ll, lr] =
                [quad.ul, quad.ur, quad.ll, quad.lr].map(|p| inverse.transform_xy(p.x, p.y));
            advance.0 += lr.0 - ll.0 + ur.0 - ul.0;
            advance.1 += lr.1 - ll.1 + ur.1 - ul.1;
            descent.0 += ll.0 - ul.0 + lr.0 - ur.0;
            descent.1 += ll.1 - ul.1 + lr.1 - ur.1;
            Glyph {
                character: character.ch,
                bounds: bounds([ul, ur, ll, lr]),
            }
        })
        .collect();
    Line {
        glyphs,
        advance,
        descent,
    }
}

fn bounds(points: [(f32, f32); 4]) -> Rect {
    let left = points.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
    let right = points.iter().map(|p| p.0).fold(f32::NEG_INFINITY, f32::max);
    let bottom = points.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
    let top = points.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);
    Rect {
        x: left,
        y: bottom,
        width: right - left,
        height: top - bottom,
    }
}

/// A closed interval along one axis, low to high.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Span {
    low: f32,
    high: f32,
}

impl Span {
    fn len(self) -> f32 {
        self.high - self.low
    }

    fn middle(self) -> f32 {
        (self.low + self.high) / 2.
    }

    fn union(self, other: Self) -> Self {
        Self {
            low: self.low.min(other.low),
            high: self.high.max(other.high),
        }
    }

    /// Length shared with another interval; negative when they are apart.
    fn overlap(self, other: Self) -> f32 {
        self.high.min(other.high) - self.low.max(other.low)
    }

    /// Distance from a value to the interval; zero inside it.
    fn distance(self, value: f32) -> f32 {
        (self.low - value).max(value - self.high).max(0.)
    }
}

/// A user-space direction that a row advances or descends along.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Axis {
    X,
    NegX,
    Y,
    NegY,
}

impl Axis {
    /// The page axis a vector runs along, if it is within [`AXIS_COS`] of one.
    fn of(vector: (f32, f32)) -> Option<Self> {
        let length = vector.0.hypot(vector.1);
        if length.is_nan() || length <= f32::EPSILON {
            return None;
        }
        let (x, y) = (vector.0 / length, vector.1 / length);
        if x.abs() >= AXIS_COS {
            Some(if x > 0. { Self::X } else { Self::NegX })
        } else if y.abs() >= AXIS_COS {
            Some(if y > 0. { Self::Y } else { Self::NegY })
        } else {
            None
        }
    }

    fn horizontal(self) -> bool {
        matches!(self, Self::X | Self::NegX)
    }

    /// A point's coordinate measured along this direction.
    fn coordinate(self, point: Point) -> f32 {
        match self {
            Self::X => point.x,
            Self::NegX => -point.x,
            Self::Y => point.y,
            Self::NegY => -point.y,
        }
    }

    /// The interval a user-space rectangle covers along this direction.
    fn span(self, rect: Rect) -> Span {
        let (low, high) = if self.horizontal() {
            (rect.x, rect.x + rect.width)
        } else {
            (rect.y, rect.y + rect.height)
        };
        match self {
            Self::X | Self::Y => Span { low, high },
            Self::NegX | Self::NegY => Span {
                low: -high,
                high: -low,
            },
        }
    }

    /// User-space origin and length of an interval measured along this direction.
    fn user(self, span: Span) -> (f32, f32) {
        match self {
            Self::X | Self::Y => (span.low, span.len()),
            Self::NegX | Self::NegY => (-span.high, span.len()),
        }
    }
}

/// The user-space directions a row advances and descends along.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Frame {
    along: Axis,
    down: Axis,
}

impl Frame {
    /// A line's frame from its summed advance and descent. `None` for runs too
    /// steep to read along a page axis. Flat quads fall back to upright text,
    /// whose descent is the advance turned clockwise in y-up user space.
    fn of(advance: (f32, f32), descent: (f32, f32)) -> Option<Self> {
        let along = Axis::of(advance)?;
        let (side, positive, negative, upright) = match along {
            Axis::X => (descent.1, Axis::Y, Axis::NegY, Axis::NegY),
            Axis::NegX => (descent.1, Axis::Y, Axis::NegY, Axis::Y),
            Axis::Y => (descent.0, Axis::X, Axis::NegX, Axis::X),
            Axis::NegY => (descent.0, Axis::X, Axis::NegX, Axis::NegX),
        };
        let down = if side > 0. {
            positive
        } else if side < 0. {
            negative
        } else {
            upright
        };
        Some(Self { along, down })
    }

    /// The user-space rectangle spanning `along` × `across` in this frame.
    fn rect(self, along: Span, across: Span) -> Rect {
        let (a, a_length) = self.along.user(along);
        let (c, c_length) = self.down.user(across);
        if self.along.horizontal() {
            Rect {
                x: a,
                y: c,
                width: a_length,
                height: c_length,
            }
        } else {
            Rect {
                x: c,
                y: a,
                width: c_length,
                height: a_length,
            }
        }
    }
}

/// One visual row: consecutive engine lines sharing a baseline.
#[derive(Clone, Debug)]
struct Row {
    glyphs: Range<usize>,
    /// First glyph of each further engine line that continues this row.
    joins: Vec<usize>,
    frame: Frame,
    /// Extent of the glyphs along the advance.
    along: Span,
    /// Extent of the glyph boxes across the advance, ascender to descender.
    extent: Span,
    /// Extent across the advance that a highlight fills.
    band: Span,
    /// Extent along the advance of the paragraph containing the row.
    block: Span,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Upright text in y-up user space: boxes `width` apart from `x`, spanning
    /// `bottom..bottom + height`.
    fn line(text: &str, x: f32, bottom: f32, width: f32, height: f32) -> Line {
        Line {
            glyphs: text
                .chars()
                .enumerate()
                .map(|(i, character)| Glyph {
                    character,
                    bounds: Rect {
                        x: x + i as f32 * width,
                        y: bottom,
                        width,
                        height,
                    },
                })
                .collect(),
            advance: (1., 0.),
            descent: (0., -1.),
        }
    }

    /// Upright text on a page turned by `/Rotate 90`: it advances along user +y
    /// and descends along user +x, so each further line has a larger x.
    fn turned_line(text: &str, left: f32, y: f32, width: f32, height: f32) -> Line {
        Line {
            glyphs: text
                .chars()
                .enumerate()
                .map(|(i, character)| Glyph {
                    character,
                    bounds: Rect {
                        x: left,
                        y: y + i as f32 * width,
                        width: height,
                        height: width,
                    },
                })
                .collect(),
            advance: (0., 1.),
            descent: (1., 0.),
        }
    }

    fn index_of(text: &PageText, needle: &str) -> usize {
        let characters: String = text.glyphs().iter().map(|g| g.character).collect();
        characters[..characters.find(needle).unwrap()]
            .chars()
            .count()
    }

    /// Each row ends exactly where the next begins, top to bottom.
    fn assert_rows_meet(rects: &[Rect]) {
        for pair in rects.windows(2) {
            assert_eq!(
                pair[0].y,
                pair[1].y + pair[1].height,
                "rows must share one edge: {pair:?}"
            );
        }
    }

    #[test]
    fn a_selection_runs_in_reading_order_across_lines() {
        let text = PageText::from_lines([
            line("abcdef", 0., 100., 10., 14.),
            line("ghijkl", 0., 84., 10., 14.),
            line("mnopqr", 0., 68., 10., 14.),
        ]);
        // From the left half of "d" on the first line to that of "o" on the last.
        let range = text.select(Point::new(31., 107.), Point::new(21., 75.));
        assert_eq!(text.text(range.clone()), "def\nghijkl\nmn");
        // Dragging the other way selects the same text.
        assert_eq!(
            text.select(Point::new(21., 75.), Point::new(31., 107.)),
            range
        );
        let rects = text.highlight_rects(range);
        assert_eq!(rects.len(), 3, "one rectangle per row, not per glyph");
        assert_eq!((rects[0].x, rects[0].width), (30., 30.));
        assert_eq!((rects[1].x, rects[1].width), (0., 60.));
        assert_eq!((rects[2].x, rects[2].width), (0., 20.));
        assert_rows_meet(&rects);
    }

    #[test]
    fn overlapping_font_boxes_meet_at_one_edge() {
        // Tall CJK font boxes on tight leading overlap the next line by 4 pt.
        let text = PageText::from_lines([
            line("一二三四", 0., 100., 14., 20.),
            line("五六七八", 0., 84., 14., 20.),
            line("九十", 0., 68., 14., 20.),
        ]);
        let rects = text.highlight_rects(0..10);
        assert_eq!(rects.len(), 3);
        assert_rows_meet(&rects);
        // The shared edge sits in the middle of each overlap.
        assert_eq!(rects[1].y + rects[1].height, 102.);
        assert_eq!(rects[2].y + rects[2].height, 86.);
        // The outer edges keep the glyph boxes whole.
        assert_eq!(rects[0].y + rects[0].height, 120.);
        assert_eq!(rects[2].y, 68.);
        // Underline and strikeout keep following the glyph boxes themselves.
        let lines = text.line_rects(0..10);
        assert_eq!((lines[0].y, lines[0].height), (100., 20.));
    }

    #[test]
    fn loose_leading_is_closed_but_a_paragraph_break_stays_open() {
        let text = PageText::from_lines([
            line("aaaa", 0., 200., 10., 14.),
            line("bbbb", 0., 178., 10., 14.),
            line("cccc", 0., 156., 10., 14.),
            // A paragraph break: 26 pt between boxes where lines leave 8 pt.
            line("dddd", 0., 116., 10., 14.),
            line("eeee", 0., 94., 10., 14.),
        ]);
        let rects = text.highlight_rects(0..20);
        assert_eq!(rects.len(), 5);
        assert_rows_meet(&rects[..3]);
        assert_rows_meet(&rects[3..]);
        // Every row of both paragraphs carries the same half leading.
        assert!(rects.iter().all(|rect| rect.height == 22.), "{rects:?}");
        let break_gap = rects[2].y - (rects[3].y + rects[3].height);
        assert_eq!(break_gap, 18.);
    }

    #[test]
    fn a_pointer_off_the_text_lands_on_line_and_paragraph_boundaries() {
        let text = PageText::from_lines([
            line("abcdefgh", 0., 200., 10., 14.),
            line("ijkl", 0., 178., 10., 14.),
            line("mnopqrst", 0., 140., 10., 14.),
        ]);
        let start = Point::new(5., 300.);
        // Beyond the end of a short last line, at its height.
        let range = text.select(start, Point::new(70., 185.));
        assert_eq!(text.text(range), "abcdefgh\nijkl");
        // In the gap between paragraphs: the end of the first one.
        let range = text.select(start, Point::new(40., 165.));
        assert_eq!(text.text(range), "abcdefgh\nijkl");
        // Below everything and above everything.
        assert_eq!(text.select(start, Point::new(10., 20.)), 0..20);
        // In the left margin beside the first line: that line's start.
        assert_eq!(text.select(start, Point::new(-30., 207.)), 0..0);
    }

    #[test]
    fn a_pointer_in_a_column_gap_stays_in_its_column() {
        let text = PageText::from_lines([
            line("left one", 0., 200., 10., 14.),
            line("left two", 0., 184., 10., 14.),
            line("left six", 0., 168., 10., 14.),
            line("right one", 120., 205., 10., 14.),
            line("right two", 120., 189., 10., 14.),
            // A new paragraph in the right column, not aligned with the left.
            line("right new", 120., 150., 10., 14.),
        ]);
        let first = index_of(&text, "right one");
        // Inside the right column's paragraph gap, level with "left six".
        let range = text.select(Point::new(122., 207.), Point::new(170., 172.));
        assert_eq!(range.start, first);
        assert_eq!(text.text(range), "right one\nright two");
    }

    #[test]
    fn rotated_pages_read_along_their_own_axis() {
        // `/Rotate 90`: lines advance along +y, and the next line lies at +x.
        let text = PageText::from_lines([
            turned_line("abcdef", 0., 0., 10., 14.),
            turned_line("ghijkl", 16., 0., 10., 14.),
        ]);
        let range = text.select(Point::new(7., 21.), Point::new(23., 16.));
        assert_eq!(text.text(range.clone()), "cdef\ngh");
        let rects = text.highlight_rects(range);
        assert_eq!(rects.len(), 2);
        assert_eq!(rects[0].x + rects[0].width, rects[1].x);
        assert_eq!((rects[0].y, rects[0].height), (20., 40.));
        assert_eq!((rects[1].y, rects[1].height), (0., 20.));
    }

    #[test]
    fn engine_lines_on_one_baseline_form_one_row() {
        let text = PageText::from_lines([
            // MuPDF starts a new line after a wide gap on the same baseline.
            line("Chapter", 0., 100., 10., 14.),
            line("one", 90., 100., 10., 14.),
            // A page number far away on the same baseline.
            line("12", 300., 100., 10., 14.),
            line("next", 0., 84., 10., 14.),
        ]);
        assert_eq!(text.text(0..16), "Chapter one 12\nnext");
        let rects = text.highlight_rects(0..12);
        assert_eq!(rects.len(), 2, "a far gap splits the highlight: {rects:?}");
        assert_eq!((rects[0].x, rects[0].width), (0., 120.));
        assert_eq!((rects[1].x, rects[1].width), (300., 20.));
    }

    #[test]
    fn steep_runs_and_invalid_boxes_are_not_selectable() {
        let mut steep = line("diagonal", 0., 50., 10., 14.);
        steep.advance = (1., 1.);
        let mut broken = line("ab", 0., 100., 10., 14.);
        broken.glyphs[1].bounds.width = f32::NAN;
        let text = PageText::from_lines([broken, steep]);
        assert_eq!(text.glyphs().len(), 1);
        assert_eq!(text.text(0..usize::MAX), "a");
        assert!(text.highlight_rects(1..1).is_empty());
        assert_eq!(
            PageText::default().select(Point::new(0., 0.), Point::new(9., 9.)),
            0..0
        );
    }
}
