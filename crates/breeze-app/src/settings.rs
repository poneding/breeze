use crate::*;
use gpui_kit::component::{
    switch::Switch,
    tab::{Tab, TabBar},
};
impl Breeze {
    pub(super) fn settings(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = self.p();
        let theme = TabBar::new("appearance")
            .segmented()
            .small()
            .selected_index(self.theme_preference.index())
            .on_click(cx.listener(|this, index, window, cx| {
                this.set_appearance(ThemePreference::ALL[*index], window, cx)
            }))
            .children(ThemePreference::ALL.map(|preference| Tab::new().label(preference.label())));
        let language = TabBar::new("language")
            .segmented()
            .small()
            .selected_index(self.language)
            .on_click(cx.listener(|this, index, _, cx| {
                this.language = *index;
                cx.notify();
            }))
            .child(Tab::new().label("简体中文"))
            .child(Tab::new().label("English"));
        let updates = TabBar::new("updates")
            .segmented()
            .small()
            .selected_index(self.update_policy)
            .on_click(cx.listener(|this, index, _, cx| {
                this.update_policy = *index;
                cx.notify();
            }))
            .child(Tab::new().label("手动"))
            .child(Tab::new().label("自动检查"))
            .child(Tab::new().label("从不"));
        let window_policy = TabBar::new("window-policy")
            .segmented()
            .small()
            .selected_index(self.window_policy)
            .on_click(cx.listener(|this, index, _, cx| {
                this.window_policy = *index;
                cx.notify();
            }))
            .child(Tab::new().label("默认"))
            .child(Tab::new().label("最大化"))
            .child(Tab::new().label("适合文档"));
        column().id("settings-body").flex_1().min_h_0().overflow_y_scroll().px_6().py_4().gap_2()
            .child(row().gap_3().mb_2().child(div().text_size(px(17.)).font_weight(FontWeight::SEMIBOLD).child("设置")).child(div().text_size(px(11.)).text_color(p.muted).child("外观即时生效 · 自动保存在本机")))
            .when_some(self.data_dir.as_ref(),|d,path|d.child(row().gap_2().child(div().flex_1().text_size(px(11.)).text_color(p.muted).child(format!("数据目录：{}",path.display()))).child(Button::new("reveal-state-directory").ghost().small().icon(IconName::FolderOpen).tooltip("显示本地数据目录").on_click({let path=path.clone();move|_,_,cx|cx.reveal_path(&path)}))))
            .when(self.storage_error.is_some()&&self.storage.is_some(),|d|d.child(Button::new("retry-state-save").small().label("重试保存本地状态").on_click(cx.listener(|this,_,_,cx|{this.storage_ready=true;this.last_snapshot=None;this.persist_if_changed(cx);cx.notify();}))))
            .child(self.setting_row("主题模式","跟随系统会实时响应系统明暗变化。",theme))
            .child(self.scheme_row(false,cx))
            .child(self.scheme_row(true,cx))
            .child(self.setting_row("默认打开工具","不强制修改系统文件关联。",Button::new("file-association").small().label("系统关联 · 待接入").disabled(true)))
            .child(self.setting_row("界面语言","原型文案暂为中文；选择入口预览。",language))
            .child(self.setting_row("检查更新","原型不联网。",updates))
            .child(self.setting_row("打开文件时的窗口","新文档载入后执行；切换标签/翻页不改变窗口，也不改变页面缩放。",window_policy))
            .child(self.setting_row("记住上次打开的文件","控制启动时恢复标签。",Switch::new("remember-files").small().checked(self.remember_files).on_change(cx.listener(|this,value,_,cx|{this.remember_files = *value;cx.notify();}))))
            .child(self.setting_row("记住阅读位置","关闭后不保存或恢复页码；不影响标签恢复。",Switch::new("remember-position").small().checked(self.remember_position).on_change(cx.listener(|this,value,_,cx|{this.remember_position = *value;cx.notify();}))))
            .child(div().mt_3().text_size(px(12.)).text_color(p.muted).child("快捷键 · ⌘/Ctrl+O 打开　⌘/Ctrl+, 设置　⌘/Ctrl+B 侧栏　⌘/Ctrl+G 跳页　Ctrl+Tab 切换标签"))
            .child(div().text_size(px(11.)).text_color(p.muted).child("其余快捷键悬停按钮查看。文档页面保持原色，不受配色影响。"))
    }
    fn scheme_row(&self, dark: bool, cx: &mut Context<Self>) -> Div {
        let selected = if dark {
            self.dark_scheme
        } else {
            self.light_scheme
        };
        self.setting_row(
            if dark { "深色配色" } else { "浅色配色" },
            "分别持久保存，切换模式不重置。",
            row()
                .flex_wrap()
                .justify_end()
                .gap_1()
                .max_w(px(610.))
                .children(ColorScheme::ALL.into_iter().enumerate().map(|(i, scheme)| {
                    let colors = scheme.colors(dark);
                    Button::new((
                        if dark {
                            "dark-palette"
                        } else {
                            "light-palette"
                        },
                        i,
                    ))
                    .ghost()
                    .small()
                    .selected(selected == scheme)
                    .icon(
                        gpui_kit::component::Icon::new(IconName::Circle).text_color(rgb(colors[6])),
                    )
                    .label(scheme.name(dark))
                    .tooltip(format!(
                        "{} · {}配色",
                        scheme.name(dark),
                        if dark { "深色" } else { "浅色" }
                    ))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if dark {
                            this.dark_scheme = scheme
                        } else {
                            this.light_scheme = scheme
                        }
                        if this.dark == dark {
                            this.apply_colors(cx);
                        }
                        cx.notify();
                    }))
                })),
        )
    }
    fn setting_row(&self, title: &str, description: &str, control: impl IntoElement) -> Div {
        let p = self.p();
        row()
            .gap_4()
            .py_2()
            .border_b_1()
            .border_color(p.line)
            .child(
                column()
                    .min_w(px(200.))
                    .flex_1()
                    .gap_1()
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .child(title.to_string()),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(p.muted)
                            .child(description.to_string()),
                    ),
            )
            .child(div().flex_shrink_0().child(control))
    }
}
