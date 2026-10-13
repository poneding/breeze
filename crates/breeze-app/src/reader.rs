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

fn navigation_handle_appearance(p: Palette) -> ResizeHandleRenderer {
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
                    .h(px(38.))
                    .flex_shrink_0()
                    .px_2()
                    .gap_2()
                    .border_b_1()
                    .border_color(p.line)
                    .child(
                        self.command(
                            "collapse",
                            IconName::PanelLeft,
                            tr!("导航面板"),
                            &ToggleSidebar,
                            cx,
                        )
                        .selected(self.sidebar),
                    )
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
                            .child(Tab::new().label(tr!("阅读")).tooltip(|window, cx| {
                                Tooltip::new(tr!("阅读模式"))
                                    .action(&ReadMode, Some("Breeze"))
                                    .build(window, cx)
                            }))
                            .child(Tab::new().label(tr!("标注")).tooltip(|window, cx| {
                                Tooltip::new(tr!("标注模式"))
                                    .action(&AnnotateMode, Some("Breeze"))
                                    .build(window, cx)
                            }))
                    )
                    .child(div().h(px(18.)).w(px(1.)).bg(p.line))
                    .child(self.tools(mode, cx))
                    .child(
                        self.command(
                            "reader-search",
                            IconName::Search,
                            tr!("搜索文档"),
                            &SearchDocument,
                            cx,
                        )
                        .selected(self.search_open),
                    )
                    .child(
                        self.command(
                            "reader-star",
                            IconName::Bookmark,
                            tr!("收藏"),
                            &Favorite,
                            cx,
                        )
                        .icon(document_icons::bookmark_icon(
                            self.books[self.active].favorite,
                        ))
                        .accessibility_label(
                            if self.books[self.active].favorite {
                                tr!("取消收藏")
                            } else {
                                tr!("收藏")
                            },
                        ),
                    )
                    .child(
                        self.command(
                            "document-panel-toggle",
                            IconName::PanelRight,
                            tr!("文档侧栏"),
                            &ToggleDocumentPanel,
                            cx,
                        )
                        .selected(self.document_panel.open),
                    ),
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
                        .size_range(px(NAVIGATION_MIN_WIDTH)..px(360.))
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
        self.thumbs || !self.has_outline()
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
                    .h(px(38.))
                    .justify_start()
                    .flex_shrink_0()
                    .px(px(8.))
                    .border_b_1()
                    .border_color(p.line)
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
                                tabs.child(Tab::new().label(tr!("目录")).tooltip(|window, cx| {
                                    Tooltip::new(tr!("目录"))
                                        .action(&ShowOutline, Some("Breeze"))
                                        .build(window, cx)
                                }))
                            })
                            .child(Tab::new().label(tr!("缩略图")).tooltip(|window, cx| {
                                Tooltip::new(tr!("缩略图"))
                                    .action(&ShowThumbnails, Some("Breeze"))
                                    .build(window, cx)
                            })),
                    )
                    .child(div().flex_1().min_w(px(4.)))
                    .when(outline_selected, |header| {
                        header.child(
                            row()
                                .gap(px(4.))
                                .flex_shrink_0()
                                .child(
                                    Button::new("outline-search")
                                        .ghost()
                                        .small()
                                        .icon(IconName::Search)
                                        .selected(self.outline_search_open())
                                        .accessibility_label(tr!("搜索目录标题"))
                                        .tooltip(tr!("搜索目录标题"))
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.toggle_outline_search(window, cx);
                                        })),
                                )
                                .when(nested, |actions| actions.child(self.outline_menu(cx))),
                        )
                    }),
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
            .accessibility_label(tr!("更多目录操作"))
            .tooltip(tr!("更多目录操作"))
            .dropdown_menu(move |menu, window, _| {
                menu.item(
                    PopupMenuItem::new(tr!("全部展开"))
                        .icon(IconName::ListChevronsUpDown)
                        .on_click(window.listener_for(&view, |this, _, _, cx| {
                            this.set_outline_expanded(true, cx);
                        })),
                )
                .item(
                    PopupMenuItem::new(tr!("全部收起"))
                        .icon(IconName::ListChevronsDownUp)
                        .on_click(window.listener_for(&view, |this, _, _, cx| {
                            this.set_outline_expanded(false, cx);
                        })),
                )
            })
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
                    tr!("上一页"),
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
                self.command("next", IconName::ChevronRight, tr!("下一页"), &NextPage, cx)
                    .disabled(visible.end == s.navigation.total()),
            )
            .child(div().w(px(42.)).child(Input::new(&self.page_input).small()))
            .child(
                Button::new("jump")
                    .ghost()
                    .small()
                    .icon(IconName::CornerDownLeft)
                    .accessibility_label(tr!("跳转到输入页码"))
                    .tooltip_with_action(
                        tr!("跳转到输入页码；快捷键聚焦页码框"),
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
                    .child(
                        if let Some(error) = &self.books[self.active].annotations.error {
                            tf!("标注异常：{}", error)
                        } else if !self.message.is_empty() {
                            self.message.clone()
                        } else if self.books[self.active].annotations.dirty() {
                            tr!("批注未保存").into()
                        } else {
                            tr!("原件未修改").into()
                        },
                    ),
            )
            .child(self.rotate_button(false, cx))
            .child(self.rotate_button(true, cx))
            .child(self.layout_button(cx))
            .child(self.zoom_button(cx))
            .child(self.command("zoom-out", IconName::Minus, tr!("缩小"), &ZoomOut, cx))
            .child(
                div()
                    .id("zoom-percent")
                    .test_support()
                    .min_w(px(48.))
                    .text_center()
                    .text_size(px(12.))
                    .child(format!("{:.0}%", self.display_zoom(window))),
            )
            .child(self.command("zoom-in", IconName::Plus, tr!("放大"), &ZoomIn, cx))
            .child(self.fullscreen_button(cx))
            .into_any_element()
    }
    pub(super) fn submit_page(&mut self, cx: &mut Context<Self>) {
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
