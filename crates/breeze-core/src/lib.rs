//! Format-independent prototype state. No GPUI or PDF engine types cross here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
#[derive(Clone, Copy, Debug, PartialEq)]
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
#[derive(Clone, Debug)]
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
#[derive(Clone, Debug)]
pub struct ReaderState {
    pub navigation: PageNavigation,
    pub zoom: Zoom,
    pub mode: Mode,
}
impl Default for ReaderState {
    fn default() -> Self {
        Self {
            navigation: PageNavigation::default(),
            zoom: Zoom::FitPage,
            mode: Mode::Read,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
