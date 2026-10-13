//! Format-independent reader state. No GPUI or PDF engine types cross here.
use serde::{Deserialize, Serialize};
pub mod annotations;
pub mod library;
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Read,
    Annotate,
}
impl Mode {
    pub fn from_index(index: usize) -> Self {
        match index {
            1 => Self::Annotate,
            _ => Self::Read,
        }
    }
    pub fn index(self) -> usize {
        match self {
            Self::Read => 0,
            Self::Annotate => 1,
        }
    }
}
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum Zoom {
    FitPage,
    FitWidth,
    ActualSize,
    Percent(u16),
}
impl Zoom {
    pub fn label(self) -> String {
        match self {
            Self::FitPage => "适合页面".into(),
            Self::FitWidth => "适合宽度".into(),
            Self::ActualSize => "实际大小".into(),
            Self::Percent(_) => "自定义比例".into(),
        }
    }
    /// Move to the next grid boundary in the requested direction, including from fit scales.
    pub fn step_percent(current: f32, forward: bool) -> Self {
        let step = if forward {
            (current / 25.).floor() + 1.
        } else {
            (current / 25.).ceil() - 1.
        };
        Self::Percent((step * 25.).clamp(25., 200.) as u16)
    }
}
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PageNavigation {
    page: usize,
    total: usize,
}
impl Default for PageNavigation {
    fn default() -> Self {
        Self { page: 0, total: 1 }
    }
}
impl PageNavigation {
    pub fn page(&self) -> usize {
        self.page
    }
    pub fn total(&self) -> usize {
        self.total
    }
    pub fn set_total(&mut self, total: usize) {
        self.total = total.max(1);
        self.page = self.page.min(self.total - 1);
    }
    pub fn go_to(&mut self, page: usize) {
        self.page = page.min(self.total - 1);
    }
    pub fn next(&mut self) {
        self.go_to(self.page.saturating_add(1));
    }
    pub fn previous(&mut self) {
        self.page = self.page.saturating_sub(1);
    }
    pub fn jump(&mut self, text: &str) -> Result<(), &'static str> {
        let page = text.trim().parse::<usize>().map_err(|_| "请输入有效页码")?;
        if page == 0 || page > self.total {
            return Err("页码超出文档范围");
        }
        self.page = page - 1;
        Ok(())
    }
}
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ReadingLayout {
    Single,
    #[default]
    Continuous,
    Spread,
    ContinuousSpread,
}
impl ReadingLayout {
    pub const ALL: [Self; 4] = [
        Self::Single,
        Self::Spread,
        Self::Continuous,
        Self::ContinuousSpread,
    ];
    pub fn is_continuous(self) -> bool {
        matches!(self, Self::Continuous | Self::ContinuousSpread)
    }
    pub fn is_spread(self) -> bool {
        matches!(self, Self::Spread | Self::ContinuousSpread)
    }
    pub fn columns(self) -> usize {
        if self.is_spread() { 2 } else { 1 }
    }
    pub fn with_spread(self, spread: bool) -> Self {
        match (spread, self.is_continuous()) {
            (false, false) => Self::Single,
            (true, false) => Self::Spread,
            (false, true) => Self::Continuous,
            (true, true) => Self::ContinuousSpread,
        }
    }
    pub fn with_continuous(self, continuous: bool) -> Self {
        (if continuous {
            Self::Continuous
        } else {
            Self::Single
        })
        .with_spread(self.is_spread())
    }
    pub fn row(self, page: usize) -> usize {
        page / self.columns()
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Single => "单页",
            Self::Continuous => "连续单页",
            Self::ContinuousSpread => "连续双页",
            Self::Spread => "双页",
        }
    }
    pub fn pages(self, page: usize, total: usize) -> std::ops::Range<usize> {
        let page = page.min(total.saturating_sub(1));
        let start = if self.is_spread() { page / 2 * 2 } else { page };
        start
            ..start
                .saturating_add(if self.is_spread() { 2 } else { 1 })
                .min(total)
    }
    pub fn step(self, page: usize, total: usize, forward: bool) -> usize {
        let stride = if self.is_spread() { 2 } else { 1 };
        let start = if self.is_spread() { page / 2 * 2 } else { page };
        if forward {
            let next = start.saturating_add(stride);
            if next < total { next } else { page }
        } else if start == 0 {
            page
        } else {
            start.saturating_sub(stride)
        }
    }
}
/// View-only page rotation layered on top of each page's own `/Rotate`.
///
/// It belongs to the reading state, is stored per document, and is never
/// written back into the source file or into annotation geometry.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Rotation {
    #[default]
    None,
    /// One clockwise quarter turn.
    Right,
    Half,
    /// One counterclockwise quarter turn, stored as 270 degrees clockwise.
    Left,
}
impl Rotation {
    pub fn degrees(self) -> i32 {
        match self {
            Self::None => 0,
            Self::Right => 90,
            Self::Half => 180,
            Self::Left => 270,
        }
    }
    pub fn clockwise(self) -> Self {
        match self {
            Self::None => Self::Right,
            Self::Right => Self::Half,
            Self::Half => Self::Left,
            Self::Left => Self::None,
        }
    }
    pub fn counterclockwise(self) -> Self {
        match self {
            Self::None => Self::Left,
            Self::Left => Self::Half,
            Self::Half => Self::Right,
            Self::Right => Self::None,
        }
    }
    /// Quarter turns exchange the page's width and height.
    pub fn swaps_axes(self) -> bool {
        matches!(self, Self::Right | Self::Left)
    }
    /// Size after rotating a top-left-origin display size.
    pub fn apply(self, size: (f32, f32)) -> (f32, f32) {
        if self.swaps_axes() {
            (size.1, size.0)
        } else {
            size
        }
    }
    /// Rotates a top-left-origin display point into the rotated view.
    pub fn map(self, size: (f32, f32), p: annotations::Point) -> annotations::Point {
        use annotations::Point;
        match self {
            Self::None => p,
            Self::Right => Point::new(size.1 - p.y, p.x),
            Self::Half => Point::new(size.0 - p.x, size.1 - p.y),
            Self::Left => Point::new(p.y, size.0 - p.x),
        }
    }
    /// Inverse of [`Rotation::map`] for the unrotated display size.
    pub fn unmap(self, size: (f32, f32), p: annotations::Point) -> annotations::Point {
        use annotations::Point;
        match self {
            Self::None => p,
            Self::Right => Point::new(p.y, size.1 - p.x),
            Self::Half => Point::new(size.0 - p.x, size.1 - p.y),
            Self::Left => Point::new(size.0 - p.y, p.x),
        }
    }
}
/// Last chosen view settings seed newly opened documents.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ReadingPreferences {
    pub layout: ReadingLayout,
    pub zoom: Zoom,
}
impl Default for ReadingPreferences {
    fn default() -> Self {
        Self {
            layout: ReadingLayout::default(),
            zoom: Zoom::FitPage,
        }
    }
}
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ReaderState {
    pub navigation: PageNavigation,
    pub zoom: Zoom,
    #[serde(default)]
    pub layout: ReadingLayout,
    #[serde(default)]
    pub rotation: Rotation,
    pub mode: Mode,
}
impl Default for ReaderState {
    fn default() -> Self {
        Self {
            navigation: PageNavigation::default(),
            zoom: Zoom::FitPage,
            layout: ReadingLayout::default(),
            rotation: Rotation::default(),
            mode: Mode::Read,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spread_groups_and_steps_without_phantom_pages() {
        let layout = ReadingLayout::Spread;
        assert_eq!(layout.pages(1, 5), 0..2);
        assert_eq!(layout.pages(4, 5), 4..5);
        assert_eq!(
            layout.pages(usize::MAX - 1, usize::MAX),
            usize::MAX - 1..usize::MAX
        );
        assert_eq!(layout.pages(99, 0), 0..0);
        assert_eq!(layout.step(1, 5, true), 2);
        assert_eq!(layout.step(2, 5, true), 4);
        assert_eq!(layout.step(4, 5, true), 4);
        assert_eq!(layout.step(2, 4, true), 2);
        assert_eq!(layout.step(1, 4, false), 1);
        assert_eq!(layout.step(4, 5, false), 2);
        for layout in [ReadingLayout::Single, ReadingLayout::Continuous] {
            assert_eq!(layout.step(1, 5, true), 2);
            assert_eq!(layout.step(1, 5, false), 0);
        }
    }
    #[test]
    fn layout_choices_are_independent_and_default_to_continuous_single() {
        assert_eq!(ReaderState::default().layout, ReadingLayout::Continuous);
        for layout in ReadingLayout::ALL {
            for spread in [false, true] {
                let next = layout.with_spread(spread);
                assert_eq!(next.is_spread(), spread);
                assert_eq!(next.is_continuous(), layout.is_continuous());
            }
            for continuous in [false, true] {
                let next = layout.with_continuous(continuous);
                assert_eq!(next.is_continuous(), continuous);
                assert_eq!(next.is_spread(), layout.is_spread());
            }
        }
        let layout = ReadingLayout::ContinuousSpread;
        assert_eq!(layout.pages(1, 5), 0..2);
        assert_eq!(layout.pages(4, 5), 4..5);
        assert_eq!(layout.row(4), 2);
        assert_eq!(layout.step(1, 5, true), 2);
        assert_eq!(layout.step(4, 5, true), 4);
        assert_eq!(layout.step(4, 5, false), 2);
    }
    #[test]
    fn navigation_clamps() {
        let mut n = PageNavigation::default();
        n.previous();
        assert_eq!(n.page(), 0);
        n.set_total(8);
        n.go_to(999);
        assert_eq!(n.page(), 7);
        n.next();
        assert_eq!(n.page(), 7);
        n.set_total(2);
        assert_eq!(n.page(), 1);
        n.set_total(0);
        assert_eq!(n.total(), 1);
    }
    #[test]
    fn jump_rejects_bad_input() {
        let mut n = PageNavigation::default();
        n.set_total(8);
        for bad in ["0", "-1", "9", "abc", ""] {
            assert!(n.jump(bad).is_err());
        }
        n.jump(" 8 ").unwrap();
        assert_eq!(n.page(), 7);
    }
    #[test]
    fn zoom_bounded() {
        for (current, smaller, larger) in [
            (167., 150, 175),
            (175., 150, 200),
            (174.9, 150, 175),
            (175.1, 175, 200),
            (25., 25, 50),
            (200., 175, 200),
            (12., 25, 25),
            (260., 200, 200),
        ] {
            assert_eq!(Zoom::step_percent(current, false), Zoom::Percent(smaller));
            assert_eq!(Zoom::step_percent(current, true), Zoom::Percent(larger));
        }
    }
    #[test]
    fn tabs_do_not_share_navigation() {
        let a = ReaderState::default();
        let mut b = a.clone();
        b.navigation.set_total(8);
        b.navigation.go_to(5);
        assert_eq!(a.navigation.page(), 0);
        assert_eq!(b.navigation.page(), 5);
    }
    #[test]
    fn rotation_turns_both_ways_and_maps_its_own_points() {
        use annotations::Point;
        let mut rotation = Rotation::None;
        for (turns, degrees) in [(0, 0), (1, 90), (2, 180), (3, 270)] {
            assert_eq!(rotation.degrees(), degrees);
            assert_eq!(rotation.swaps_axes(), turns % 2 == 1);
            assert_eq!(rotation.apply((200., 300.)), {
                if turns % 2 == 1 {
                    (300., 200.)
                } else {
                    (200., 300.)
                }
            });
            let size = (200., 300.);
            for p in [
                Point::new(0., 0.),
                Point::new(50., 90.),
                Point::new(200., 300.),
            ] {
                let view = rotation.map(size, p);
                assert_eq!(rotation.unmap(size, view), p);
                let view_size = rotation.apply(size);
                assert!((0. ..=view_size.0).contains(&view.x));
                assert!((0. ..=view_size.1).contains(&view.y));
            }
            rotation = rotation.clockwise();
        }
        assert_eq!(rotation, Rotation::None);
        for rotation in [
            Rotation::None,
            Rotation::Right,
            Rotation::Half,
            Rotation::Left,
        ] {
            assert_eq!(rotation.clockwise().counterclockwise(), rotation);
            assert_eq!(rotation.counterclockwise().clockwise(), rotation);
        }
        // A clockwise turn sends the display top-left corner to the view top-right.
        assert_eq!(
            Rotation::Right.map((200., 300.), Point::new(0., 0.)),
            Point::new(300., 0.)
        );
    }
    #[test]
    fn mode_roundtrip() {
        for i in 0..2 {
            assert_eq!(Mode::from_index(i).index(), i);
        }
    }
}
