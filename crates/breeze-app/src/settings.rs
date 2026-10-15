//! Settings stay deliberately short: a title and one control per row.
use crate::*;
use gpui_kit::base::actions::{SelectDown, SelectUp};
use gpui_kit::component::{
    select::Select,
    switch::Switch,
    tab::{Tab, TabBar},
};
impl Breeze {
    pub(super) fn settings_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.p(cx);
        // Kit reports the dropdown cursor only as an index, so an arrow key schedules a
        // preview of the row it is about to move to. Confirming the row commits it, while
        // Escape, an outside click, or focus loss restores the committed theme.
        let theme =
            div()
                .w(px(220.))
                .capture_action(cx.listener(|this, _: &SelectDown, window, cx| {
                    this.schedule_theme_preview(window, cx)
                }))
                .capture_action(cx.listener(|this, _: &SelectUp, window, cx| {
                    this.schedule_theme_preview(window, cx)
                }))
                .child(
                    Select::new(&self.theme_select)
                        .id("theme-select")
                        .accessibility_label(tr!("Theme"))
                        .search_placeholder(tr!("Search themes"))
                        .small(),
                );
        let language = Select::new(&self.language_select)
            .id("language-select")
            .accessibility_label(tr!("Interface language"))
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
            .child(Tab::new().label(tr!("Manual")))
            .child(Tab::new().label(tr!("Automatic")));
        let window_policy = TabBar::new("window-policy")
            .segmented()
            .small()
            .selected_index(self.window_policy)
            .on_click(cx.listener(|this, index, _, cx| {
                this.window_policy = *index;
                cx.notify();
            }))
            .child(Tab::new().label(tr!("Default")))
            .child(Tab::new().label(tr!("Maximized")))
            .child(Tab::new().label(tr!("Fit document")));
        Breeze::page_frame(
            "settings-body",
            column()
                .w_full()
                .gap_1()
                .child(
                    div()
                        .text_size(px(16.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .mb_2()
                        .child(tr!("Settings")),
                )
                .when(
                    self.storage_error.is_some() && self.storage.is_some(),
                    |d| {
                        d.child(
                            Button::new("retry-state-save")
                                .small()
                                .label(tr!("Retry saving local state"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.storage_ready = true;
                                    this.last_snapshot = None;
                                    this.persist_if_changed(cx);
                                    cx.notify();
                                })),
                        )
                    },
                )
                .child(Self::setting_row(tr!("Theme"), theme, p))
                .child(Self::setting_row(tr!("Interface language"), language, p))
                .child(Self::setting_row(tr!("Check for updates"), updates, p))
                .child(Self::setting_row(
                    tr!("Window when opening files"),
                    window_policy,
                    p,
                ))
                .child(Self::setting_row(
                    tr!("Remember open files"),
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
                    tr!("Remember reading position"),
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
                    tr!("Default PDF app"),
                    // Breeze declares PDF to the OS so file managers offer it
                    // under Open With. Which application is the default stays a
                    // system-owned choice this app must not fake or force.
                    div()
                        .id("default-pdf-app")
                        .test_support()
                        .text_color(p.muted)
                        .child(tr!(
                            "Offered under Open With · Choose the default in the system"
                        )),
                    p,
                )),
        )
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
