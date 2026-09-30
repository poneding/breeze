use crate::appearance::ColorScheme;
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::*;
#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Hsla,
    pub panel: Hsla,
    pub canvas: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    pub line: Hsla,
    pub accent: Hsla,
    pub tint: Hsla,
}
impl Palette {
    pub fn new(dark: bool, scheme: ColorScheme) -> Self {
        let [bg, panel, canvas, text, muted, line, accent, tint] =
            scheme.colors(dark).map(|c| rgb(c).into());
        Self {
            bg,
            panel,
            canvas,
            text,
            muted,
            line,
            accent,
            tint,
        }
    }
}
pub fn apply(dark: bool, scheme: ColorScheme, cx: &mut App) {
    Theme::change(
        if dark {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        },
        None,
        cx,
    );
    let p = Palette::new(dark, scheme);
    Theme::update(cx, |t| {
        t.font_size = px(15.);
        t.radius = px(5.);
        t.radius_lg = px(7.);
        t.colors.background = p.bg;
        t.colors.foreground = p.text;
        t.colors.title_bar = p.panel;
        t.colors.title_bar_border = p.line;
        t.colors.primary = p.accent;
        t.colors.primary_hover = p.accent;
        t.colors.primary_active = p.accent;
        t.colors.primary_foreground = if dark { p.canvas } else { rgb(0xffffff).into() };
        t.colors.button_primary = p.accent;
        t.colors.button_primary_hover = p.accent;
        t.colors.button_primary_active = p.accent;
        t.colors.button_primary_foreground = t.colors.primary_foreground;
        t.colors.button_foreground = p.text;
        t.colors.button_hover = p.tint;
        t.colors.border = p.line;
        t.colors.input = p.line;
        t.colors.secondary = p.panel;
        t.colors.secondary_foreground = p.text;
        t.colors.accent = p.tint;
        t.colors.accent_foreground = p.text;
        t.colors.muted = p.panel;
        t.colors.muted_foreground = p.muted;
        t.colors.popover = p.bg;
        t.colors.popover_foreground = p.text;
        t.colors.tab_bar = p.panel;
        t.colors.tab = p.panel;
        t.colors.tab_active = p.bg;
        t.colors.tab_foreground = p.muted;
        t.colors.tab_active_foreground = p.text;
        t.colors.list_active = p.tint;
        t.colors.list_active_border = p.line;
        t.colors.list_hover = p.tint;
        t.colors.selection = p.tint;
        t.colors.ring = p.accent;
        t.colors.drag_border = p.accent;
    });
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
