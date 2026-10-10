use crate::*;
use gpui_kit::component::{
    select::Select,
    switch::Switch,
    tab::{Tab, TabBar},
};
impl Breeze {
    pub(super) fn settings(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = self.p(cx);
        let theme = Select::new(&self.theme_select)
            .id("theme-select")
            .accessibility_label("主题")
            .search_placeholder("搜索主题")
            .small()
            .w(px(220.));
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
            .child(Self::setting_row("主题","使用 GPUI Kit 内置主题；跟随系统会实时响应系统明暗变化。",theme, p))
            .child(Self::setting_row("默认打开工具","不强制修改系统文件关联。",Button::new("file-association").small().label("系统关联 · 待接入").disabled(true), p))
            .child(Self::setting_row("界面语言","原型文案暂为中文；选择入口预览。",language, p))
            .child(Self::setting_row("检查更新","原型不联网。",updates, p))
            .child(Self::setting_row("打开文件时的窗口","新文档载入后执行；切换标签/翻页不改变窗口，也不改变页面缩放。",window_policy, p))
            .child(Self::setting_row("记住上次打开的文件","控制启动时恢复标签。",Switch::new("remember-files").small().checked(self.remember_files).on_change(cx.listener(|this,value,_,cx|{this.remember_files = *value;cx.notify();})), p))
            .child(Self::setting_row("记住阅读位置","关闭后不保存或恢复页码；不影响标签恢复。",Switch::new("remember-position").small().checked(self.remember_position).on_change(cx.listener(|this,value,_,cx|{this.remember_position = *value;cx.notify();})), p))
            .child(div().mt_3().text_size(px(12.)).text_color(p.muted).child("快捷键 · ⌘/Ctrl+O 打开　⌘/Ctrl+, 设置　⌘/Ctrl+B 侧栏　⌘/Ctrl+G 跳页　Ctrl+Tab 切换标签"))
            .child(div().text_size(px(11.)).text_color(p.muted).child("其余快捷键悬停按钮查看。文档页面保持原色，不受主题影响。"))
    }
    fn setting_row(title: &str, description: &str, control: impl IntoElement, p: Palette) -> Div {
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
