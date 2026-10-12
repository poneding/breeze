//! Status controls share the same mutations as keyboard and pointer zoom commands.
use crate::*;
use gpui_kit::component::{
    Side,
    menu::{DropdownMenu, PopupMenuItem},
};

impl Breeze {
    /// One quarter turn per click; the reader's rotation is view-only state.
    pub(super) fn rotate_button(&self, clockwise: bool, cx: &mut Context<Self>) -> Button {
        let label = if clockwise {
            tr!("向右旋转 90 度")
        } else {
            tr!("向左旋转 90 度")
        };
        Button::new(if clockwise {
            "rotate-right"
        } else {
            "rotate-left"
        })
        .ghost()
        .small()
        .icon(if clockwise {
            IconName::RotateCw
        } else {
            IconName::RotateCcw
        })
        .accessibility_label(label)
        .tooltip(label)
        .on_click(cx.listener(move |this, _, _, cx| this.rotate(clockwise, cx)))
    }

    pub(super) fn fullscreen_button(&self, cx: &mut Context<Self>) -> Button {
        let immersive = self.full_screen;
        self.command(
            "fullscreen",
            if immersive {
                IconName::Shrink
            } else {
                IconName::Fullscreen
            },
            tr!("全屏阅读"),
            &ToggleFullScreen,
            cx,
        )
    }

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
                IconName::File
            })
            .label(tr!(selected.label()))
            .accessibility_label(tf!("阅读布局：{}", tr!(selected.label())))
            .tooltip_with_action(tr!("切换阅读布局"), &CycleLayout, Some("Breeze"))
            .dropdown_menu(move |menu, window, _| {
                menu.check_side(Side::Right)
                    .item(
                        PopupMenuItem::new(tr!("单页"))
                            .icon(IconName::File)
                            .checked(!selected.is_spread())
                            .on_click(window.listener_for(&view, |this, _, _, cx| {
                                this.set_layout(
                                    this.books[this.active].state.layout.with_spread(false),
                                    cx,
                                );
                            })),
                    )
                    .item(
                        PopupMenuItem::new(tr!("双页"))
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
                        PopupMenuItem::new(tr!("连续"))
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
        let (icon, label): (_, String) = match selected {
            Zoom::FitPage => (IconName::Maximize, tr!("适合页面").into()),
            Zoom::FitWidth => (IconName::MoveHorizontal, tr!("适合宽度").into()),
            Zoom::ActualSize => (IconName::Scan, tr!("实际大小").into()),
            Zoom::Percent(_) => (IconName::SlidersHorizontal, tr!("自定义比例").into()),
        };
        Button::new("zoom")
            .ghost()
            .small()
            .icon(icon)
            .label(label)
            .accessibility_label(tr!("缩放选项"))
            .tooltip(tr!("选择缩放方式"))
            .dropdown_menu(move |menu, window, _| {
                menu.check_side(Side::Right)
                    .item(
                        PopupMenuItem::new(tr!("适合页面"))
                            .icon(IconName::Maximize)
                            .checked(selected == Zoom::FitPage)
                            .action(Box::new(FitPage))
                            .on_click(window.listener_for(&view, |this, _, window, cx| {
                                this.execute(&FitPage, window, cx)
                            })),
                    )
                    .item(
                        PopupMenuItem::new(tr!("适合宽度"))
                            .icon(IconName::MoveHorizontal)
                            .checked(selected == Zoom::FitWidth)
                            .action(Box::new(FitWidth))
                            .on_click(window.listener_for(&view, |this, _, window, cx| {
                                this.execute(&FitWidth, window, cx)
                            })),
                    )
                    .item(
                        PopupMenuItem::new(tr!("实际大小"))
                            .icon(IconName::Scan)
                            .checked(selected == Zoom::ActualSize)
                            .action(Box::new(ActualSize))
                            .on_click(window.listener_for(&view, |this, _, window, cx| {
                                this.execute(&ActualSize, window, cx)
                            })),
                    )
                    .item(
                        PopupMenuItem::new(tr!("自定义比例"))
                            .icon(IconName::SlidersHorizontal)
                            .checked(matches!(selected, Zoom::Percent(_)))
                            .on_click(window.listener_for(&view, |this, _, window, cx| {
                                let percent =
                                    this.display_zoom(window).round().clamp(25., 200.) as u16;
                                this.set_zoom(Zoom::Percent(percent), cx);
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
        Zoom::step_percent(self.display_zoom(window).round(), delta > 0)
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
            ScrollDelta::Lines(delta) => {
                if delta.y.is_finite() && delta.y != 0. {
                    self.set_zoom(
                        Zoom::step_percent(self.display_zoom(window).round(), delta.y > 0.),
                        cx,
                    );
                }
                return;
            }
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
