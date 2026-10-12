//! Settings stay deliberately short: a title and one control per row.
use crate::*;
use gpui_kit::component::{
    select::Select,
    switch::Switch,
    tab::{Tab, TabBar},
};
impl Breeze {
    pub(super) fn settings_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.p(cx);
        let theme = Select::new(&self.theme_select)
            .id("theme-select")
            .accessibility_label(tr!("主题"))
            .search_placeholder(tr!("搜索主题"))
            .small()
            .w(px(220.));
        let language = Select::new(&self.language_select)
            .id("language-select")
            .accessibility_label(tr!("界面语言"))
            .small()
            .w(px(220.));
        let updates = TabBar::new("updates")
            .segmented()
            .small()
            .selected_index(self.update_policy)
            .on_click(cx.listener(|this, index, _, cx| {
                this.update_policy = *index;
                this.check_updates(true, cx);
                cx.notify();
            }))
            .child(Tab::new().label(tr!("手动")))
            .child(Tab::new().label(tr!("自动检查")))
            .child(Tab::new().label(tr!("从不")));
        let window_policy = TabBar::new("window-policy")
            .segmented()
            .small()
            .selected_index(self.window_policy)
            .on_click(cx.listener(|this, index, _, cx| {
                this.window_policy = *index;
                cx.notify();
            }))
            .child(Tab::new().label(tr!("默认")))
            .child(Tab::new().label(tr!("最大化")))
            .child(Tab::new().label(tr!("适合文档")));
        column()
            .id("settings-body")
            .test_support()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .gap_1()
            .child(
                div()
                    .text_size(px(16.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .mb_2()
                    .child(tr!("设置")),
            )
            .when(
                self.storage_error.is_some() && self.storage.is_some(),
                |d| {
                    d.child(
                        Button::new("retry-state-save")
                            .small()
                            .label(tr!("重试保存本地状态"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.storage_ready = true;
                                this.last_snapshot = None;
                                this.persist_if_changed(cx);
                                cx.notify();
                            })),
                    )
                },
            )
            .child(Self::setting_row(tr!("主题"), theme, p))
            .child(Self::setting_row(tr!("界面语言"), language, p))
            .child(Self::setting_row(tr!("检查更新"), updates, p))
            .child(Self::setting_row(tr!("打开文件时的窗口"), window_policy, p))
            .child(Self::setting_row(
                tr!("记住上次打开的文件"),
                Switch::new("remember-files")
                    .small()
                    .checked(self.remember_files)
                    .on_change(cx.listener(|this, value, _, cx| {
                        this.remember_files = *value;
                        cx.notify();
                    })),
                p,
            ))
            .child(Self::setting_row(
                tr!("记住阅读位置"),
                Switch::new("remember-position")
                    .small()
                    .checked(self.remember_position)
                    .on_change(cx.listener(|this, value, _, cx| {
                        this.remember_position = *value;
                        cx.notify();
                    })),
                p,
            ))
            .child(Self::setting_row(
                tr!("默认打开工具"),
                Button::new("file-association")
                    .small()
                    .label(tr!("系统关联 · 待接入"))
                    .disabled(true),
                p,
            ))
    }

    fn setting_row(title: &str, control: impl IntoElement, p: Palette) -> Div {
        row()
            .gap_4()
            .py_2()
            .border_b_1()
            .border_color(p.line)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .font_weight(FontWeight::MEDIUM)
                    .child(title.to_string()),
            )
            .child(div().flex_shrink_0().child(control))
    }
}
