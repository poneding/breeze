//! Format-independent reader state. No GPUI or PDF engine types cross here.
use serde::{Deserialize, Serialize};
pub mod annotations;
pub mod library;
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Read,
    Annotate,
    Edit,
}
impl Mode {
    pub fn from_index(index: usize) -> Self {
        match index {
            1 => Self::Annotate,
            2 => Self::Edit,
            _ => Self::Read,
        }
    }
    pub fn index(self) -> usize {
        match self {
            Self::Read => 0,
            Self::Annotate => 1,
            Self::Edit => 2,
        }
    }
}
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum Zoom {
    FitPage,
    FitWidth,
    Percent(u16),
}
impl Zoom {
    pub fn label(self) -> String {
        match self {
            Self::FitPage => "适合页面".into(),
            Self::FitWidth => "适合页宽".into(),
            Self::Percent(n) => format!("{n}%"),
        }
    }
    pub fn step(self, delta: i16) -> Self {
        let current = match self {
            Self::Percent(n) => n as i16,
            _ => 100,
        };
        Self::Percent((current + delta).clamp(25, 200) as u16)
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
    #[default]
    Single,
    Continuous,
    Spread,
}
impl ReadingLayout {
    pub const ALL: [Self; 3] = [Self::Single, Self::Continuous, Self::Spread];
    pub fn label(self) -> &'static str {
        match self {
            Self::Single => "单页",
            Self::Continuous => "连续",
            Self::Spread => "双页",
        }
    }
    pub fn pages(self, page: usize, total: usize) -> std::ops::Range<usize> {
        let page = page.min(total.saturating_sub(1));
        let start = if self == Self::Spread {
            page / 2 * 2
        } else {
            page
        };
        start
            ..start
                .saturating_add(if self == Self::Spread { 2 } else { 1 })
                .min(total)
    }
    pub fn step(self, page: usize, total: usize, forward: bool) -> usize {
        let stride = if self == Self::Spread { 2 } else { 1 };
        let start = if self == Self::Spread {
            page / 2 * 2
        } else {
            page
        };
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
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ReaderState {
    pub navigation: PageNavigation,
    pub zoom: Zoom,
    #[serde(default)]
    pub layout: ReadingLayout,
    pub mode: Mode,
}
impl Default for ReaderState {
    fn default() -> Self {
        Self {
            navigation: PageNavigation::default(),
            zoom: Zoom::FitPage,
            layout: ReadingLayout::Single,
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
        assert_eq!(Zoom::Percent(25).step(-25), Zoom::Percent(25));
        assert_eq!(Zoom::Percent(200).step(25), Zoom::Percent(200));
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
    fn mode_roundtrip() {
        for i in 0..3 {
            assert_eq!(Mode::from_index(i).index(), i);
        }
    }
}
