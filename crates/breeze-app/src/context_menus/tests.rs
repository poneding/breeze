use super::*;
use gpui_kit::test::TestWindowExt;

#[gpui_kit::test]
fn tab_menu_reveals_a_file_action_and_closes_the_clicked_tab(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("first.pdf");
    std::fs::write(&path, b"not loaded by this menu test").unwrap();
    let (handle, view) = crate::ui_tests::app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.books[0].source = Source::File(path);
            view.books[0].title = "first.pdf".into();
            view.books.push(Book {
                source: Source::Sample,
                title: "second.pdf".into(),
                favorite: false,
                categories: Vec::new(),
                state: ReaderState::default(),
                reflow: ReflowState::default(),
                opened: false,
                last_opened: 0,
                annotations: DocumentAnnotations::default(),
                markdown_edit: None,
                markdown_editing: false,
            });
            view.open_book(1, cx);
        });
        window.render_frame(cx);
        window.right_click(("document", 0usize), cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let mut menu = window.within("popup-menu");
        assert_ne!(
            menu.find_by_label(i18n::text(reveal::label())).disabled(),
            Some(true)
        );
        assert!(menu.find_by_label(i18n::text("Close tab")).visible());
        menu.click(1usize, cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(view.read(cx).screen, Screen::Reader);
        assert_eq!(view.read(cx).active, 1);
        assert!(!view.read(cx).books[0].opened);
        assert!(view.read(cx).books[1].opened);
    })
    .unwrap();
}

#[gpui_kit::test]
fn pdf_content_menu_exposes_status_commands_and_runs_them(cx: &mut TestAppContext) {
    let (handle, view) = crate::ui_tests::app(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.right_click("document-viewport", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let mut menu = window.within("popup-menu");
        for label in [
            "Go to page",
            "Rotate 90° left",
            "Rotate 90° right",
            "Change reading layout",
            "Zoom options",
            "Fullscreen reading",
            "Show status bar",
        ] {
            assert!(
                menu.find_by_label(i18n::text(label)).visible(),
                "the PDF content menu omitted {label:?}"
            );
        }
        menu.click(0usize, cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(view.read(cx).page_editing);
    })
    .unwrap();
}

#[gpui_kit::test]
fn status_bar_toggle_hides_and_restores_the_pdf_status_bar(cx: &mut TestAppContext) {
    let (handle, view) = crate::ui_tests::app(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("status-scroll").visible());
        window.right_click("document-viewport", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let mut menu = window.within("popup-menu");
        assert!(menu.find_by_label(i18n::text("Show status bar")).visible());
        menu.click(8usize, cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(!view.read(cx).status_bar_visible);
        assert!(!view.read(cx).snapshot().show_status_bar);
        assert!(window.try_find("status-scroll").is_none());
        window.right_click("document-viewport", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let mut menu = window.within("popup-menu");
        assert!(menu.find_by_label(i18n::text("Show status bar")).visible());
        menu.click(8usize, cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(view.read(cx).status_bar_visible);
        assert!(window.find("status-scroll").visible());
    })
    .unwrap();
}

#[gpui_kit::test]
fn status_bar_toggle_hides_and_restores_the_markdown_status_bar(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("status-bar.md");
    std::fs::write(&path, "# Status bar\n\nBody\n").unwrap();
    let (handle, view) = crate::ui_tests::app(cx);
    cx.update_window(handle, |_, _, cx| {
        view.update(cx, |view, cx| {
            view.books[0].source = Source::File(path);
            view.books[0].title = "status-bar.md".into();
            view.load(cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("reflow-status").visible());
        window.right_click("reflow-viewport", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let mut menu = window.within("popup-menu");
        assert!(menu.find_by_label(i18n::text("Show status bar")).visible());
        menu.click(7usize, cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(!view.read(cx).status_bar_visible);
        assert!(window.try_find("reflow-status").is_none());
        window.right_click("reflow-viewport", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let mut menu = window.within("popup-menu");
        assert!(menu.find_by_label(i18n::text("Show status bar")).visible());
        menu.click(7usize, cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(view.read(cx).status_bar_visible);
        assert!(window.find("reflow-status").visible());
    })
    .unwrap();
}

#[gpui_kit::test]
fn markdown_content_menu_duplicates_status_controls(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("context-menu.md");
    std::fs::write(&path, "# Context menu\n\nBody\n").unwrap();
    let (handle, view) = crate::ui_tests::app(cx);
    cx.update_window(handle, |_, _, cx| {
        view.update(cx, |view, cx| {
            view.books[0].source = Source::File(path);
            view.books[0].title = "context-menu.md".into();
            view.load(cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.right_click("reflow-viewport", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let mut menu = window.within("popup-menu");
        for label in [
            "Use the full window width",
            "Decrease text size",
            "Increase text size",
            "Fullscreen reading",
            "Show status bar",
        ] {
            assert!(
                menu.find_by_label(i18n::text(label)).visible(),
                "the Markdown content menu omitted {label:?}"
            );
        }
        menu.click(0usize, cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(view.read(cx).books[0].reflow.full_width);
    })
    .unwrap();
}
