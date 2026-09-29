//! Bindings are the single source of truth for command buttons and Kit's Kbd tooltips.
use crate::*;
use gpui_kit::component::TitleBar;

pub fn bind(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("secondary-o", Open, Some("Breeze")),
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
        KeyBinding::new("secondary--", ZoomOut, Some("Breeze && !Input")),
        KeyBinding::new("secondary-0", ActualSize, Some("Breeze && !Input")),
        KeyBinding::new("secondary-9", FitPage, Some("Breeze && !Input")),
        KeyBinding::new("secondary-8", FitWidth, Some("Breeze && !Input")),
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
            .xsmall()
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
        if action.as_any().is::<Open>() {
            self.choose_file(cx);
        } else if action.as_any().is::<Home>() {
            self.show_home(cx);
        } else if action.as_any().is::<Preferences>() {
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
            if action.as_any().is::<CloseTab>() {
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
                let s = &mut self.books[self.active].state;
                s.zoom = if action.as_any().is::<ZoomIn>() {
                    s.zoom.step(25)
                } else if action.as_any().is::<ZoomOut>() {
                    s.zoom.step(-25)
                } else if action.as_any().is::<FitPage>() {
                    Zoom::FitPage
                } else if action.as_any().is::<FitWidth>() {
                    Zoom::FitWidth
                } else if action.as_any().is::<ActualSize>() {
                    Zoom::Percent(100)
                } else {
                    s.zoom
                };
                cx.notify();
            }
        }
    }
    pub(super) fn close_book(&mut self, index: usize, cx: &mut Context<Self>) {
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
        TitleBar::new().bg(p.panel).border_color(p.line).child(
            row()
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
                                row()
                                    .h(px(28.))
                                    .min_w(px(120.))
                                    .max_w(px(228.))
                                    .gap_1()
                                    .px_1()
                                    .rounded_t_md()
                                    .when(
                                        self.screen == Screen::Reader && index == self.active,
                                        |d| d.bg(p.bg),
                                    )
                                    .child(
                                        Button::new(("document", index))
                                            .ghost()
                                            .xsmall()
                                            .max_w(px(190.))
                                            .overflow_hidden()
                                            .icon(IconName::FileText)
                                            .label(book.title.clone())
                                            .tooltip(book.title.clone())
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.open_book(index, cx)
                                            })),
                                    )
                                    .child(
                                        Button::new(("close", index))
                                            .ghost()
                                            .xsmall()
                                            .icon(IconName::X)
                                            .accessibility_label("关闭标签")
                                            .tooltip_with_action(
                                                "关闭标签",
                                                &CloseTab,
                                                Some("Breeze"),
                                            )
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.close_book(index, cx)
                                            })),
                                    )
                            },
                        )),
                )
                .child(self.command("open", IconName::FolderOpen, "打开文档", &Open, cx))
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
    name!(FitWidth, "适合页宽");
    name!(ActualSize, "实际大小");
    name!(ShowOutline, "目录");
    name!(ShowThumbnails, "缩略图");
    name!(ReadMode, "阅读模式");
    name!(AnnotateMode, "标注模式 · 工具预览");
    name!(EditMode, "编辑模式 · 工具预览");
    "命令"
}
