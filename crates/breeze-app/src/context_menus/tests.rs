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

/// A committed reading selection leads the content menu with Copy, and the
/// item copies exactly the text the drag selected.
#[gpui_kit::test]
fn reading_selection_leads_the_content_menu_with_copy(cx: &mut TestAppContext) {
    let (handle, view) = crate::ui_tests::spread_pages(cx);
    let selected = cx
        .update_window(handle, |_, window, cx| {
            let left = window
                .within(("page-frame", 0usize))
                .find("page-surface")
                .bounds();
            window.drag(
                left.origin + point(px(40.), px(300.)),
                left.origin + point(px(320.), px(360.)),
                cx,
            );
            let selected = view.read(cx).selected_text.clone();
            assert!(!selected.is_empty(), "the drag must select text");
            window
                .within(("page-frame", 0usize))
                .right_click("page-surface", cx);
            selected
        })
        .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let mut menu = window.within("popup-menu");
        assert!(
            menu.find_by_label(i18n::text("Copy")).visible(),
            "the selected text must lead the menu with Copy"
        );
        menu.click(0usize, cx);
    })
    .unwrap();
    cx.update_window(handle, |_, _, cx| {
        assert_eq!(
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .as_deref(),
            Some(selected.as_str()),
            "Copy must publish the committed selection"
        );
    })
    .unwrap();
}

/// The Markdown draft's own menu offers Paste, which writes the clipboard into
/// the focused source editor.
#[gpui_kit::test]
fn markdown_draft_menu_offers_paste(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("paste.md");
    std::fs::write(&path, "# Paste\n\nBody\n").unwrap();
    let (handle, view) = crate::ui_tests::app(cx);
    cx.update_window(handle, |_, _, cx| {
        view.update(cx, |view, cx| {
            view.books[0].source = Source::File(path);
            view.books[0].title = "paste.md".into();
            view.load(cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.within("markdown-modes").click(1usize, cx);
        window.render_frame(cx);
        cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("pasted text".into()));
        window.right_click("markdown-editor", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let mut menu = window.within("popup-menu");
        assert!(
            menu.find_by_label(i18n::text("Paste")).visible(),
            "the draft menu must offer Paste"
        );
        menu.click(0usize, cx);
    })
    .unwrap();
    cx.update_window(handle, |_, _, cx| {
        let draft = view.read(cx).books[0]
            .markdown_edit
            .clone()
            .expect("edit mode keeps the source session");
        assert!(
            draft.read(cx).source_text(cx).contains("pasted text"),
            "Paste must reach the source editor"
        );
    })
    .unwrap();
}
