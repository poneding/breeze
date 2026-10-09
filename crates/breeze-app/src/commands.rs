//! Bindings are the single source of truth for command buttons and Kit's Kbd tooltips.
use crate::*;
use gpui_kit::component::TitleBar;

pub fn bind(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("secondary-o", Open, Some("Breeze")),
        KeyBinding::new("secondary-shift-l", CycleLayout, Some("Breeze && !Input")),
        KeyBinding::new("secondary-,", Preferences, Some("Breeze")),
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
        KeyBinding::new("secondary-3", EditMode, Some("Breeze")),
        KeyBinding::new("secondary-shift-o", ShowOutline, Some("Breeze")),
        KeyBinding::new("secondary-shift-m", ShowThumbnails, Some("Breeze")),
        // Page/zoom shortcuts must not consume editing keys inside an Input.
        KeyBinding::new("pagedown", NextPage, Some("Breeze && !Input")),
        KeyBinding::new("pageup", PreviousPage, Some("Breeze && !Input")),
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
            self.choose_file(cx);
        } else if action.as_any().is::<Home>() {
            self.show_home(cx);
        } else if action.as_any().is::<Preferences>() {
            self.covers.cancel();
            self.screen = Screen::Settings;
            cx.notify();
        } else if action.as_any().is::<CycleTheme>() {
            self.set_appearance(self.theme_preference.next(), window, cx);
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
            if action.as_any().is::<ExportPdf>() {
                self.choose_pdf_export(cx);
            } else if action.as_any().is::<SaveAnnotations>() {
                self.save_annotations(cx);
            } else if action.as_any().is::<SaveAnnotationsAs>() {
                self.save_annotations_as(cx);
            } else if action.as_any().is::<UndoAnnotation>() {
                self.books[self.active].annotations.history.undo();
                self.schedule_annotation_save(self.active, cx);
                cx.notify();
            } else if action.as_any().is::<RedoAnnotation>() {
                self.books[self.active].annotations.history.redo();
                self.schedule_annotation_save(self.active, cx);
                cx.notify();
            } else if action.as_any().is::<CycleLayout>() {
                let next = match self.books[self.active].state.layout {
                    ReadingLayout::Single => ReadingLayout::Continuous,
                    ReadingLayout::Continuous => ReadingLayout::ContinuousSpread,
                    ReadingLayout::ContinuousSpread => ReadingLayout::Spread,
                    ReadingLayout::Spread => ReadingLayout::Single,
                };
                self.set_layout(next, cx);
            } else if action.as_any().is::<CopySelection>() {
                if !self.selected_text.is_empty() {
                    cx.write_to_clipboard(ClipboardItem::new_string(self.selected_text.clone()));
                }
            } else if action.as_any().is::<SearchDocument>() {
                self.search_open = !self.search_open;
                self.sidebar = true;
                self.search_input
                    .update(cx, |input, cx| input.focus(window, cx));
                cx.notify();
            } else if action.as_any().is::<CloseTab>() {
                self.close_book(self.active, cx);
            } else if action.as_any().is::<NextPage>() {
                self.next_page(1, cx);
            } else if action.as_any().is::<PreviousPage>() {
                self.next_page(-1, cx);
            } else if action.as_any().is::<ToggleSidebar>() {
                self.sidebar = !self.sidebar;
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
            } else if action.as_any().is::<ReadMode>()
                || action.as_any().is::<AnnotateMode>()
                || action.as_any().is::<EditMode>()
            {
                self.books[self.active].state.mode = if action.as_any().is::<ReadMode>() {
                    Mode::Read
                } else if action.as_any().is::<AnnotateMode>() {
                    Mode::Annotate
                } else {
                    Mode::Edit
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
        if self.books[index].annotations.dirty() || self.books[index].annotations.saving {
            self.message = "请先保存当前文档的批注，再关闭标签".into();
            cx.notify();
            return;
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
    pub(super) fn header(&self, cx: &mut Context<Self>) -> TitleBar {
        let p = self.p();
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
                    .test_support()
                    .size_full()
                    .min_w_0()
                    .gap_1()
                    .pr_2()
                    .child(
                        self.command("home-tab", IconName::House, "主页", &Home, cx)
                            .selected(self.screen == Screen::Home),
                    )
                    .child(
                        row()
                            .id("document-tabs")
                            .h_full()
                            .flex_1()
                            .min_w_0()
                            .overflow_x_scroll()
                            .gap_1()
                            .children(self.books.iter().enumerate().filter(|(_, b)| b.opened).map(
                                |(index, book)| {
                                    let active =
                                        self.screen == Screen::Reader && index == self.active;
                                    row()
                                        .h_full()
                                        .gap_1()
                                        .when(
                                            !active
                                                && self.books[..index]
                                                    .iter()
                                                    .enumerate()
                                                    .rfind(|(_, b)| b.opened)
                                                    .is_some_and(|(previous, _)| {
                                                        !(self.screen == Screen::Reader
                                                            && previous == self.active)
                                                    }),
                                            |d| d.child(div().w(px(1.)).h(px(14.)).bg(p.line)),
                                        )
                                        .child(
                                            Button::new(("document", index))
                                                .ghost()
                                                .small()
                                                .group("document-tab")
                                                .min_w(px(128.))
                                                .max_w(px(228.))
                                                .pl_2()
                                                .pr_0()
                                                .accessibility_label(book.title.clone())
                                                .tooltip(book.title.clone())
                                                .when(active, |d| d.bg(p.bg))
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        window.focus(&this.focus, cx);
                                                        this.open_book(index, cx);
                                                    },
                                                ))
                                                .child(
                                                    row()
                                                        .w_full()
                                                        .min_w_0()
                                                        .gap_1()
                                                        .child(document_icons::file_icon(
                                                            &book.source,
                                                            cx,
                                                        ))
                                                        .child(
                                                            div()
                                                                .flex_1()
                                                                .min_w_0()
                                                                .text_ellipsis()
                                                                .child(book.title.clone()),
                                                        ),
                                                )
                                                .child(
                                                    div()
                                                        .flex_shrink_0()
                                                        .h_full()
                                                        .invisible()
                                                        .group_hover("document-tab", |s| {
                                                            s.visible()
                                                        })
                                                        .child(
                                                            Button::new(("close", index))
                                                                .ghost()
                                                                .small()
                                                                .icon(IconName::X)
                                                                .accessibility_label("关闭标签")
                                                                .tooltip_with_action(
                                                                    "关闭标签",
                                                                    &CloseTab,
                                                                    Some("Breeze"),
                                                                )
                                                                .on_click(cx.listener(
                                                                    move |this, _, _, cx| {
                                                                        cx.stop_propagation();
                                                                        this.close_book(index, cx);
                                                                    },
                                                                )),
                                                        ),
                                                ),
                                        )
                                },
                            )),
                    )
                    .child(self.command("open", IconName::FilePlusCorner, "打开文档", &Open, cx))
                    .child(self.command(
                        "theme",
                        if self.theme_preference == ThemePreference::System {
                            IconName::Monitor
                        } else if self.dark {
                            IconName::Moon
                        } else {
                            IconName::Sun
                        },
                        "主题模式",
                        &CycleTheme,
                        cx,
                    ))
                    .child(
                        self.command("settings", IconName::Settings2, "设置", &Preferences, cx)
                            .selected(self.screen == Screen::Settings),
                    ),
            )
    }
}
fn action_label(action: &dyn Action) -> &'static str {
    macro_rules! name {
        ($ty:ty,$label:literal) => {
            if action.as_any().is::<$ty>() {
                return $label;
            }
        };
    }
    name!(SaveAnnotations, "保存批注旁置文件");
    name!(ExportPdf, "导出批注 PDF · 新文件，不覆盖原件");
    name!(SaveAnnotationsAs, "选择新的批注保存位置");
    name!(UndoAnnotation, "撤销批注");
    name!(RedoAnnotation, "重做批注");
    name!(SearchDocument, "搜索文档");
    name!(CycleLayout, "切换阅读布局");
    name!(CopySelection, "复制选中文字");
    name!(Open, "打开文档");
    name!(Home, "主页");
    name!(Preferences, "设置");
    name!(CloseTab, "关闭当前标签");
    name!(CycleTheme, "主题模式：跟随系统 → 浅色 → 深色");
    name!(ToggleSidebar, "展开 / 收起导航");
    name!(NextPage, "下一页");
    name!(PreviousPage, "上一页");
    name!(FocusPage, "跳转到页");
    name!(Favorite, "收藏 / 取消收藏");
    name!(ZoomIn, "放大");
    name!(ZoomOut, "缩小");
    name!(FitPage, "适合页面");
    name!(FitWidth, "适合宽度");
    name!(ActualSize, "实际大小");
    name!(ShowOutline, "目录");
    name!(ShowThumbnails, "缩略图");
    name!(ReadMode, "阅读模式");
    name!(AnnotateMode, "标注模式 · 工具预览");
    name!(EditMode, "编辑模式 · 工具预览");
    "命令"
}
