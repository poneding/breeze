//! Status controls share the same mutations as keyboard and pointer zoom commands.
use crate::*;
use gpui_kit::component::{
    Icon, Side,
    menu::{DropdownMenu, PopupMenuItem},
};

impl Breeze {
    pub(super) fn status_color(&self, key: &'static str, cx: &App) -> Hsla {
        let p = self.p(cx);
        if self.status_hovered == Some(key) {
            p.text
        } else {
            p.muted
        }
    }

    pub(super) fn status_control<E: IntoElement>(
        &self,
        key: &'static str,
        element: E,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id((key, 0usize))
            .flex()
            .flex_shrink_0()
            .items_center()
            .on_hover(cx.listener(move |this, hovered, _, cx| {
                if *hovered {
                    if this.status_hovered != Some(key) {
                        this.status_hovered = Some(key);
                        cx.notify();
                    }
                } else if this.status_hovered == Some(key) {
                    this.status_hovered = None;
                    cx.notify();
                }
            }))
            .child(element)
            .into_any_element()
    }

    /// One quarter turn per click; the reader's rotation is view-only state.
    pub(super) fn rotate_button(
        &self,
        clockwise: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let label = if clockwise {
            tr!("Rotate 90° right")
        } else {
            tr!("Rotate 90° left")
        };
        let key = if clockwise {
            "rotate-right"
        } else {
            "rotate-left"
        };
        let color = self.status_color(key, cx);
        let button = Button::new(key)
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
            .text_color(color);
        self.status_control(key, button, cx)
    }

    pub(super) fn fullscreen_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let immersive = self.full_screen;
        let color = self.status_color("fullscreen", cx);
        let button = self
            .command(
                "fullscreen",
                if immersive {
                    IconName::Shrink
                } else {
                    IconName::Fullscreen
                },
                tr!("Fullscreen reading"),
                &ToggleFullScreen,
                cx,
            )
            .text_color(color);
        self.status_control("fullscreen", button, cx)
    }

    pub(super) fn layout_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let selected = self.books[self.active].state.layout;
        let color = self.status_color("layout", cx);
        let control = Button::new("layout")
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
            .accessibility_label(tf!("Reading layout: {}", tr!(selected.label())))
            .tooltip_with_action(tr!("Change reading layout"), &CycleLayout, Some("Breeze"))
            .text_color(color)
            .dropdown_menu(move |menu, window, _| {
                menu.check_side(Side::Right)
                    .item(
                        PopupMenuItem::new(tr!("Single page"))
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
                        PopupMenuItem::new(tr!("Two pages"))
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
                        PopupMenuItem::new(tr!("Continuous"))
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
            });
        self.status_control("layout", control, cx)
    }

    pub(super) fn zoom_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.books[self.active].state.zoom;
        let view = cx.entity();
        let (icon, label): (_, String) = match selected {
            Zoom::FitPage => (IconName::Maximize, tr!("Fit page").into()),
            Zoom::FitWidth => (IconName::MoveHorizontal, tr!("Fit width").into()),
            Zoom::ActualSize => (IconName::Scan, tr!("Actual size").into()),
            Zoom::Percent(_) => (IconName::SlidersHorizontal, tr!("Custom zoom").into()),
        };
        let color = self.status_color("zoom", cx);
        let control = Button::new("zoom")
            .ghost()
            .small()
            .icon(icon)
            .label(label)
            .accessibility_label(tr!("Zoom options"))
            .tooltip(tr!("Choose zoom mode"))
            .text_color(color)
            .dropdown_menu(move |menu, window, _| {
                menu.check_side(Side::Right)
                    .item(
                        PopupMenuItem::new(tr!("Fit page"))
                            .icon(IconName::Maximize)
                            .checked(selected == Zoom::FitPage)
                            .action(Box::new(FitPage))
                            .on_click(window.listener_for(&view, |this, _, window, cx| {
                                this.execute(&FitPage, window, cx)
                            })),
                    )
                    .item(
                        PopupMenuItem::new(tr!("Fit width"))
                            .icon(IconName::MoveHorizontal)
                            .checked(selected == Zoom::FitWidth)
                            .action(Box::new(FitWidth))
                            .on_click(window.listener_for(&view, |this, _, window, cx| {
                                this.execute(&FitWidth, window, cx)
                            })),
                    )
                    .item(
                        PopupMenuItem::new(tr!("Actual size"))
                            .icon(IconName::Scan)
                            .checked(selected == Zoom::ActualSize)
                            .action(Box::new(ActualSize))
                            .on_click(window.listener_for(&view, |this, _, window, cx| {
                                this.execute(&ActualSize, window, cx)
                            })),
                    )
                    .item(
                        PopupMenuItem::new(tr!("Custom zoom"))
                            .icon(IconName::SlidersHorizontal)
                            .checked(matches!(selected, Zoom::Percent(_)))
                            .on_click(window.listener_for(&view, |this, _, window, cx| {
                                let percent = this.display_zoom(window).round().clamp(
                                    f32::from(Zoom::MIN_PERCENT),
                                    f32::from(Zoom::MAX_PERCENT),
                                ) as u16;
                                this.set_zoom(Zoom::Percent(percent), cx);
                            })),
                    )
            });
        self.status_control("zoom", control, cx)
    }

    pub(super) fn zoom_percent_button(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let selected = self.books[self.active].state.zoom;
        let current = self.display_zoom(window).round() as u16;
        let view = cx.entity();
        let color = self.status_color("zoom-percent", cx);
        let control = Button::new("zoom-percent")
            .ghost()
            .small()
            .accessibility_label(tr!("Zoom options"))
            .tooltip(tr!("Choose zoom mode"))
            .child(
                row()
                    .gap_1()
                    .child(format!("{current}%"))
                    .child(Icon::new(IconName::ChevronDown).xsmall()),
            )
            .text_color(color)
            .dropdown_menu(move |menu, window, _| {
                let mut menu = menu.check_side(Side::Right);
                for percent in Zoom::PERCENT_OPTIONS {
                    menu = menu.item(
                        PopupMenuItem::new(format!("{percent}%"))
                            .checked(selected == Zoom::Percent(percent))
                            .on_click(window.listener_for(&view, move |this, _, _, cx| {
                                this.set_zoom(Zoom::Percent(percent), cx);
                            })),
                    );
                }
                menu
            });
        self.status_control("zoom-percent", control, cx)
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
        event: &ScrollEvent,
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
        let target = (current + delta + self.reading.zoom_remainder)
            .clamp(f32::from(Zoom::MIN_PERCENT), f32::from(Zoom::MAX_PERCENT));
        let rounded = target.round();
        self.set_zoom(Zoom::Percent(rounded as u16), cx);
        self.reading.zoom_remainder = target - rounded;
    }
}
