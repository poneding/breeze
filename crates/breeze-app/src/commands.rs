//! Bindings are the single source of truth for command buttons and Kit's Kbd tooltips.
use crate::*;
use gpui_kit::base::InteractiveElementExt as _;
use gpui_kit::component::{TitleBar, button::ButtonCustomVariant};

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
        KeyBinding::new("secondary-shift-s", SaveAnnotationsAs, Some("Breeze")),
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
            let tabs: Vec<_> = self
                .books
                .iter()
                .enumerate()
                .filter(|(_, b)| b.opened)
                .map(|(i, _)| i)
                .collect();
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
                    || action.as_any().is::<SaveAnnotationsAs>()
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
            } else if action.as_any().is::<SaveAnnotationsAs>() {
                self.save_annotations_as(cx);
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
                self.books[self.active].favorite = !self.books[self.active].favorite;
                cx.notify();
            } else if action.as_any().is::<FocusPage>() {
                let n = self.books[self.active].state.navigation.page() + 1;
                self.page_input.update(cx, |input, cx| {
                    input.set_value(n.to_string(), window, cx);
                    input.focus(window, cx);
                    input.select_all(window, cx);
                });
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
            .is_some_and(|editor| editor.read(cx).dirty())
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
            if let Some(next) = self.books.iter().position(|b| b.opened) {
                self.open_book(next, cx)
            } else {
                self.show_home(cx)
            }
        } else {
            cx.notify()
        }
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

    fn document_tab(&self, index: usize, width: gpui_kit::Pixels, cx: &Context<Self>) -> Button {
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
        Button::new(("document", index))
            .custom(
                ButtonCustomVariant::new(cx)
                    .hover(tab_hover)
                    .active(tab_hover),
            )
            .small()
            .group("document-tab")
            .flex_initial()
            .w(width)
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
            )
    }

    pub(super) fn header(&self, window: &Window, cx: &mut Context<Self>) -> TitleBar {
        let p = self.p(cx);
        let tabs: Vec<_> = self
            .books
            .iter()
            .enumerate()
            .filter(|(_, book)| book.opened)
            .map(|(index, _)| (index, self.document_tab_width(index, window, cx)))
            .collect();
        // Cap the strip at its actual content width so Plus follows short titles.
        let tab_strip_width = px(10.)
            + tabs
                .iter()
                .map(|(_, width)| *width)
                .sum::<gpui_kit::Pixels>()
            + window.rem_size() * 0.25 * tabs.len().saturating_sub(1) as f32;
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
                    .test_support()
                    // A zero basis prevents Kit's nonshrinking titlebar wrapper
                    // from requesting the tabs' combined intrinsic width.
                    .h_full()
                    .w_0()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .pr_2()
                    .child(
                        self.command("home-tab", IconName::House, tr!("Home"), &Home, cx)
                            .selected(
                                self.screen == Screen::Home && self.home_page == HomePage::Library,
                            ),
                    )
                    .child(
                        row()
                            .id("document-tab-group")
                            .h_full()
                            .w_0()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(
                                row()
                                    .id("document-tabs")
                                    .test_support()
                                    .h_full()
                                    .w_0()
                                    .flex_1()
                                    .max_w(tab_strip_width)
                                    .min_w_0()
                                    .overflow_x_scroll()
                                    // Keep the first tab's shadow inside the scroll clip.
                                    .pl(px(4.))
                                    // Keep the last tab's shadow inside the clip and away from Plus.
                                    .pr(px(6.))
                                    .gap_1()
                                    .children(
                                        tabs.into_iter().map(|(index, width)| {
                                            self.document_tab(index, width, cx)
                                        }),
                                    ),
                            )
                            .child(
                                row()
                                    .id("document-tab-add")
                                    .test_support()
                                    .flex_shrink_0()
                                    .gap_1()
                                    .child(
                                        div()
                                            .id("document-tab-divider")
                                            .test_support()
                                            .w(px(1.))
                                            .h(px(14.))
                                            .bg(p.line),
                                    )
                                    .child(
                                        self.command(
                                            "add-document",
                                            IconName::Plus,
                                            tr!("Open document"),
                                            &Open,
                                            cx,
                                        )
                                        .text_color(p.muted),
                                    ),
                            ),
                    ),
            )
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
    name!(SaveAnnotationsAs, tr!("Choose annotation save location"));
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
