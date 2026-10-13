//! Refresh persistent Kit entities without recreating inputs or changing their values.
use crate::*;
impl Breeze {
    pub(super) fn refresh_language(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.applied_language == Some(self.language) {
            return;
        }
        let language = i18n::Language::from_saved(self.language);
        i18n::set_language(language);
        gpui_kit::component::set_locale(language.locale());
        self.applied_language = Some(self.language);
        self.language_select.update(cx, |select, cx| {
            select.set_selected_value(&language, window, cx);
        });
        self.theme_select.update(cx, |select, cx| {
            select.set_items(
                appearance::ThemeChoices::new(ThemePreference::groups(cx)),
                window,
                cx,
            );
            select.set_selected_value(&self.theme_preference, window, cx);
        });
        for (input, placeholder) in [
            (&self.search_input, tr!("搜索文档文字（回车）")),
            (&self.library_input, tr!("筛选文档名称")),
            (&self.category_input, tr!("分类名称")),
        ] {
            input.update(cx, |input, cx| {
                input.set_placeholder(placeholder, window, cx)
            });
        }
        self.refresh_outline_language(window, cx);
        self.refresh_annotation_language(window, cx);
        self.tool_settings.dismiss();
        window.set_window_title(tr!("Breeze · 本地阅读工作台"));
    }
}

#[cfg(test)]
mod tests;
