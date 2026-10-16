use super::*;
use gpui_kit::test::TestWindowExt;

#[gpui_kit::test]
fn arrow_page_keys_respect_reader_layout_and_boundaries(cx: &mut TestAppContext) {
    let (handle, view) = crate::ui_tests::app(cx);
    cx.update_window(handle, |_, window, cx| {
        for (layout, stride, last) in [
            (ReadingLayout::Single, 1, 7),
            (ReadingLayout::Continuous, 1, 7),
            (ReadingLayout::Spread, 2, 6),
            (ReadingLayout::ContinuousSpread, 2, 6),
        ] {
            view.update(cx, |view, _| {
                view.books[0].state.layout = layout;
                view.books[0].state.navigation.go_to(0);
            });
            window.render_frame(cx);
            window.press("left", cx);
            assert_eq!(view.read(cx).books[0].state.navigation.page(), 0);
            window.press("right", cx);
            assert_eq!(view.read(cx).books[0].state.navigation.page(), stride);
            window.press("left", cx);
            assert_eq!(view.read(cx).books[0].state.navigation.page(), 0);

            // The original keys remain aliases for the same layout-aware commands.
            window.press("pagedown", cx);
            assert_eq!(view.read(cx).books[0].state.navigation.page(), stride);
            window.press("pageup", cx);
            assert_eq!(view.read(cx).books[0].state.navigation.page(), 0);

            view.update(cx, |view, _| {
                view.books[0].state.navigation.go_to(last);
            });
            window.press("right", cx);
            assert_eq!(view.read(cx).books[0].state.navigation.page(), last);
        }
    })
    .unwrap();
}

#[gpui_kit::test]
fn arrow_page_keys_preserve_text_input_editing(cx: &mut TestAppContext) {
    let (handle, view) = crate::ui_tests::app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| {
            view.books[0].state.navigation.go_to(3);
        });
        window.render_frame(cx);
        window.press("secondary-g", cx);
        window.input("48", cx);
        window.press("left", cx);
        window.input("3", cx);
        assert_eq!(view.read(cx).page_input.read(cx).value().as_ref(), "438");
        window.press("right", cx);
        window.input("5", cx);
        assert_eq!(view.read(cx).page_input.read(cx).value().as_ref(), "4385");
        assert_eq!(view.read(cx).books[0].state.navigation.page(), 3);

        window.press("secondary-f", cx);
        window.render_frame(cx);
        window.input("abc", cx);
        window.press("left", cx);
        window.input("x", cx);
        assert_eq!(view.read(cx).search_input.read(cx).value().as_ref(), "abxc");
        window.press("right", cx);
        window.input("y", cx);
        assert_eq!(
            view.read(cx).search_input.read(cx).value().as_ref(),
            "abxcy"
        );
        assert_eq!(view.read(cx).books[0].state.navigation.page(), 3);
    })
    .unwrap();
}
