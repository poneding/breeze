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
    pub tint: Hsla,
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
            tint: theme.list_active,
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
pub fn eyebrow(text: impl Into<SharedString>, p: Palette) -> Div {
    div()
        .text_size(px(11.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(p.muted)
        .child(text.into())
}
