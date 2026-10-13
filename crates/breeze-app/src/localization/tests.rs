use crate::*;
use gpui_kit::test::TestWindowExt;

#[gpui_kit::test]
fn language_select_changes_chrome_and_preserves_content(cx: &mut TestAppContext) {
    let (handle, view) = ui_tests::app(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press("secondary-,", cx);
        window.render_frame(cx);
        // Detection is the default and sits above the explicit languages.
        assert_eq!(
            view.read(cx).language_select.read(cx).selected_value(),
            Some(&i18n::LanguageItem::Choice(i18n::LanguagePreference::Auto))
        );
        window.click("language-select", cx);
        window.render_frame(cx);
        // The Languages label is a row but not a choice.
        window.press("down", cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(view.read(cx).language, 0);
        assert_eq!(window.find("home-settings").label(), Some("设置"));
        window.click("language-select", cx);
        window.render_frame(cx);
        window.press("down", cx);
        window.press("down", cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(view.read(cx).language, 1);
        assert_eq!(window.find("home-settings").label(), Some("Settings"));
        assert_eq!(
            view.read(cx).language_select.read(cx).selected_value(),
            Some(&i18n::LanguageItem::Choice(
                i18n::LanguagePreference::English
            ))
        );
        // English sits below the detection row and the Languages label.
        assert_eq!(
            view.read(cx).language_select.read(cx).selected_index(cx),
            Some(gpui_kit::component::IndexPath::new(2))
        );
        assert_eq!(view.read(cx).books[0].title, "Field notes");
        assert_eq!(
            view.read(cx)
                .library_input
                .read(cx)
                .presentation()
                .placeholder()
                .as_ref(),
            "Filter document names"
        );
        view.update(cx, |view, cx| {
            view.category_input.update(cx, |input, cx| {
                input.set_value("中文 category {}", window, cx)
            });
        });
        window.click("language-select", cx);
        window.render_frame(cx);
        window.press("down", cx);
        window.press("down", cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(view.read(cx).language, 3);
        assert_eq!(window.find("home-settings").label(), Some("設定"));
        assert_eq!(
            view.read(cx).category_input.read(cx).value().as_ref(),
            "中文 category {}"
        );
        view.update(cx, |view, cx| {
            let saved =
                serde_json::from_slice(&serde_json::to_vec(&view.snapshot()).unwrap()).unwrap();
            view.language = i18n::LanguagePreference::Auto as usize;
            view.refresh_language(window, cx);
            view.restore(saved, cx);
            view.refresh_language(window, cx);
            assert_eq!(view.language, 3);
            assert_eq!(view.snapshot().language, 3);
            view.screen = Screen::Reader;
        });
        window.render_frame(cx);
        assert_eq!(window.within("nav-tabs").find(0usize).label(), Some("縮圖"));
        assert_eq!(window.find("reader-toolbar").bounds().size.height, px(38.));
        assert_eq!(window.find("status-scroll").bounds().size.height, px(32.));
        view.update(cx, |view, cx| {
            view.language = i18n::LanguagePreference::SimplifiedChinese as usize;
            view.refresh_language(window, cx);
        });
    })
    .unwrap();
}
