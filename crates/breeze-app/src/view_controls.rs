//! Status controls share the same mutations as keyboard and pointer zoom commands.
use crate::*;
use gpui_kit::component::{
    Side,
    menu::{DropdownMenu, PopupMenuItem},
};

impl Breeze {
    pub(super) fn layout_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let selected = self.books[self.active].state.layout;
        Button::new("layout")
            .ghost()
            .small()
            .icon(if selected.is_continuous() {
                IconName::GalleryVertical
            } else if selected.is_spread() {
                IconName::Columns2
            } else {
                IconName::RectangleVertical
            })
            .label(selected.label())
            .accessibility_label(format!("阅读布局：{}", selected.label()))
            .tooltip_with_action("切换阅读布局", &CycleLayout, Some("Breeze"))
            .dropdown_menu(move |menu, window, _| {
                menu.check_side(Side::Right)
                    .item(
                        PopupMenuItem::new("单页")
                            .icon(IconName::RectangleVertical)
                            .checked(!selected.is_spread())
                            .on_click(window.listener_for(&view, |this, _, _, cx| {
                                this.set_layout(
                                    this.books[this.active].state.layout.with_spread(false),
                                    cx,
                                );
                            })),
                    )
                    .item(
                        PopupMenuItem::new("双页")
                            .icon(IconName::Columns2)
                            .checked(selected.is_spread())
                            .on_click(window.listener_for(&view, |this, _, _, cx| {
                                this.set_layout(
                                    this.books[this.active].state.layout.with_spread(true),
                                    cx,
                                );
                            })),
                    )
                    .separator()
                    .item(
                        PopupMenuItem::new("连续")
                            .icon(IconName::GalleryVertical)
                            .checked(selected.is_continuous())
                            .on_click(window.listener_for(&view, |this, _, _, cx| {
                                let layout = this.books[this.active].state.layout;
                                this.set_layout(
                                    layout.with_continuous(!layout.is_continuous()),
                                    cx,
                                );
                            })),
                    )
            })
    }

    pub(super) fn zoom_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.books[self.active].state.zoom;
        let view = cx.entity();
        let (icon, label) = match selected {
            Zoom::FitPage => (IconName::Maximize, "适合页面".into()),
            Zoom::FitWidth => (IconName::MoveHorizontal, "适合宽度".into()),
            Zoom::Percent(100) => (IconName::Scan, "实际大小".into()),
            Zoom::Percent(_) => (IconName::ZoomIn, selected.label()),
        };
        Button::new("zoom")
            .ghost()
            .small()
            .icon(icon)
            .label(label)
            .accessibility_label("缩放选项")
            .tooltip("选择缩放方式")
            .dropdown_menu(move |menu, window, _| {
                menu.check_side(Side::Right)
                    .item(
                        PopupMenuItem::new("适合页面")
                            .icon(IconName::Maximize)
                            .checked(selected == Zoom::FitPage)
                            .action(Box::new(FitPage))
                            .on_click(window.listener_for(&view, |this, _, window, cx| {
                                this.execute(&FitPage, window, cx)
                            })),
                    )
                    .item(
                        PopupMenuItem::new("适合宽度")
                            .icon(IconName::MoveHorizontal)
                            .checked(selected == Zoom::FitWidth)
                            .action(Box::new(FitWidth))
                            .on_click(window.listener_for(&view, |this, _, window, cx| {
                                this.execute(&FitWidth, window, cx)
                            })),
                    )
                    .item(
                        PopupMenuItem::new("实际大小")
                            .icon(IconName::Scan)
                            .checked(selected == Zoom::Percent(100))
                            .action(Box::new(ActualSize))
                            .on_click(window.listener_for(&view, |this, _, window, cx| {
                                this.execute(&ActualSize, window, cx)
                            })),
                    )
            })
    }

    pub(super) fn set_zoom(&mut self, zoom: Zoom, cx: &mut Context<Self>) {
        self.reading.zoom_remainder = 0.;
        self.reading_preferences.zoom = zoom;
        self.books[self.active].state.zoom = zoom;
        cx.notify();
    }

    pub(super) fn stepped_zoom(&self, delta: i16, window: &Window) -> Zoom {
        Zoom::Percent((self.display_zoom(window).round() + delta as f32).clamp(25., 200.) as u16)
    }

    pub(super) fn scroll_zoom(
        &mut self,
        event: &ScrollWheelEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if self.restoring || self.closing || self.screen != Screen::Reader {
            return;
        }
        let delta = match event.delta {
            ScrollDelta::Lines(delta) => delta.y * 25.,
            ScrollDelta::Pixels(delta) => f32::from(delta.y) * 0.25,
        };
        if !delta.is_finite() || delta == 0. {
            return;
        }
        let current = self.display_zoom(window);
        let target = (current + delta + self.reading.zoom_remainder).clamp(25., 200.);
        let rounded = target.round();
        self.set_zoom(Zoom::Percent(rounded as u16), cx);
        self.reading.zoom_remainder = target - rounded;
    }
}
