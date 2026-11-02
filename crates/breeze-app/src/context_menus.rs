//! Native-style contextual commands for tabs and document content.

use crate::*;
use gpui_kit::component::{
    Icon, Side,
    menu::{PopupMenu, PopupMenuItem},
};

/// The document-wide controls available in the current content surface.
#[derive(Clone, Copy)]
pub(super) enum ContentMenu {
    Pdf {
        layout: ReadingLayout,
        zoom: Zoom,
        displayed_zoom: u16,
        full_screen: bool,
    },
    Reflow {
        epub: bool,
        spread: bool,
        full_width: bool,
        font_size: u8,
        full_screen: bool,
    },
}

#[derive(Clone, Copy)]
struct PdfMenuState {
    layout: ReadingLayout,
    zoom: Zoom,
    displayed_zoom: u16,
    full_screen: bool,
}

#[derive(Clone, Copy)]
struct ReflowMenuState {
    epub: bool,
    spread: bool,
    full_width: bool,
    font_size: u8,
    full_screen: bool,
}

/// Build the menu for one document tab.
pub(super) fn tab(
    menu: PopupMenu,
    view: Entity<Breeze>,
    index: usize,
    can_reveal: bool,
    window: &Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let reveal_view = view.clone();
    let close_view = view.clone();
    menu.action_context(view.read(cx).focus.clone())
        .item(
            PopupMenuItem::new(tr!(reveal::label()))
                .icon(IconName::FolderOpen)
                .disabled(!can_reveal)
                .on_click(window.listener_for(&reveal_view, move |this, _, _, cx| {
                    this.reveal_book(index, cx);
                })),
        )
        .item(
            PopupMenuItem::new(tr!("Close tab"))
                .icon(IconName::X)
                .action(Box::new(CloseTab))
                .on_click(window.listener_for(&close_view, move |this, _, _, cx| {
                    this.close_book(index, cx);
                })),
        )
}

/// Build the document viewport menu from the same mutations used by its
/// status bar, so pointer context and visible controls cannot drift apart.
pub(super) fn content(
    menu: PopupMenu,
    view: Entity<Breeze>,
    content: ContentMenu,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let menu = menu.action_context(view.read(cx).focus.clone());
    let show_status_bar = view.read(cx).status_bar_visible;
    let menu = match content {
        ContentMenu::Pdf {
            layout,
            zoom,
            displayed_zoom,
            full_screen,
        } => pdf(
            menu,
            view.clone(),
            PdfMenuState {
                layout,
                zoom,
                displayed_zoom,
                full_screen,
            },
            window,
            cx,
        ),
        ContentMenu::Reflow {
            epub,
            spread,
            full_width,
            font_size,
            full_screen,
        } => reflow(
            menu,
            view.clone(),
            ReflowMenuState {
                epub,
                spread,
                full_width,
                font_size,
                full_screen,
            },
            window,
        ),
    };
    status_bar_item(menu, view, show_status_bar, window)
}

fn pdf(
    mut menu: PopupMenu,
    view: Entity<Breeze>,
    state: PdfMenuState,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let PdfMenuState {
        layout,
        zoom,
        displayed_zoom,
        full_screen,
    } = state;
    menu = action_item(
        menu,
        tr!("Go to page"),
        IconName::Hash,
        &FocusPage,
        None,
        &view,
        window,
    )
    .separator();
    menu = action_item(
        menu,
        tr!("Rotate 90° left"),
        IconName::RotateCcw,
        &RotateLeft,
        None,
        &view,
        window,
    );
    menu = action_item(
        menu,
        tr!("Rotate 90° right"),
        IconName::RotateCw,
        &RotateRight,
        None,
        &view,
        window,
    );
    menu = layout_menu(menu, view.clone(), layout, window, cx);
    menu = zoom_menu(menu, view.clone(), zoom, displayed_zoom, window, cx);
    action_item(
        menu,
        tr!("Fullscreen reading"),
        if full_screen {
            IconName::Shrink
        } else {
            IconName::Fullscreen
        },
        &ToggleFullScreen,
        None,
        &view,
        window,
    )
}

fn layout_menu(
    menu: PopupMenu,
    view: Entity<Breeze>,
    selected: ReadingLayout,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let single_view = view.clone();
    let spread_view = view.clone();
    let continuous_view = view;
    menu.submenu_with_icon(
        Some(Icon::new(IconName::LayoutDashboard)),
        tr!("Change reading layout"),
        window,
        cx,
        move |menu, window, _| {
            menu.item(
                PopupMenuItem::new(tr!("Single page"))
                    .icon(IconName::File)
                    .checked(!selected.is_spread())
                    .on_click(window.listener_for(&single_view, |this, _, _, cx| {
                        let layout = this.books[this.active].state.layout.with_spread(false);
                        this.set_layout(layout, cx);
                    })),
            )
            .item(
                PopupMenuItem::new(tr!("Two pages"))
                    .icon(IconName::Columns2)
                    .checked(selected.is_spread())
                    .on_click(window.listener_for(&spread_view, |this, _, _, cx| {
                        let layout = this.books[this.active].state.layout.with_spread(true);
                        this.set_layout(layout, cx);
                    })),
            )
            .separator()
            .item(
                PopupMenuItem::new(tr!("Continuous"))
                    .icon(IconName::GalleryVertical)
                    .checked(selected.is_continuous())
                    .on_click(window.listener_for(&continuous_view, |this, _, _, cx| {
                        let layout = this.books[this.active].state.layout;
                        this.set_layout(layout.with_continuous(!layout.is_continuous()), cx);
                    })),
            )
        },
    )
}

fn zoom_menu(
    menu: PopupMenu,
    view: Entity<Breeze>,
    selected: Zoom,
    displayed_zoom: u16,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let step_out_view = view.clone();
    let step_in_view = view.clone();
    let fit_page_view = view.clone();
    let fit_width_view = view.clone();
    let actual_size_view = view.clone();
    let custom_view = view.clone();
    menu.submenu_with_icon(
        Some(Icon::new(IconName::SlidersHorizontal)),
        tr!("Zoom options"),
        window,
        cx,
        move |mut menu, window, _| {
            menu = action_item(
                menu,
                tr!("Zoom out"),
                IconName::Minus,
                &ZoomOut,
                None,
                &step_out_view,
                window,
            );
            menu = action_item(
                menu,
                tr!("Zoom in"),
                IconName::Plus,
                &ZoomIn,
                None,
                &step_in_view,
                window,
            );
            menu = action_item(
                menu,
                tr!("Fit page"),
                IconName::Maximize,
                &FitPage,
                Some(selected == Zoom::FitPage),
                &fit_page_view,
                window,
            );
            menu = action_item(
                menu,
                tr!("Fit width"),
                IconName::MoveHorizontal,
                &FitWidth,
                Some(selected == Zoom::FitWidth),
                &fit_width_view,
                window,
            );
            menu = action_item(
                menu,
                tr!("Actual size"),
                IconName::Scan,
                &ActualSize,
                Some(selected == Zoom::ActualSize),
                &actual_size_view,
                window,
            );
            menu = menu
                .item(
                    PopupMenuItem::new(tr!("Custom zoom"))
                        .icon(IconName::SlidersHorizontal)
                        .checked(matches!(selected, Zoom::Percent(_)))
                        .on_click(window.listener_for(&custom_view, move |this, _, _, cx| {
                            this.set_zoom(Zoom::Percent(displayed_zoom), cx);
                        })),
                )
                .separator();
            for percent in Zoom::PERCENT_OPTIONS {
                let percent_view = view.clone();
                menu = menu.item(
                    PopupMenuItem::new(format!("{percent}%"))
                        .icon(IconName::Percent)
                        .checked(selected == Zoom::Percent(percent))
                        .on_click(window.listener_for(&percent_view, move |this, _, _, cx| {
                            this.set_zoom(Zoom::Percent(percent), cx);
                        })),
                );
            }
            menu
        },
    )
}

fn reflow(
    mut menu: PopupMenu,
    view: Entity<Breeze>,
    state: ReflowMenuState,
    window: &Window,
) -> PopupMenu {
    let ReflowMenuState {
        epub,
        spread,
        full_width,
        font_size,
        full_screen,
    } = state;
    if epub {
        menu = menu.item(
            PopupMenuItem::new(if spread {
                tr!("Single-page reading")
            } else {
                tr!("Two-page reading")
            })
            .icon(IconName::Columns2)
            .checked(spread)
            .on_click(window.listener_for(&view, |this, _, _, cx| {
                this.toggle_reflow_spread(cx);
            })),
        );
    } else {
        menu = menu.item(
            PopupMenuItem::new(if full_width {
                tr!("Use the maximum reading width")
            } else {
                tr!("Use the full window width")
            })
            .icon(if full_width {
                IconName::ChevronsRightLeft
            } else {
                IconName::ChevronsLeftRight
            })
            .checked(full_width)
            .on_click(window.listener_for(&view, |this, _, _, cx| {
                this.toggle_reflow_width(cx);
            })),
        );
    }
    menu = menu.separator();
    menu = menu.item(
        PopupMenuItem::new(tr!("Decrease text size"))
            .icon(IconName::Minus)
            .disabled(font_size <= 12)
            .on_click(window.listener_for(&view, |this, _, _, cx| {
                this.resize_reflow_text(-1, cx);
            })),
    );
    menu = menu.item(
        PopupMenuItem::new(tr!("Increase text size"))
            .icon(IconName::Plus)
            .disabled(font_size >= 32)
            .on_click(window.listener_for(&view, |this, _, _, cx| {
                this.resize_reflow_text(1, cx);
            })),
    );
    menu = menu.separator();
    action_item(
        menu,
        tr!("Fullscreen reading"),
        if full_screen {
            IconName::Shrink
        } else {
            IconName::Fullscreen
        },
        &ToggleFullScreen,
        None,
        &view,
        window,
    )
}

fn status_bar_item(
    menu: PopupMenu,
    view: Entity<Breeze>,
    visible: bool,
    window: &Window,
) -> PopupMenu {
    menu.check_side(Side::Right).separator().item(
        PopupMenuItem::new(tr!("Show status bar"))
            .icon(IconName::PanelBottom)
            .checked(visible)
            .on_click(window.listener_for(&view, |this, _, _, cx| {
                this.toggle_status_bar(cx);
            })),
    )
}

fn action_item(
    menu: PopupMenu,
    label: &'static str,
    icon: IconName,
    action: &dyn Action,
    checked: Option<bool>,
    view: &Entity<Breeze>,
    window: &Window,
) -> PopupMenu {
    let menu_action = action.boxed_clone();
    let click_action = action.boxed_clone();
    let mut item = PopupMenuItem::new(label)
        .icon(icon)
        .action(menu_action)
        .on_click(window.listener_for(view, move |this, _, window, cx| {
            this.execute(click_action.as_ref(), window, cx);
        }));
    if let Some(checked) = checked {
        item = item.checked(checked);
    }
    menu.item(item)
}

impl Breeze {
    fn toggle_status_bar(&mut self, cx: &mut Context<Self>) {
        if let Some(parent) = self.workspace.parent.clone() {
            parent
                .update(cx, |root, cx| {
                    let visible = !root.status_bar_visible;
                    root.set_status_bar_visible(visible, cx);
                })
                .ok();
        } else {
            let visible = !self.status_bar_visible;
            self.set_status_bar_visible(visible, cx);
        }
    }

    fn set_status_bar_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        self.status_bar_visible = visible;
        if let Some(peer) = self.workspace.secondary.clone() {
            peer.update(cx, |peer, _| peer.status_bar_visible = visible);
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests;
