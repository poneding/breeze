use super::*;
use gpui_kit::test::TestAppContextExt as _;
use gpui_kit::test::{TestQueryExt, TestWindowExt};

#[gpui_kit::test]
async fn markdown_export_results_use_dialogs_and_leave_reading_status_clear(
    cx: &mut TestAppContext,
) {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.md");
    let saved = "# Saved\n\nBody 中文\n";
    std::fs::write(&source, saved).unwrap();
    let (handle, view) = crate::ui_tests::app(cx);
    cx.update(|cx| cx.set_reduce_motion(true));
    cx.update_window(handle, |_, _, cx| {
        view.update(cx, |view, cx| {
            view.books[0].source = Source::File(source.clone());
            view.load(cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    for format in [ExportFormat::Word, ExportFormat::Pdf] {
        let destination = directory
            .path()
            .join(format!("result.{}", format.extension()));
        for success in [true, false] {
            cx.update_window(handle, |_, _, cx| {
                view.update(cx, |view, cx| {
                    let mut request = view.conversion_snapshot(format, cx).unwrap();
                    request.destination = destination.clone();
                    view.start_document_export(request, cx);
                });
            })
            .unwrap();
            cx.wait_for(handle, std::time::Duration::from_secs(10), |_, cx| {
                !view.read(cx).export_busy
            })
            .await;
            cx.update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                let result = window.find("export-result");
                assert!(result.visible());
                let message = result.label().unwrap();
                assert!(message.contains(if success { "已导出" } else { "导出失败" }));
                assert!(message.contains(&destination.display().to_string()));
                assert!(view.read(cx).export_notice.is_none());
                assert!(window.try_find("reflow-export-notice").is_none());
                assert!(view.read(cx).can_convert_document());
                window.press("escape", cx);
                window.render_frame(cx);
                assert!(window.try_find("export-result").is_none());
            })
            .unwrap();
            assert_eq!(std::fs::read_to_string(&source).unwrap(), saved);
        }
    }
}

#[gpui_kit::test]
fn content_favorite_toggle_and_toolbar_more_share_the_current_document(cx: &mut TestAppContext) {
    let (handle, view) = crate::ui_tests::app(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let toolbar = window.find("reader-toolbar").bounds();
        let favorite = window.find("reader-star").bounds();
        let more = window.find("reader-more").bounds();
        assert_eq!(toolbar.size.height, px(style::TOOLBAR_HEIGHT));
        assert!(favorite.right() <= more.left());
        window.right_click("document-viewport", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.within("popup-menu").click(8usize, cx);
        assert!(view.read(cx).books[0].favorite);
        window.click("reader-more", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let mut menu = window.within("popup-menu");
        assert!(menu.find_by_label(i18n::text("Export")).visible());
        menu.hover(0usize, cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        for label in [
            "Markdown document (.md)",
            "Word document (.docx)",
            "Export annotated PDF",
        ] {
            assert!(window.find_by_label(i18n::text(label)).visible());
        }
    })
    .unwrap();
}

#[gpui_kit::test]
fn markdown_more_offers_pdf_word_and_captures_the_draft(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("menu.md");
    std::fs::write(&path, "# Saved\n\nBody\n").unwrap();
    let (handle, view) = crate::ui_tests::app(cx);
    cx.update_window(handle, |_, _, cx| {
        view.update(cx, |view, cx| {
            view.books[0].source = Source::File(path);
            view.books[0].title = "menu.md".into();
            view.load(cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("reader-more", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.within("popup-menu").hover(0usize, cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        for label in ["PDF document (.pdf)", "Word document (.docx)"] {
            assert!(window.find_by_label(i18n::text(label)).visible());
        }
        assert!(
            window
                .try_find_by_label(i18n::text("Markdown document (.md)"))
                .is_none()
        );
        view.update(cx, |view, cx| {
            assert!(view.conversion_snapshot(ExportFormat::Word, cx).is_some());
            assert!(
                view.conversion_snapshot(ExportFormat::Markdown, cx)
                    .is_none()
            );
            view.export_busy = true;
            assert!(view.conversion_snapshot(ExportFormat::Word, cx).is_none());
            view.export_busy = false;
        });
        window.press("escape", cx);
        window.within("markdown-modes").click(1usize, cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press("ctrl-end", cx);
        window.input("\nUnsaved export draft", cx);
        let editor = view.read(cx).books[0].markdown_edit.clone().unwrap();
        let text = editor.read(cx).source_text(cx);
        assert!(text.contains("Unsaved export draft"));
        assert!(editor.read(cx).dirty(cx));
        let request = view.read(cx).conversion_snapshot(ExportFormat::Word, cx).unwrap();
        assert!(matches!(request.snapshot, DocumentSnapshot::Markdown { text: snapshot, .. } if snapshot == text));
        assert!(editor.read(cx).dirty(cx));
    }).unwrap();
}
