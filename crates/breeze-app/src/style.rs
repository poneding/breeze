use gpui_kit::component::tab::Tab;
use gpui_kit::component::{Theme, ThemeConfig};
use gpui_kit::*;
use std::rc::Rc;
#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Hsla,
    pub panel: Hsla,
    pub sidebar: Hsla,
    pub canvas: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    pub line: Hsla,
    pub accent: Hsla,
    /// The theme's base blue, used by the release banner so a new version is
    /// noticeable without borrowing a success, warning or danger meaning.
    pub notice: Hsla,
}
impl Palette {
    pub fn from_theme(cx: &App) -> Self {
        let theme = Theme::global(cx);
        Self {
            bg: theme.background,
            panel: theme.title_bar,
            sidebar: theme.sidebar,
            canvas: theme.muted,
            text: theme.foreground,
            muted: theme.muted_foreground,
            line: theme.border,
            accent: theme.primary,
            notice: theme.blue,
        }
    }
}
pub fn apply(config: &Rc<ThemeConfig>, cx: &mut App) {
    Theme::update(cx, |theme| {
        theme.apply_config(config);
        // Preserve the reader's compact control density without overriding Kit colors.
        theme.font_size = px(15.);
        theme.radius = px(5.);
        theme.radius_lg = px(7.);
    });
}
/// Height of the reader toolbar and of the navigation and document-panel
/// headers aligned with it. Kept equal to the title bar so the two chrome rows
/// above the document read as one bar.
pub const TOOLBAR_HEIGHT: f32 = 34.;
/// The hover fill every ghost-style control shares: the theme accent, softened
/// in dark mode exactly as Kit softens its own ghost-button hover. List rows
/// reuse it instead of a list-specific fill so a row and a button read alike.
pub fn accent_hover(cx: &App) -> Hsla {
    let theme = Theme::global(cx);
    if theme.is_dark() {
        theme.accent.opacity(0.5)
    } else {
        theme.accent
    }
}
pub fn row() -> Div {
    div().flex().flex_row().items_center()
}
pub fn column() -> Div {
    div().flex().flex_col()
}
/// Side of a square icon-only tab, matching a `small` Kit icon button.
pub const ICON_TAB_SIZE: f32 = 24.;
/// Give an icon-only tab a square the size of the icon buttons beside it.
///
/// A Kit icon tab sizes itself from an inner box that is `flex_1()` with a fixed
/// width; in an auto-width tab bar the flex basis wins, so the tab collapses
/// onto the glyph and neighbouring icons keep only the bar's own gap. The bar's
/// `small` tabs are already 24 px high, so stating the width makes them square.
pub fn icon_tab(tab: Tab) -> Tab {
    tab.w(px(ICON_TAB_SIZE))
}
pub fn eyebrow(text: impl Into<SharedString>, p: Palette) -> Div {
    div()
        .text_size(px(11.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(p.muted)
        .child(text.into())
}
