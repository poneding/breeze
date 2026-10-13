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
        let width_input = view
            .read(cx)
            .tool_settings
            .controls
            .as_ref()
            .unwrap()
            .width
            .clone();
        width_input.update(cx, |input, cx| {
            input.focus(window, cx);
            input.select_all(window, cx);
        });
        window.input("8", cx);
        window.press("up", cx);
        let slider = view
            .read(cx)
            .tool_settings
            .controls
            .as_ref()
            .unwrap()
            .opacity
            .clone();
        let bounds = window.find(("slider", slider.entity_id())).bounds();
        window.click_at(
            ("slider", slider.entity_id()),
            point(bounds.size.width * 0.6, bounds.size.height / 2.),
            cx,
        );
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let settings = &view.read(cx).tool_settings;
        assert_eq!(
            settings.annotation[4],
            AnnotationStyle {
                color: CategoryColor::Blue,
                width: 8.25,
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
        assert_eq!(annotation.width, 8.25);
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
            Some(ToolKey { index: 4 })
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
fn note_settings_are_reachable_without_a_sidebar(cx: &mut TestAppContext) {
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
        assert!(window.try_find("ink-width").is_none());
        assert!(view.read(cx).books[0].annotations.history.items.is_empty());
        assert!(!view.read(cx).document_panel.open);
    })
    .unwrap();
}

#[gpui_kit::test]
fn numeric_controls_reject_invalid_values_and_discard_old_tool_events(cx: &mut TestAppContext) {
    let (handle, view) = crate::ui_tests::app(cx);
    let width = cx
        .update_window(handle, |_, window, cx| {
            view.update(cx, |view, cx| {
                view.prepare_tool_controls(ToolKey { index: 4 }, window, cx);
                view.tool_settings.controls.as_ref().unwrap().width.clone()
            })
        })
        .unwrap();
    for value in ["NaN", "inf", "0", "101", "-"] {
        cx.update_window(handle, |_, window, cx| {
            width.update(cx, |input, cx| input.set_value(value, window, cx));
        })
        .unwrap();
        assert_eq!(
            view.read_with(cx, |view, _| view.tool_settings.annotation[4].width),
            2.
        );
    }
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.prepare_tool_controls(ToolKey { index: 9 }, window, cx)
        });
        width.update(cx, |input, cx| input.set_value("7", window, cx));
    })
    .unwrap();
    view.read_with(cx, |view, _| {
        assert_eq!(view.tool_settings.annotation[4].width, 2.);
        assert_eq!(view.tool_settings.annotation[9].width, 2.);
    });
}
