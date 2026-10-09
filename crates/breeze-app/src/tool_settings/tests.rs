use super::*;
use gpui_kit::test::TestWindowExt;

fn advance(cx: &mut TestAppContext, handle: AnyWindowHandle, milliseconds: u64) {
    cx.executor()
        .advance_clock(Duration::from_millis(milliseconds));
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.render_frame(cx);
    })
    .unwrap();
}

#[gpui_kit::test]
fn hover_settings_remain_interactive_and_style_only_their_tool(cx: &mut TestAppContext) {
    let worker = PdfWorker::new().unwrap();
    let page = futures::executor::block_on(worker.request(Source::Sample, 0).unwrap())
        .unwrap()
        .unwrap();
    let preview = Preview::from_page(page).unwrap();
    let (handle, view) = crate::ui_tests::app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| {
            view.preview = Some(preview.clone());
            view.books[0].annotations.ready = true;
            view.books[0].state.mode = Mode::Annotate;
            view.tool = 0;
        });
        window.render_frame(cx);
        window.hover(("tool", 4usize), cx);
    })
    .unwrap();
    advance(cx, handle, 400);
    cx.update_window(handle, |_, window, cx| {
        assert!(window.find("tool-settings-content").visible());
        window.hover(("tool-color", 5usize), cx);
    })
    .unwrap();
    advance(cx, handle, 500);
    cx.update_window(handle, |_, window, cx| {
        assert!(window.find("tool-settings-content").visible());
        for index in 0..8usize {
            assert!(window.find(("tool-color", index)).visible());
        }
        window.click(("tool-color", 5usize), cx);
        window.click(("ink-width", 8usize), cx);
        window.click(("tool-opacity", 60usize), cx);
        let settings = &view.read(cx).tool_settings;
        assert_eq!(
            settings.annotation[4],
            AnnotationStyle {
                color: CategoryColor::Blue,
                width: 8.,
                opacity: 60
            }
        );
        assert_eq!(settings.annotation[0].color, CategoryColor::Amber);
        assert_eq!(settings.annotation[0].opacity, 40);
        assert_eq!(view.read(cx).tool, 0, "Hovering a tool must not select it");
        assert!(view.read(cx).books[0].annotations.history.items.is_empty());
        window.hover("document-viewport", cx);
    })
    .unwrap();
    advance(cx, handle, 400);
    cx.update_window(handle, |_, window, cx| {
        assert!(window.try_find("tool-settings-content").is_none());
        window.click(("tool", 4usize), cx);
        let bounds = window.find("page-surface").bounds();
        let start = bounds.origin + point(px(40.), px(100.));
        window.drag(start, start + point(px(100.), px(40.)), cx);
        let annotation = view.read(cx).books[0]
            .annotations
            .history
            .items
            .last()
            .unwrap()
            .clone();
        assert_eq!(annotation.color, colors::components(CategoryColor::Blue));
        assert_eq!(annotation.width, 8.);
        assert_eq!(annotation.opacity, 0.6);
        window.press("secondary-shift-t", cx);
        assert_eq!(
            view.read(cx).books[0].annotations.history.items.last(),
            Some(&annotation)
        );
        window.press("secondary-z", cx);
        assert!(view.read(cx).books[0].annotations.history.items.is_empty());
    })
    .unwrap();
}

#[gpui_kit::test]
fn keyboard_activation_opens_settings_and_escape_dismisses(cx: &mut TestAppContext) {
    let (handle, view) = crate::ui_tests::app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| view.books[0].state.mode = Mode::Annotate);
        window.render_frame(cx);
        for _ in 0..50 {
            if window.find(("tool", 4usize)).focused() == Some(true) {
                break;
            }
            window.press("tab", cx);
        }
        assert_eq!(window.find(("tool", 4usize)).focused(), Some(true));
        window.press("enter", cx);
        assert_eq!(
            view.read(cx).tool_settings.keyboard_open,
            Some(ToolKey {
                mode: Mode::Annotate,
                index: 4
            })
        );
    })
    .unwrap();
    advance(cx, handle, 200);
    cx.update_window(handle, |_, window, cx| {
        assert!(window.find("tool-settings-content").visible());
        window.press("escape", cx);
        assert!(view.read(cx).tool_settings.keyboard_open.is_none());
        assert!(window.try_find("tool-settings-content").is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn note_and_edit_settings_are_reachable_without_a_sidebar(cx: &mut TestAppContext) {
    let (handle, view) = crate::ui_tests::app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| view.books[0].state.mode = Mode::Annotate);
        window.render_frame(cx);
        window.hover(("tool", 6usize), cx);
    })
    .unwrap();
    advance(cx, handle, 400);
    cx.update_window(handle, |_, window, cx| {
        assert!(window.find("tool-settings-content").visible());
        assert!(window.try_find(("ink-width", 2usize)).is_none());
        assert!(!view.read(cx).document_panel.open);
        view.read(cx)
            .text_input
            .clone()
            .update(cx, |input, cx| input.focus(window, cx));
        window.press("secondary-a", cx);
        window.input("Review note", cx);
        assert_eq!(
            view.read(cx).text_input.read(cx).value().as_ref(),
            "Review note"
        );
        window.press("secondary-3", cx);
        window.render_frame(cx);
        window.hover(("tool", 0usize), cx);
    })
    .unwrap();
    advance(cx, handle, 400);
    cx.update_window(handle, |_, window, cx| {
        assert!(window.find("tool-settings-content").visible());
        window.click("font-larger", cx);
        window.click("bold", cx);
        window.click(("tool-color", 7usize), cx);
        assert_eq!(view.read(cx).tool_settings.text[0].size, 20);
        assert!(view.read(cx).tool_settings.text[0].bold);
        assert_eq!(
            view.read(cx).tool_settings.text[0].color,
            CategoryColor::Pink
        );
        assert!(view.read(cx).books[0].annotations.history.items.is_empty());
        assert!(!view.read(cx).document_panel.open);
    })
    .unwrap();
}
