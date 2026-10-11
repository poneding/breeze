use crate::*;
use gpui_kit::test::TestWindowExt;

#[gpui_kit::test]
fn language_select_changes_chrome_and_preserves_content(cx: &mut TestAppContext) {
    let (handle, view) = ui_tests::app(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("settings", cx);
        window.render_frame(cx);
        window.click("language-select", cx);
        window.render_frame(cx);
        window.press("down", cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(view.read(cx).language, 1);
        assert_eq!(window.find("settings").label(), Some("Settings"));
        assert_eq!(
            view.read(cx).language_select.read(cx).selected_value(),
            Some(&i18n::Language::English)
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
        window.press("enter", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(view.read(cx).language, 2);
        assert_eq!(window.find("settings").label(), Some("設定"));
        assert_eq!(
            view.read(cx).category_input.read(cx).value().as_ref(),
            "中文 category {}"
        );
        view.update(cx, |view, cx| {
            let saved =
                serde_json::from_slice(&serde_json::to_vec(&view.snapshot()).unwrap()).unwrap();
            view.language = 0;
            view.refresh_language(window, cx);
            view.restore(saved, cx);
            view.refresh_language(window, cx);
            assert_eq!(view.language, 2);
            assert_eq!(view.snapshot().language, 2);
            view.screen = Screen::Reader;
        });
        window.render_frame(cx);
        assert_eq!(window.within("nav-tabs").find(0usize).label(), Some("縮圖"));
        assert_eq!(window.find("reader-toolbar").bounds().size.height, px(38.));
        assert_eq!(window.find("status-scroll").bounds().size.height, px(32.));
        view.update(cx, |view, cx| {
            view.language = 0;
            view.refresh_language(window, cx);
        });
    })
    .unwrap();
}
