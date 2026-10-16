use super::*;
use gpui_kit::InputEvent as _;
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

#[gpui_kit::test]
fn vertical_arrows_scroll_continuous_pdf_without_turning_a_whole_page(cx: &mut TestAppContext) {
    let (handle, view) = crate::ui_tests::app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| {
            view.reading.sizes = Arc::new(vec![(612., 792.); 8]);
            view.books[0].state.zoom = Zoom::Percent(100);
            // Keep worker jobs out of this scroll-geometry test.
            view.loading = true;
        });
        for layout in [ReadingLayout::Continuous, ReadingLayout::ContinuousSpread] {
            view.update(cx, |view, _| {
                view.books[0].state.layout = layout;
                view.reading.pages_scroll.set_offset(point(px(0.), px(0.)));
            });
            window.render_frame(cx);
            view.read(cx)
                .reading
                .pages_scroll
                .set_offset(point(px(0.), px(0.)));
            window.render_frame(cx);
            window.press("up", cx);
            assert_eq!(view.read(cx).reading.pages_scroll.offset().y, px(0.));
            window.press("down", cx);
            assert_eq!(view.read(cx).reading.pages_scroll.offset().y, px(-48.));
            assert_eq!(view.read(cx).books[0].state.navigation.page(), 0);
            window.dispatch_event(
                KeyDownEvent {
                    keystroke: Keystroke::parse("down").unwrap(),
                    is_held: true,
                    prefer_character_input: false,
                }
                .to_platform_input(),
                cx,
            );
            assert_eq!(view.read(cx).reading.pages_scroll.offset().y, px(-96.));
            window.press("up", cx);
            window.press("up", cx);
            assert_eq!(view.read(cx).reading.pages_scroll.offset().y, px(0.));

            view.read(cx).reading.pages_scroll.scroll_to_bottom();
            window.render_frame(cx);
            let bottom = view.read(cx).reading.pages_scroll.offset();
            assert!(bottom.y < px(-1000.));
            window.press("down", cx);
            assert_eq!(view.read(cx).reading.pages_scroll.offset(), bottom);
            window.press("up", cx);
            assert_eq!(
                view.read(cx).reading.pages_scroll.offset().y,
                bottom.y + px(48.)
            );

            window.press("secondary-g", cx);
            let before = view.read(cx).reading.pages_scroll.offset();
            window.press("up", cx);
            window.press("down", cx);
            assert_eq!(view.read(cx).reading.pages_scroll.offset(), before);
            window.focus(&view.read(cx).focus.clone(), cx);
        }

        for layout in [ReadingLayout::Single, ReadingLayout::Spread] {
            view.update(cx, |view, _| view.books[0].state.layout = layout);
            window.render_frame(cx);
            let before = view.read(cx).reading.pages_scroll.offset();
            window.press("down", cx);
            window.press("up", cx);
            assert_eq!(view.read(cx).reading.pages_scroll.offset(), before);
            assert_eq!(view.read(cx).books[0].state.navigation.page(), 0);
        }
    })
    .unwrap();
}

#[gpui_kit::test]
fn vertical_arrows_scroll_markdown_and_preserve_search_input(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("arrow-scroll.md");
    let markdown: String = (0..80)
        .map(|index| format!("## Heading {index}\n\nParagraph {index} for keyboard scrolling.\n\n"))
        .collect();
    std::fs::write(&path, markdown).unwrap();
    let (handle, view) = crate::ui_tests::app(cx);
    cx.update_window(handle, |_, _, cx| {
        view.update(cx, |view, cx| {
            view.books[0].source = Source::File(path);
            view.load(cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.render_frame(cx);
        assert!(view.read(cx).reflow.markdown.is_some());
        let position = |cx: &App| {
            let top = view.read(cx).reflow.list.logical_scroll_top();
            (top.item_ix, top.offset_in_item)
        };
        let start = position(cx);
        window.press("up", cx);
        assert_eq!(position(cx), start);
        window.press("down", cx);
        assert!(position(cx) > start);
        window.press("up", cx);
        assert_eq!(position(cx), start);

        for _ in 0..24 {
            window.press("down", cx);
        }
        assert!(view.read(cx).books[0].reflow.markdown_offset > 0);
        assert!(view.read(cx).books[0].reflow.progress > 0.);
        assert_eq!(view.read(cx).reflow.active_block, position(cx).0);

        window.press("secondary-f", cx);
        let before = position(cx);
        window.input("text", cx);
        window.press("up", cx);
        window.press("down", cx);
        assert_eq!(position(cx), before);
        assert_eq!(view.read(cx).search_input.read(cx).value().as_ref(), "text");

        window.focus(&view.read(cx).focus.clone(), cx);
        view.read(cx).reflow.list.scroll_to_end();
        window.render_frame(cx);
        let end = position(cx);
        window.press("down", cx);
        assert_eq!(position(cx), end);
        assert_eq!(view.read(cx).reflow.active_block, end.0);
        assert_eq!(view.read(cx).books[0].reflow.progress, 1.);
        window.press("up", cx);
        assert!(position(cx) < end);
    })
    .unwrap();
}
