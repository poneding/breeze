//! Bindings are the single source of truth for command buttons and Kit's Kbd tooltips.
use crate::*;
use gpui_kit::base::InteractiveElementExt as _;
use gpui_kit::component::{
    ElementExt, TitleBar, button::ButtonCustomVariant, menu::ContextMenuExt,
};

pub fn bind(cx: &mut App) {
    tool_settings::bind(cx);
    cx.bind_keys([
        KeyBinding::new("secondary-o", Open, Some("Breeze")),
        KeyBinding::new(
            "backspace",
            DeleteSelectedAnnotation,
            Some("Breeze && !Input"),
        ),
        KeyBinding::new("delete", DeleteSelectedAnnotation, Some("Breeze && !Input")),
        KeyBinding::new("escape", CancelAnnotationEdit, Some("Breeze")),
        // Immersive reading is also left with Escape, the same key that cancels edits.
        KeyBinding::new("f11", ToggleFullScreen, Some("Breeze")),
        KeyBinding::new("escape", CancelAnnotationEdit, Some("AnnotationText")),
        KeyBinding::new("secondary-shift-l", CycleLayout, Some("Breeze && !Input")),
        KeyBinding::new("secondary-,", Preferences, Some("Breeze")),
        KeyBinding::new("secondary-i", ShowAbout, Some("Breeze")),
        KeyBinding::new("secondary-k", ShowShortcuts, Some("Breeze")),
        KeyBinding::new("secondary-shift-h", Home, Some("Breeze")),
        KeyBinding::new("secondary-w", CloseTab, Some("Breeze")),
        KeyBinding::new("ctrl-tab", NextTab, Some("Breeze")),
        KeyBinding::new("ctrl-shift-tab", PreviousTab, Some("Breeze")),
        KeyBinding::new("secondary-b", ToggleSidebar, Some("Breeze")),
        KeyBinding::new("secondary-g", FocusPage, Some("Breeze")),
        KeyBinding::new("secondary-d", Favorite, Some("Breeze")),
        KeyBinding::new("secondary-shift-t", CycleTheme, Some("Breeze")),
        KeyBinding::new("secondary-1", ReadMode, Some("Breeze")),
        KeyBinding::new("secondary-2", AnnotateMode, Some("Breeze")),
        KeyBinding::new("secondary-shift-o", ShowOutline, Some("Breeze")),
        KeyBinding::new("secondary-shift-m", ShowThumbnails, Some("Breeze")),
        // Page/zoom shortcuts must not consume editing keys inside an Input.
        KeyBinding::new("pagedown", NextPage, Some("Breeze && !Input")),
        KeyBinding::new("pageup", PreviousPage, Some("Breeze && !Input")),
        KeyBinding::new("right", NextPage, Some("Breeze && !Input")),
        KeyBinding::new("left", PreviousPage, Some("Breeze && !Input")),
        KeyBinding::new("up", ScrollReaderUp, Some("Breeze && !Input")),
        KeyBinding::new("down", ScrollReaderDown, Some("Breeze && !Input")),
        KeyBinding::new("secondary-=", ZoomIn, Some("Breeze && !Input")),
        KeyBinding::new("secondary-+", ZoomIn, Some("Breeze && !Input")),
        KeyBinding::new("secondary-shift-=", ZoomIn, Some("Breeze && !Input")),
        KeyBinding::new("secondary--", ZoomOut, Some("Breeze && !Input")),
        KeyBinding::new("secondary-0", ActualSize, Some("Breeze && !Input")),
        KeyBinding::new("secondary-9", FitPage, Some("Breeze && !Input")),
        KeyBinding::new("secondary-8", FitWidth, Some("Breeze && !Input")),
        KeyBinding::new("secondary-s", SaveAnnotations, Some("Breeze")),
        KeyBinding::new("secondary-shift-e", ExportPdf, Some("Breeze")),
        KeyBinding::new("secondary-z", UndoAnnotation, Some("Breeze && !Input")),
        KeyBinding::new(
            "secondary-shift-z",
            RedoAnnotation,
            Some("Breeze && !Input"),
        ),
        KeyBinding::new("secondary-f", SearchDocument, Some("Breeze")),
        KeyBinding::new("secondary-c", CopySelection, Some("Breeze && !Input")),
        KeyBinding::new("secondary-q", Quit, None),
    ]);
}
impl Breeze {
    pub(super) fn command(
        &self,
        id: impl Into<ElementId>,
        icon: IconName,
        label: impl Into<SharedString>,
        action: &dyn Action,
        cx: &Context<Self>,
    ) -> Button {
        let action = action.boxed_clone();
        let click_action = action.boxed_clone();
        Button::new(id)
            .ghost()
            .small()
            .icon(icon)
            .accessibility_label(label.into())
            .tooltip_with_action(
                action_label(action.as_ref()),
                action.as_ref(),
                Some("Breeze"),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.execute(click_action.as_ref(), window, cx)
            }))
    }
    pub(super) fn execute(
        &mut self,
        action: &dyn Action,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.forward_global(action, window, cx) {
            return;
        }
        self.focus_pane(window, cx);
        if self.restoring || self.closing {
            return;
        }
        if action.as_any().is::<Open>() {
            if !self.annotation_navigation_allowed(cx) {
                return;
            }
            self.choose_file(cx);
        } else if action.as_any().is::<Home>() {
            self.show_home(cx);
        } else if action.as_any().is::<Preferences>() {
            // Settings is a Home page; the sidebar keeps global navigation visible.
            self.show_home_page(HomePage::Settings, cx);
        } else if action.as_any().is::<ShowAbout>() {
            self.show_home_page(HomePage::About, cx);
        } else if action.as_any().is::<ShowShortcuts>() {
            self.show_home_page(HomePage::Shortcuts, cx);
        } else if action.as_any().is::<CycleTheme>() {
            self.set_appearance(self.theme_preference.next(cx), window, cx);
        } else if action.as_any().is::<ToggleFullScreen>() {
            self.set_full_screen(!self.full_screen, window, cx);
        } else if action.as_any().is::<NextTab>() || action.as_any().is::<PreviousTab>() {
            let tabs = self.pane_tabs();
            if !tabs.is_empty() {
                let current = tabs.iter().position(|i| *i == self.active).unwrap_or(0);
                let step = if action.as_any().is::<NextTab>() {
                    1
                } else {
                    tabs.len() - 1
                };
                self.open_book(tabs[(current + step) % tabs.len()], cx);
            }
        } else if self.screen == Screen::Reader {
            if !self.books[self.active].is_pdf() {
                if action.as_any().is::<ExportPdf>()
                    || action.as_any().is::<SaveAnnotations>()
                    || action.as_any().is::<UndoAnnotation>()
                    || action.as_any().is::<RedoAnnotation>()
                    || action.as_any().is::<CycleLayout>()
                    || action.as_any().is::<RotateLeft>()
                    || action.as_any().is::<RotateRight>()
                    || action.as_any().is::<ShowThumbnails>()
                    || action.as_any().is::<FocusPage>()
                    || action.as_any().is::<AnnotateMode>()
                    || action.as_any().is::<FitPage>()
                    || action.as_any().is::<FitWidth>()
                {
                    return;
                }
                if action.as_any().is::<ZoomIn>()
                    || action.as_any().is::<ZoomOut>()
                    || action.as_any().is::<ActualSize>()
                {
                    let delta = if action.as_any().is::<ZoomIn>() {
                        1
                    } else if action.as_any().is::<ZoomOut>() {
                        -1
                    } else {
                        18 - self.books[self.active].reflow.font_size as i16
                    };
                    self.resize_reflow_text(delta, cx);
                    return;
                }
            }
            if action.as_any().is::<ExportPdf>() {
                self.choose_pdf_export(cx);
            } else if action.as_any().is::<SaveAnnotations>() {
                self.save_annotations(cx);
            } else if action.as_any().is::<UndoAnnotation>() {
                if self.annotation_editing.pending_text() {
                    return;
                }
                self.annotation_editing.clear();
                self.books[self.active].annotations.history.undo();
                cx.notify();
            } else if action.as_any().is::<RedoAnnotation>() {
                if self.annotation_editing.pending_text() {
                    return;
                }
                self.annotation_editing.clear();
                self.books[self.active].annotations.history.redo();
                cx.notify();
            } else if action.as_any().is::<CycleLayout>() {
                let next = match self.books[self.active].state.layout {
                    ReadingLayout::Single => ReadingLayout::Continuous,
                    ReadingLayout::Continuous => ReadingLayout::ContinuousSpread,
                    ReadingLayout::ContinuousSpread => ReadingLayout::Spread,
                    ReadingLayout::Spread => ReadingLayout::Single,
                };
                self.set_layout(next, cx);
            } else if action.as_any().is::<RotateLeft>() || action.as_any().is::<RotateRight>() {
                self.rotate(action.as_any().is::<RotateRight>(), cx);
            } else if action.as_any().is::<CopySelection>() {
                if !self.selected_text.is_empty() {
                    cx.write_to_clipboard(ClipboardItem::new_string(self.selected_text.clone()));
                }
            } else if action.as_any().is::<SearchDocument>() {
                if !self.annotation_navigation_allowed(cx) {
                    return;
                }
                if self.search_open {
                    self.close_search(cx);
                } else {
                    self.search_restore_document_panel_open = Some(self.document_panel.open);
                    self.search_open = true;
                    self.document_panel.open = true;
                }
                self.search_input
                    .update(cx, |input, cx| input.focus(window, cx));
                cx.notify();
            } else if action.as_any().is::<CloseTab>() {
                self.close_book(self.active, cx);
            } else if action.as_any().is::<NextPage>() {
                self.next_page(1, cx);
            } else if action.as_any().is::<PreviousPage>() {
                self.next_page(-1, cx);
            } else if action.as_any().is::<ScrollReaderUp>()
                || action.as_any().is::<ScrollReaderDown>()
            {
                let distance = if action.as_any().is::<ScrollReaderDown>() {
                    px(48.)
                } else {
                    px(-48.)
                };
                self.scroll_reading(distance, cx);
            } else if action.as_any().is::<ToggleSidebar>() {
                self.sidebar = !self.sidebar;
                cx.notify();
            } else if action.as_any().is::<ToggleDocumentPanel>() {
                if self.document_panel.open {
                    if self.search_open {
                        self.close_search(cx);
                    }
                    self.document_panel.open = false;
                } else {
                    self.document_panel.open = true;
                }
                cx.notify();
            } else if action.as_any().is::<ShowOutline>() || action.as_any().is::<ShowThumbnails>()
            {
                self.sidebar = true;
                self.thumbs = action.as_any().is::<ShowThumbnails>();
                cx.notify();
            } else if action.as_any().is::<Favorite>() {
                self.toggle_favorite(self.active, cx);
                cx.notify();
            } else if action.as_any().is::<FocusPage>() {
                self.begin_page_edit(window, cx);
            } else if action.as_any().is::<ReadMode>() || action.as_any().is::<AnnotateMode>() {
                if !self.annotation_navigation_allowed(cx) {
                    return;
                }
                self.tool_settings.dismiss();
                self.gesture.clear();
                self.books[self.active].state.mode = if action.as_any().is::<ReadMode>() {
                    Mode::Read
                } else {
                    Mode::Annotate
                };
                self.tool = 0;
                cx.notify();
            } else {
                let zoom = if action.as_any().is::<ZoomIn>() {
                    self.stepped_zoom(25, window)
                } else if action.as_any().is::<ZoomOut>() {
                    self.stepped_zoom(-25, window)
                } else if action.as_any().is::<FitPage>() {
                    Zoom::FitPage
                } else if action.as_any().is::<FitWidth>() {
                    Zoom::FitWidth
                } else if action.as_any().is::<ActualSize>() {
                    Zoom::ActualSize
                } else {
                    return;
                };
                self.set_zoom(zoom, cx);
            }
        }
    }
    pub(super) fn close_book(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some((peer, index)) = self.secondary_book(&self.books[index].source, cx) {
            peer.update(cx, |peer, cx| peer.close_book(index, cx));
            return;
        }
        if self.active == index && !self.annotation_navigation_allowed(cx) {
            return;
        }
        if self.books[index].annotations.dirty() || self.books[index].annotations.saving {
            self.message = tr!("Save this document's annotations before closing its tab").into();
            cx.notify();
            return;
        }
        if self.books[index]
            .markdown_edit
            .as_ref()
            .is_some_and(|editor| editor.read(cx).dirty(cx))
        {
            if self.pending_markdown_discard != Some(index) {
                self.pending_markdown_discard = Some(index);
                self.message = tr!(
                    "This Markdown draft has unsaved changes. Closing again discards them; saving arrives in a later phase."
                )
                .into();
                cx.notify();
                return;
            }
            self.pending_markdown_discard = None;
        }
        self.books[index].opened = false;
        if self.active == index {
            if let Some(next) = self.pane_tabs().first().copied() {
                self.open_book(next, cx)
            } else {
                self.show_home(cx)
            }
        } else {
            cx.notify()
        }
        self.normalize_split(cx);
    }
    fn document_tab_width(&self, index: usize, window: &Window, cx: &App) -> gpui_kit::Pixels {
        let theme = Theme::global(cx);
        // Use the same Small text size and font as Kit, including Unicode shaping.
        let title: SharedString = self.books[index].title.replace(['\r', '\n'], " ").into();
        let width = window
            .text_system()
            .shape_line(
                title.clone(),
                window.rem_size() * 0.875,
                &[TextRun {
                    len: title.len(),
                    font: font(theme.font_family.clone()),
                    color: theme.foreground,
                    ..Default::default()
                }],
                None,
            )
            .width();
        // Small icon (0.875 rem), icon gap (0.25 rem), and equal 0.5 rem side padding.
        (width + window.rem_size() * 2.125).clamp(px(40.), px(228.))
    }

    fn document_tab(
        &self,
        index: usize,
        width: gpui_kit::Pixels,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let book = &self.books[index];
        let active = self.screen == Screen::Reader && index == self.active;
        let p = self.p(cx);
        let theme = Theme::global(cx);
        let tab_hover = style::accent_hover(cx);
        let close_surface = p.panel.blend(tab_hover);
        let close_overlay = row()
            .id(("document-tab-close-overlay", index))
            .test_support()
            .absolute()
            .right_0()
            .top_0()
            .h_full()
            .w(px(34.5))
            .invisible()
            .group_hover("document-tab", |s| s.visible())
            .child(
                div()
                    .w(px(12.))
                    .flex_shrink_0()
                    .h_full()
                    .bg(linear_gradient(
                        90.,
                        linear_color_stop(close_surface.opacity(0.), 0.),
                        linear_color_stop(close_surface, 1.),
                    )),
            )
            .child(
                row()
                    .h_full()
                    .flex_1()
                    .justify_end()
                    .bg(close_surface)
                    .child(
                        Button::new(("close", index))
                            .custom(ButtonCustomVariant::new(cx).hover(p.line).active(p.line))
                            .small()
                            .size(px(18.5))
                            .icon(IconName::X)
                            .accessibility_label(tr!("Close tab"))
                            .tooltip_with_action(tr!("Close tab"), &CloseTab, Some("Breeze"))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.close_book(index, cx);
                            })),
                    ),
            );
        let source = book.source.clone();
        let destination = if self.workspace.parent.is_some() {
            split::Destination::Secondary
        } else {
            split::Destination::Primary
        };
        let drag = split::TabDrag {
            owner: cx.weak_entity(),
            source: source.clone(),
            title: book.title.clone(),
        };
        let can_reveal = matches!(source, Source::File(_));
        let menu_view = cx.entity();
        row()
            .id(("draggable-tab", index))
            .h_full()
            .flex_initial()
            .min_w(px(40.))
            .w(width)
            .max_w(px(228.))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_drag(drag, |drag, _, _, cx| {
                let _ = drag.owner.update(cx, |_, cx| cx.notify());
                cx.new(|_| drag.clone())
            })
            .on_drop(cx.listener(move |this, drag: &split::TabDrag, window, cx| {
                cx.stop_propagation();
                this.queue_tab_drop(drag, destination, Some(source.clone()), window, cx);
            }))
            .child(
                Button::new(("document", index))
                    .custom(
                        ButtonCustomVariant::new(cx)
                            .hover(tab_hover)
                            .active(tab_hover),
                    )
                    .small()
                    .group("document-tab")
                    .flex_initial()
                    .w_full()
                    .min_w(px(40.))
                    .max_w(px(228.))
                    .pl_2()
                    .pr(px(2.))
                    .accessibility_label(book.title.clone())
                    .tooltip(book.title.clone())
                    // Custom variants tint their base color; keep this token exact.
                    .when(active, |d| d.bg(theme.tokens.background).shadow_sm())
                    // Keep one hover surface while the pointer crosses the Close overlay.
                    .group_hover("document-tab", move |s| s.bg(tab_hover).shadow_sm())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        window.focus(&this.focus, cx);
                        this.open_book(index, cx);
                    }))
                    .context_menu(move |menu, window, menu_cx| {
                        context_menus::tab(
                            menu,
                            menu_view.clone(),
                            index,
                            can_reveal,
                            window,
                            menu_cx,
                        )
                    })
                    .child(
                        row()
                            .id(("document-tab-content", index))
                            .test_support()
                            .relative()
                            .size_full()
                            .min_w_0()
                            .gap_1()
                            // Inset the title while keeping the absolute Close overlay near the edge.
                            .pr(theme.font_size * 0.5 - px(2.))
                            .child(document_icons::file_icon(&book.source, cx).flex_shrink_0())
                            .child(
                                div()
                                    .id(("document-tab-title", index))
                                    .test_support()
                                    .flex_1()
                                    .min_w_0()
                                    .text_ellipsis()
                                    .child(book.title.clone()),
                            )
                            // Absolute positioning keeps the full title width until hover.
                            .child(close_overlay),
                    ),
            )
    }

    pub(super) fn header(&self, window: &Window, cx: &mut Context<Self>) -> TitleBar {
        let p = self.p(cx);
        TitleBar::new()
            .on_close_window(cx.listener(|this, _, window, cx| {
                if this.request_close(window, cx) {
                    window.remove_window();
                }
            }))
            .bg(p.panel)
            .border_color(p.line)
            .child(
                row()
                    .id("titlebar-content")
                    // Must stay ahead of `test_support`, like Kit's own
                    // double-click handler, because it is a stateful element
                    // method. Kit's title bar asks AppKit to zoom the window on
                    // a double click; a fullscreen window already fills its
                    // Space, so that zoom shrinks the window inside the Space
                    // and exposes the desktop above and below it. macOS shows
                    // no title bar in fullscreen to double click, so the
                    // gesture does nothing there.
                    .when(window.is_fullscreen(), |this| {
                        this.on_double_click(|_, _, cx| cx.stop_propagation())
                    })
                    // Windows turns a hit test over a drag area into the native
                    // window caption, and the caption consumes the press before
                    // GPUI sees it. Kit marks the whole title-bar content area as
                    // draggable, so without this block the caption would sit on top
                    // of Home and Plus and neither would ever receive a click.
                    // Blocking the mouse stops the hit test at this row, leaving the
                    // tab group's empty width as the title bar's drag surface.
                    .occlude()
                    .on_prepaint({
                        let view = cx.entity();
                        move |bounds, window, cx| {
                            // Where the window's trailing chrome starts: Kit's
                            // window controls on Windows and Linux, none on
                            // macOS. Measuring it keeps the pane groups aligned
                            // with the panes without duplicating Kit's widths.
                            let chrome = window.viewport_size().width - bounds.right();
                            view.update(cx, |this, cx| {
                                if (this.title_bar_right_chrome - chrome).abs() > px(0.5) {
                                    this.title_bar_right_chrome = chrome;
                                    cx.defer_in(window, |_, _, cx| cx.notify());
                                }
                            });
                        }
                    })
                    .test_support()
                    // A zero basis prevents Kit's nonshrinking titlebar wrapper
                    // from requesting the tabs' combined intrinsic width.
                    .h_full()
                    .w_0()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .child(
                        self.command("home-tab", IconName::House, tr!("Home"), &Home, cx)
                            .selected(
                                self.screen == Screen::Home && self.home_page == HomePage::Library,
                            ),
                    )
                    .child(self.workspace_tabs(window, cx)),
            )
    }
    pub(super) fn tab_group(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let tabs: Vec<_> = self
            .pane_tabs()
            .into_iter()
            .map(|index| (index, self.document_tab_width(index, window, cx)))
            .collect();
        // Cap the strip at its actual content width so Plus follows short titles.
        let tab_strip_width = px(10.)
            + tabs
                .iter()
                .map(|(_, width)| *width)
                .sum::<gpui_kit::Pixels>()
            + window.rem_size() * 0.25 * tabs.len().saturating_sub(1) as f32;
        let destination = if self.workspace.parent.is_some() {
            split::Destination::Secondary
        } else {
            split::Destination::Primary
        };
        let group = row()
            .id("document-tab-group")
            .h_full()
            .w_0()
            .flex_1()
            .min_w_0()
            .gap_1()
            // Windows turns a hit test over this area into the native window
            // caption, and the caption consumes the press before GPUI sees it.
            // The controls below block this hitbox, so only the empty width after
            // them keeps the caption; Home and Plus stay ordinary client clicks.
            // macOS and Linux ignore the window-control area and start the move
            // from these listeners instead.
            .window_control_area(WindowControlArea::Drag)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, _| {
                    this.title_bar_drag = true;
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| {
                    this.title_bar_drag = false;
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, _| {
                    this.title_bar_drag = false;
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, _| {
                // A press that ends outside the group never reaches `on_mouse_up`,
                // so require the button to still be down before moving the window.
                if this.title_bar_drag && event.dragging() {
                    this.title_bar_drag = false;
                    window.start_window_move();
                }
            }))
            .on_drop(cx.listener(move |this, drag: &split::TabDrag, window, cx| {
                this.queue_tab_drop(drag, destination, None, window, cx);
            }))
            .child(
                row()
                    .id("document-tabs")
                    .test_support()
                    .h_full()
                    .w_0()
                    .flex_1()
                    .max_w(tab_strip_width)
                    .min_w_0()
                    // Block the group's caption behind the strip, including the
                    // gaps between tabs, so a tab press reaches the tab and not the
                    // window move. Tabs themselves paint in front and stay clickable.
                    .occlude()
                    .overflow_x_scroll()
                    .pl(px(4.))
                    .pr(px(6.))
                    .gap_1()
                    // The strip blocks the group's drop target, so dropping in its
                    // padding still moves the tab to the end of this pane. A drop on
                    // a tab reaches that tab's own target first and inserts there.
                    .on_drop(cx.listener(move |this, drag: &split::TabDrag, window, cx| {
                        this.queue_tab_drop(drag, destination, None, window, cx);
                    }))
                    .children(
                        tabs.into_iter()
                            .map(|(index, width)| self.document_tab(index, width, cx)),
                    ),
            )
            .child(
                row()
                    .id("document-tab-add")
                    .test_support()
                    .flex_shrink_0()
                    // The same block keeps the Plus click out of the caption.
                    .occlude()
                    .gap_1()
                    .child(
                        div()
                            .id("document-tab-divider")
                            .test_support()
                            .w(px(1.))
                            .h(px(14.))
                            .bg(self.p(cx).line),
                    )
                    .child(
                        self.command(
                            "add-document",
                            IconName::Plus,
                            tr!("Open document"),
                            &Open,
                            cx,
                        )
                        .text_color(self.p(cx).muted),
                    ),
            )
            // Keep a drag handle even when tabs fill the strip: without it the
            // caption would vanish and the window could not be moved from its
            // title bar. The spacer is covered by the group's drag area.
            .child(div().w(px(24.)).flex_shrink_0());
        // Windows handles a caption double click itself; the other platforms reach
        // the same behavior through Kit's title-bar listeners, which the controls
        // above keep away from the tab area. These are stateful element methods,
        // so they precede `test_support`.
        #[cfg(target_os = "macos")]
        let group = group.on_double_click(|_, window, _| window.titlebar_double_click());
        #[cfg(target_os = "linux")]
        let group = group
            .on_double_click(|_, window, _| window.zoom_window())
            .on_mouse_down(MouseButton::Right, |event, window, _| {
                window.show_window_menu(event.position);
            });
        group.test_support().into_any_element()
    }

    fn workspace_tabs(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.p(cx);
        let primary = self.tab_group(window, cx);
        let Some(peer) = self.workspace.secondary.clone() else {
            return row()
                .id("primary-tabs")
                .test_support()
                .h_full()
                .flex_1()
                .min_w_0()
                .pr_2()
                .child(primary)
                .into_any_element();
        };
        let secondary = peer.update(cx, |peer, cx| peer.tab_group(window, cx));
        // The panes divide the whole window, so the right pane's tab group has to
        // start on that same seam; half of whatever the title bar has left after
        // Home and the traffic lights does not land there. `sizes()[1]` is the
        // right-hand pane in either arrangement, because the secondary pane is
        // added second unless it sits on the left.
        let right_width = self.workspace.split.as_ref().and_then(|state| {
            let sizes = state.read(cx).sizes();
            let right_pane = *sizes.get(1)?;
            // This row ends where the window's trailing chrome begins, so that
            // distance is added back onto the pane to reach the window's edge.
            Some(right_pane - self.title_bar_right_chrome)
        });
        let primary_on_right = self.workspace.side == split::Side::Left;
        let primary =
            Self::pane_tabs_row("primary-tabs", primary_on_right, right_width, p, primary);
        let secondary = Self::pane_tabs_row(
            "secondary-tabs",
            !primary_on_right,
            right_width,
            p,
            secondary,
        );
        let group = row().h_full().w_0().flex_1().min_w_0();
        if primary_on_right {
            group.child(secondary).child(primary).into_any_element()
        } else {
            group.child(primary).child(secondary).into_any_element()
        }
    }

    /// One pane's tab row in the title bar.
    ///
    /// The right-hand row is sized to its pane and carries the divider as its
    /// leading border, which puts the hairline on the same pixel as the panes'
    /// own divider and the first tab at the pane's left edge. It also carries the
    /// header's trailing space, so this row's own bounds end at the window's
    /// trailing chrome.
    fn pane_tabs_row(
        id: &'static str,
        on_right: bool,
        right_width: Option<gpui_kit::Pixels>,
        p: Palette,
        content: AnyElement,
    ) -> AnyElement {
        let row = row().id(id).test_support().h_full().min_w_0();
        if !on_right {
            return row.flex_1().child(content).into_any_element();
        }
        let row = row.border_l_1().border_color(p.line).pr_2();
        match right_width {
            Some(width) => row
                .flex_none()
                .w(width.max(px(0.)))
                .child(content)
                .into_any_element(),
            // Until both panes have reported their sizes, the groups share the
            // leftover width.
            None => row.flex_1().child(content).into_any_element(),
        }
    }
}
fn action_label(action: &dyn Action) -> &'static str {
    macro_rules! name {
        ($ty:ty,$label:expr) => {
            if action.as_any().is::<$ty>() {
                return $label;
            }
        };
    }
    name!(SaveAnnotations, tr!("Save annotation sidecar"));
    name!(
        ExportPdf,
        tr!("Export annotations to a new PDF; preserve source")
    );
    name!(UndoAnnotation, tr!("Undo annotation"));
    name!(RedoAnnotation, tr!("Redo annotation"));
    name!(SearchDocument, tr!("Search document"));
    name!(CycleLayout, tr!("Change reading layout"));
    name!(RotateLeft, tr!("Rotate 90° left"));
    name!(RotateRight, tr!("Rotate 90° right"));
    name!(
        ToggleFullScreen,
        tr!("Fullscreen reading (Escape exits, F11 toggles)")
    );
    name!(CopySelection, tr!("Copy selected text"));
    name!(Open, tr!("Open document"));
    name!(Home, tr!("Home"));
    name!(Preferences, tr!("Settings"));
    name!(ShowAbout, tr!("About"));
    name!(ShowShortcuts, tr!("Shortcuts"));
    name!(CloseTab, tr!("Close current tab"));
    name!(ToggleSidebar, tr!("Toggle navigation"));
    name!(ToggleDocumentPanel, tr!("Toggle document sidebar"));
    name!(NextPage, tr!("Next page"));
    name!(PreviousPage, tr!("Previous page"));
    name!(ScrollReaderUp, tr!("Scroll up (Markdown / continuous PDF)"));
    name!(
        ScrollReaderDown,
        tr!("Scroll down (Markdown / continuous PDF)")
    );
    name!(FocusPage, tr!("Go to page"));
    name!(Favorite, tr!("Toggle favorite"));
    name!(ZoomIn, tr!("Zoom in"));
    name!(ZoomOut, tr!("Zoom out"));
    name!(FitPage, tr!("Fit page"));
    name!(FitWidth, tr!("Fit width"));
    name!(ActualSize, tr!("Actual size"));
    name!(ShowOutline, tr!("Outline"));
    name!(ShowThumbnails, tr!("Thumbnails"));
    name!(ReadMode, tr!("Read mode"));
    name!(AnnotateMode, tr!("Annotate mode"));
    tr!("Command")
}

#[cfg(test)]
mod tests;
