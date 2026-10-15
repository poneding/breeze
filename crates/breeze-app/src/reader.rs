use crate::*;
use gpui_kit::base::{ResizeHandleRenderer, ResizeHandleState, Transition, transition};
use gpui_kit::component::{
    Theme, h_resizable,
    menu::{DropdownMenu, PopupMenuItem},
    resizable_panel,
    tab::{Tab, TabBar},
    tooltip::Tooltip,
};
use std::rc::Rc;

pub(super) const NAVIGATION_MIN_WIDTH: f32 = 192.;
pub(super) const NAVIGATION_MAX_WIDTH: f32 = 360.;

pub(super) fn navigation_handle_appearance(p: Palette) -> ResizeHandleRenderer {
    Rc::new(move |handle, window, cx| {
        let (target_length, target_opacity) = match handle.state() {
            ResizeHandleState::Idle => (px(0.), 0.),
            ResizeHandleState::Hovered => (px(20.), 0.35),
            ResizeHandleState::Pressed => (px(28.), 0.6),
            ResizeHandleState::Dragging => (px(44.), 0.9),
        };
        let motion = Theme::global(cx).motion_tokens();
        let policy = Transition::new(motion.duration_fast).easing(motion.easing_move.clone());
        let length = transition(
            "navigation-handle-length",
            target_length,
            policy.clone(),
            window,
            cx,
        );
        let opacity = transition(
            "navigation-handle-opacity",
            target_opacity,
            policy,
            window,
            cx,
        );
        let line_width = px(1.);
        let indicator_width = px(4.);
        Some(
            row()
                .flex_none()
                .w(line_width)
                .h_full()
                .bg(p.line)
                .when(length > px(0.5), |line| {
                    line.child(
                        div()
                            .flex_none()
                            .w(indicator_width)
                            .h(length)
                            // Center the overhang explicitly; flex alignment clamps overflow.
                            .ml((line_width - indicator_width) * 0.5)
                            .rounded(Theme::global(cx).radius_full())
                            .bg(p.muted)
                            .opacity(opacity),
                    )
                })
                .into_any_element(),
        )
    })
}

impl Breeze {
    pub(super) fn reader(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        if !self.books[self.active].is_pdf() {
            return self.reflow_reader(window, cx);
        }
        // Immersive reading shows the document alone: no toolbar, no panels, no status bar.
        if self.full_screen {
            return column()
                .size_full()
                .min_w_0()
                .min_h_0()
                .child(self.pages_view(window, cx));
        }
        let p = self.p(cx);
        let mode = self.books[self.active].state.mode;
        let main = column()
            .size_full()
            .min_w_0()
            .min_h_0()
            .child(
                row()
                    .id("reader-toolbar")
                    .test_support()
                    .h(px(style::TOOLBAR_HEIGHT))
                    .flex_shrink_0()
                    .px_2()
                    .gap_2()
                    .border_b_1()
                    .border_color(p.line)
                    // Each panel toggle lives in the header of the panel it
                    // controls; the toolbar shows the toggle only while that
                    // panel is collapsed, so it stays pinned to the window edge.
                    .when(!self.sidebar, |toolbar| {
                        toolbar.child(self.command(
                            "collapse",
                            IconName::PanelLeft,
                            tr!("Navigation panel"),
                            &ToggleSidebar,
                            cx,
                        ))
                    })
                    .child(
                        TabBar::new("reader-modes")
                            .segmented()
                            .small()
                            .selected_index(mode.index())
                            .on_click(cx.listener(|this, index, _, cx| {
                                if !this.annotation_navigation_allowed(cx) {
                                    return;
                                }
                                this.tool_settings.dismiss();
                                this.gesture.clear();
                                this.books[this.active].state.mode = Mode::from_index(*index);
                                this.tool = 0;
                                cx.notify();
                            }))
                            .child(Tab::new().label(tr!("Read")).tooltip(|window, cx| {
                                Tooltip::new(tr!("Read mode"))
                                    .action(&ReadMode, Some("Breeze"))
                                    .build(window, cx)
                            }))
                            .child(Tab::new().label(tr!("Annotations")).tooltip(|window, cx| {
                                Tooltip::new(tr!("Annotate mode"))
                                    .action(&AnnotateMode, Some("Breeze"))
                                    .build(window, cx)
                            })),
                    )
                    .child(div().h(px(18.)).w(px(1.)).bg(p.line))
                    .child(self.tools(mode, cx))
                    .child(
                        self.command(
                            "reader-search",
                            IconName::Search,
                            tr!("Search document"),
                            &SearchDocument,
                            cx,
                        )
                        .selected(self.search_open),
                    )
                    .child(
                        self.command(
                            "reader-star",
                            IconName::Bookmark,
                            tr!("Favorites"),
                            &Favorite,
                            cx,
                        )
                        .icon(document_icons::bookmark_icon(
                            self.books[self.active].favorite,
                        ))
                        .accessibility_label(
                            if self.books[self.active].favorite {
                                tr!("Remove favorite")
                            } else {
                                tr!("Favorites")
                            },
                        ),
                    )
                    .when(!self.document_panel.open, |toolbar| {
                        toolbar.child(self.command(
                            "document-panel-toggle",
                            IconName::PanelRight,
                            tr!("Document sidebar"),
                            &ToggleDocumentPanel,
                            cx,
                        ))
                    }),
            )
            .child(
                row()
                    .flex_1()
                    .min_h_0()
                    .items_stretch()
                    .child(self.pages_view(window, cx)),
            )
            .child(self.status(window, cx));
        let center = if self.sidebar {
            h_resizable("reader-split")
                .with_state(&self.navigation_split)
                .with_handle_appearance(navigation_handle_appearance(p))
                .child(
                    resizable_panel()
                        .size(px(self.navigation_width))
                        .size_range(px(NAVIGATION_MIN_WIDTH)..px(NAVIGATION_MAX_WIDTH))
                        .child(self.navigation(cx)),
                )
                .child(
                    resizable_panel()
                        .size_range(px(430.)..gpui_kit::Pixels::MAX)
                        .child(main),
                )
                .into_any_element()
        } else {
            main.into_any_element()
        };
        div().flex_1().min_w_0().min_h_0().child(
            row()
                .size_full()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .items_stretch()
                .child(div().size_full().min_w_0().min_h_0().child(center))
                .when(self.document_panel.open, |row| {
                    row.child(self.document_sidebar(cx))
                }),
        )
    }
    pub(super) fn navigation_outline(&self) -> Option<Arc<Vec<breeze_pdf::Outline>>> {
        self.preview
            .as_ref()
            .map(|preview| preview.outline.clone())
            .or_else(|| self.reading.outline())
    }

    fn has_outline(&self) -> bool {
        self.navigation_outline()
            .is_some_and(|outline| !outline.is_empty())
    }

    pub(super) fn show_thumbnails(&self) -> bool {
        self.books[self.active].is_pdf() && (self.thumbs || !self.has_outline())
    }

    fn navigation(&self, cx: &mut Context<Self>) -> Div {
        let has_outline = self.has_outline();
        let outline_selected = has_outline && !self.thumbs;
        let nested = self.navigation_outline().is_some_and(|outline| {
            (0..outline.len()).any(|index| reading::outline_has_children(&outline, index))
        });
        let p = self.p(cx);
        column()
            .size_full()
            .min_w_0()
            .min_h_0()
            .flex_shrink_0()
            .bg(p.sidebar)
            .child(
                row()
                    .id("navigation-header")
                    .test_support()
                    .h(px(style::TOOLBAR_HEIGHT))
                    .flex_shrink_0()
                    .px(px(8.))
                    .border_b_1()
                    .border_color(p.line)
                    // Two equal flex slots flank the tabs so the tab group stays
                    // centered however wide the side controls become.
                    .child(
                        row()
                            .id("navigation-header-leading")
                            .test_support()
                            .flex_1()
                            .min_w_0()
                            .justify_start()
                            .child(
                                self.command(
                                    "collapse",
                                    IconName::PanelLeft,
                                    tr!("Navigation panel"),
                                    &ToggleSidebar,
                                    cx,
                                )
                                .selected(true),
                            ),
                    )
                    .child(
                        TabBar::new("nav-tabs")
                            .segmented()
                            .small()
                            .flex_shrink_0()
                            .selected_index(usize::from(has_outline && self.thumbs))
                            .on_click(cx.listener(move |this, index, window, cx| {
                                this.thumbs = !has_outline || *index == 1;
                                window.focus(&this.focus, cx);
                                cx.notify()
                            }))
                            .when(has_outline, |tabs| {
                                tabs.child(
                                    Tab::new()
                                        .icon(IconName::TableOfContents)
                                        .aria_label(tr!("Outline"))
                                        .tooltip(|window, cx| {
                                            Tooltip::new(tr!("Outline"))
                                                .action(&ShowOutline, Some("Breeze"))
                                                .build(window, cx)
                                        }),
                                )
                            })
                            .child(
                                Tab::new()
                                    .icon(IconName::Images)
                                    .aria_label(tr!("Thumbnails"))
                                    .tooltip(|window, cx| {
                                        Tooltip::new(tr!("Thumbnails"))
                                            .action(&ShowThumbnails, Some("Breeze"))
                                            .build(window, cx)
                                    }),
                            ),
                    )
                    .child(
                        row()
                            .id("navigation-header-trailing")
                            .test_support()
                            .flex_1()
                            .min_w_0()
                            .justify_end()
                            .when(outline_selected, |slot| {
                                slot.child(
                                    row()
                                        .gap(px(4.))
                                        .flex_shrink_0()
                                        .child(
                                            Button::new("outline-search")
                                                .ghost()
                                                .small()
                                                .icon(IconName::Search)
                                                .selected(self.outline_search_open())
                                                .accessibility_label(tr!("Search outline titles"))
                                                .tooltip(tr!("Search outline titles"))
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.toggle_outline_search(window, cx);
                                                })),
                                        )
                                        .when(nested, |actions| {
                                            actions.child(self.outline_menu(cx))
                                        }),
                                )
                            }),
                    ),
            )
            .when(outline_selected, |panel| {
                panel.children(self.outline_search_bar(cx))
            })
            .child(
                column()
                    .id("navigation-content")
                    .test_support()
                    .flex_1()
                    .min_h_0()
                    .when_some(self.navigation_outline(), |this, outline| {
                        if self.show_thumbnails() {
                            this.child(self.thumbnails_view(cx))
                        } else {
                            this.child(self.outline_view(outline, cx))
                        }
                    }),
            )
    }
    fn outline_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        Button::new("outline-more")
            .ghost()
            .small()
            .icon(IconName::EllipsisVertical)
            .accessibility_label(tr!("More outline actions"))
            .tooltip(tr!("More outline actions"))
            .dropdown_menu(move |menu, window, _| {
                menu.item(
                    PopupMenuItem::new(tr!("Expand all"))
                        .icon(IconName::ListChevronsUpDown)
                        .on_click(window.listener_for(&view, |this, _, _, cx| {
                            this.set_outline_expanded(true, cx);
                        })),
                )
                .item(
                    PopupMenuItem::new(tr!("Collapse all"))
                        .icon(IconName::ListChevronsDownUp)
                        .on_click(window.listener_for(&view, |this, _, _, cx| {
                            this.set_outline_expanded(false, cx);
                        })),
                )
            })
    }
    /// The one status line the reader shows, or `None` when there is nothing to
    /// report. A healthy document leaves the area empty instead of announcing
    /// that nothing is wrong.
    fn status_notice(&self) -> Option<String> {
        let book = &self.books[self.active];
        if let Some(error) = &book.annotations.error {
            Some(tf!("Annotation error: {}", error))
        } else if self.raster.page_failed(book.state.navigation.page()) {
            Some(tr!("Page rendering failed; some content could not be displayed").into())
        } else if !self.message.is_empty() {
            Some(self.message.clone())
        } else if book.annotations.dirty() {
            Some(tr!("Unsaved annotations").into())
        } else {
            None
        }
    }
    fn status(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.p(cx);
        let s = &self.books[self.active].state;
        let visible = s.layout.pages(s.navigation.page(), s.navigation.total());
        row()
            .id("status-scroll")
            .test_support()
            .overflow_x_scroll()
            .h(px(32.))
            .flex_shrink_0()
            .px_2()
            .gap_1()
            .border_t_1()
            .border_color(p.line)
            .bg(p.bg)
            .child(
                self.command(
                    "previous",
                    IconName::ChevronLeft,
                    tr!("Previous page"),
                    &PreviousPage,
                    cx,
                )
                .disabled(visible.start == 0),
            )
            .child(div().text_size(px(12.)).child(format!(
                "{} / {}",
                s.navigation.page() + 1,
                s.navigation.total()
            )))
            .child(
                self.command(
                    "next",
                    IconName::ChevronRight,
                    tr!("Next page"),
                    &NextPage,
                    cx,
                )
                .disabled(visible.end == s.navigation.total()),
            )
            .child(div().w(px(42.)).child(Input::new(&self.page_input).small()))
            .child(
                Button::new("jump")
                    .ghost()
                    .small()
                    .icon(IconName::CornerDownLeft)
                    .accessibility_label(tr!("Go to entered page"))
                    .tooltip_with_action(
                        tr!("Go to entered page; shortcut focuses the page field"),
                        &FocusPage,
                        Some("Breeze"),
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.submit_page(cx))),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(50.))
                    .overflow_hidden()
                    .text_size(px(11.))
                    .text_color(p.muted)
                    .when_some(self.status_notice(), |area, notice| {
                        area.child(div().id("reader-status").test_support().child(notice))
                    }),
            )
            .child(self.rotate_button(false, cx))
            .child(self.rotate_button(true, cx))
            .child(self.layout_button(cx))
            .child(self.zoom_button(cx))
            .child(self.command("zoom-out", IconName::Minus, tr!("Zoom out"), &ZoomOut, cx))
            .child(
                div()
                    .id("zoom-percent")
                    .test_support()
                    .min_w(px(48.))
                    .text_center()
                    .text_size(px(12.))
                    .child(format!("{:.0}%", self.display_zoom(window))),
            )
            .child(self.command("zoom-in", IconName::Plus, tr!("Zoom in"), &ZoomIn, cx))
            .child(self.fullscreen_button(cx))
            .into_any_element()
    }
    pub(super) fn submit_page(&mut self, cx: &mut Context<Self>) {
        if !self.books[self.active].is_pdf() {
            return;
        }
        if !self.annotation_navigation_allowed(cx) {
            return;
        }
        let value = self.page_input.read(cx).value();
        match self.books[self.active].state.navigation.jump(&value) {
            Ok(()) => {
                self.message.clear();
                self.load(cx);
            }
            Err(error) => {
                self.message = error.into();
                cx.notify();
            }
        }
    }
}
