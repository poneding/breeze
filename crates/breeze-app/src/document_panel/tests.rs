use super::*;
use breeze_core::annotations::Point as DocPoint;
use gpui_kit::test::TestWindowExt;

fn annotation(page: usize, kind: Kind) -> Annotation {
    Annotation {
        page,
        kind,
        points: vec![DocPoint::new(20., 20.), DocPoint::new(80., 80.)],
        quads: Vec::new(),
        color: colors::components(breeze_core::library::CategoryColor::Red),
        opacity: 1.,
        width: 2.,
        text: if kind == Kind::Note {
            "Review note".into()
        } else {
            String::new()
        },
    }
}

#[test]
fn annotation_filters_preserve_original_indices_and_page_order() {
    let items = vec![
        annotation(4, Kind::Note),
        annotation(0, Kind::Ink),
        annotation(2, Kind::Note),
        annotation(2, Kind::Rectangle),
    ];
    assert_eq!(filtered_indices(&items, None, None), vec![1, 2, 3, 0]);
    assert_eq!(filtered_indices(&items, Some(Kind::Note), None), vec![2, 0]);
    assert_eq!(filtered_indices(&items, None, Some(2)), vec![2, 3]);
    assert_eq!(filtered_indices(&items, Some(Kind::Note), Some(2)), vec![2]);
}

#[gpui_kit::test]
fn toolbar_sidebar_and_filtered_annotation_actions_work(cx: &mut TestAppContext) {
    let worker = PdfWorker::new().unwrap();
    let page = futures::executor::block_on(worker.request(Source::Sample, 0).unwrap())
        .unwrap()
        .unwrap();
    let fingerprint = page.fingerprint.clone();
    let preview = Preview::from_page(page).unwrap();
    let (handle, view) = crate::ui_tests::app(cx);
    let items = vec![
        annotation(3, Kind::Note),
        annotation(0, Kind::Ink),
        annotation(1, Kind::Note),
    ];
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| {
            view.preview = Some(preview.clone());
            view.reading.sizes = Arc::new(vec![preview.points; 8]);
            view.reading.source = Some(Source::Sample);
            view.reading.fingerprint = fingerprint.clone();
            view.books[0].annotations.fingerprint = fingerprint.clone();
            view.books[0].annotations.ready = true;
            view.books[0].annotations.history.items = items.clone();
            view.books[0].annotations.saved = items.clone();
        });
        window.render_frame(cx);
        assert!(window.try_find("document-panel").is_none());
        let search = window.find("reader-search").bounds();
        let favorite = window.find("reader-star").bounds();
        let sidebar = window.find("document-panel-toggle").bounds();
        assert!(search.right() <= favorite.left() && favorite.right() <= sidebar.left());
        assert!(window.within("tools").try_find(("tool", 1usize)).is_none());
        window.click("document-panel-toggle", cx);
        assert!(window.find("document-information").visible());
        assert_eq!(window.find("reader-toolbar").bounds().size.height, px(34.));
        assert_eq!(
            window.find("document-panel-header").bounds().size.height,
            px(34.)
        );
        assert_eq!(
            window.find("document-panel-header").bounds().origin.y,
            window.find("reader-toolbar").bounds().origin.y
        );
        let information = window.find("document-panel-information").bounds();
        let annotations = window.find("document-panel-annotations").bounds();
        assert!(annotations.origin.x - information.right() >= px(4.));
        assert!(window.find("document-panel").bounds().right() <= window.viewport_size().width);
        window.click("document-panel-annotations", cx);
        view.update(cx, |view, cx| {
            view.books[0].state.mode = Mode::Annotate;
            cx.notify();
        });
        window.render_frame(cx);
        assert_eq!(view.read(cx).document_panel.tab, 1);
        let filter_row = window.find("annotation-filter-row").bounds();
        let kind_filter = window.find("annotation-kind-filter").bounds();
        let current_page = window.find("annotations-current-page").bounds();
        assert!((kind_filter.center().y - current_page.center().y).abs() <= px(1.));
        assert!(kind_filter.right() < current_page.left());
        assert!(current_page.right() <= filter_row.right());
        assert!(filter_row.right() - current_page.right() <= px(1.));
        assert!(window.try_find("annotations-current-page-label").is_none());
        assert!(window.within("reader-toolbar").find("undo").visible());
        assert!(window.within("reader-toolbar").find("redo").visible());
        assert!(
            window
                .within("reader-toolbar")
                .find("save-sidecar")
                .visible()
        );
        assert!(
            window
                .within("reader-toolbar")
                .try_find("save-sidecar-as")
                .is_none()
        );
        assert!(window.within("document-panel").try_find("undo").is_none());
        assert!(
            window
                .within("document-panel")
                .try_find("save-sidecar")
                .is_none()
        );
        assert!(
            window
                .within("document-panel")
                .try_find("export-pdf")
                .is_none()
        );
        assert!(window.find(("annotation-item", 0usize)).visible());
        assert!(window.find(("annotation-item", 1usize)).visible());
        window.click("annotation-kind-filter", cx);
        for _ in 0..7 {
            window.press("down", cx);
        }
        window.press("enter", cx);
        assert_eq!(view.read(cx).document_panel.kind, Some(Kind::Note));
        assert!(window.try_find(("annotation-item", 1usize)).is_none());
    })
    .unwrap();
    // The closing menu retains its surface until the native transition finishes.
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(400));
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("annotations-current-page", cx);
        assert!(window.find("annotations-empty").visible());
        window.click("annotations-current-page", cx);
        window.click(("remove-annotation", 2usize), cx);
        assert_eq!(view.read(cx).books[0].annotations.history.items, items[..2]);
        window.click("undo", cx);
        assert_eq!(view.read(cx).books[0].annotations.history.items, items);
        assert_ne!(
            window.find(("annotation-item", 0usize)).disabled(),
            Some(true)
        );
        let generation = view.read(cx).generation;
        window.click(("annotation-item", 0usize), cx);
        assert!(
            view.read(cx).generation > generation,
            "The annotation row must dispatch navigation"
        );
        assert_eq!(view.read(cx).books[0].state.navigation.page(), 3);
        window.click("document-panel-toggle", cx);
        assert!(window.try_find("document-panel").is_none());
        window.click("reader-search", cx);
        assert!(view.read(cx).search_open);
        assert!(view.read(cx).document_panel.open);
        assert!(window.find("search-results").visible());
        assert_eq!(
            window.find("document-panel").bounds().origin.y,
            window.find("reader-toolbar").bounds().origin.y
        );
        assert!(
            view.read(cx)
                .search_input
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
        );
        assert!(window.find("document-panel-search-toolbar").visible());
        assert!(window.try_find("document-panel-information").is_none());
        let search_toolbar = window.find("document-panel-search-toolbar").bounds();
        let toggle = window.find("document-panel-toggle").bounds();
        let panel = window.find("document-panel").bounds();
        assert!(search_toolbar.right() <= toggle.left());
        // The toggle is pinned to the header's right edge, within fractional
        // control rounding of the Small icon button.
        assert!(toggle.right() <= panel.right() - px(7.));
        assert!(search_toolbar.size.width >= px(200.));
        window.press("escape", cx);
        assert!(!view.read(cx).search_open);
        assert!(view.read(cx).search_notice.is_empty());
        assert!(!view.read(cx).document_panel.open);
        assert!(window.try_find("document-panel").is_none());
        window.click("document-panel-toggle", cx);
        window.click("reader-search", cx);
        assert!(view.read(cx).search_open);
        window.press("escape", cx);
        assert!(!view.read(cx).search_open);
        assert!(view.read(cx).document_panel.open);
        assert!(window.find("document-panel-information").visible());
    })
    .unwrap();
}

#[gpui_kit::test]
fn locations_stay_on_one_line_and_expose_folder_actions(cx: &mut TestAppContext) {
    let (handle, view) = crate::ui_tests::app(cx);
    let directory = tempfile::tempdir().unwrap();
    let source = directory
        .path()
        .join("a very long directory name that should be truncated from the start")
        .join("Field notes.pdf");
    let sidecar = source.with_extension("pdf.breeze-annotations.json");

    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.books[0].source = Source::File(source.clone());
            view.books[0].annotations.path = Some(sidecar.clone());
            view.document_panel.open = true;
            view.document_panel.tab = 0;
            cx.notify();
        });
        window.render_frame(cx);

        let source_location = window.find("document-file-location");
        let source_reveal = window.find("reveal-document");
        let annotation_location = window.find("annotation-location");
        let annotation_reveal = window.find("reveal-annotation-location");
        assert!(source_location.visible());
        assert!(source_reveal.visible());
        assert!(annotation_location.visible());
        assert!(annotation_reveal.visible());
        assert!(source_location.bounds().size.height <= px(20.));
        assert!(annotation_location.bounds().size.height <= px(20.));
        assert!(source_reveal.bounds().left() >= source_location.bounds().right());
        assert!(annotation_reveal.bounds().left() >= annotation_location.bounds().right());
        assert!(
            (source_reveal.bounds().center().y - source_location.bounds().center().y).abs()
                <= px(1.)
        );
        assert!(
            (annotation_reveal.bounds().center().y - annotation_location.bounds().center().y).abs()
                <= px(1.)
        );
        assert_eq!(
            source_reveal.label(),
            Some(i18n::text(crate::reveal::label()))
        );
        assert_eq!(
            annotation_reveal.label(),
            Some(i18n::text(crate::reveal::label()))
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn sidebar_is_independent_of_mode_and_fits_a_narrow_window(cx: &mut TestAppContext) {
    let (handle, view) = crate::ui_tests::app(cx);
    cx.update_window(handle, |_, window, cx| {
        window.resize(size(px(720.), px(580.)));
        window.render_frame(cx);
        window.click("document-panel-toggle", cx);
        for action in [&AnnotateMode as &dyn Action, &ReadMode] {
            view.update(cx, |view, cx| view.execute(action, window, cx));
            window.render_frame(cx);
            assert!(view.read(cx).document_panel.open);
            assert!(window.find("document-panel-toggle").visible());
            assert!(window.find("reader-search").visible());
            assert!(window.find("reader-star").visible());
            assert!(window.find("document-panel").bounds().right() <= window.viewport_size().width);
            assert!(view.read(cx).document_panel.open);
        }
        window.click("document-panel-toggle", cx);
        assert!(window.try_find("document-panel").is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn annotations_footer_only_reports_unfinished_work(cx: &mut TestAppContext) {
    let worker = PdfWorker::new().unwrap();
    let page = futures::executor::block_on(worker.request(Source::Sample, 0).unwrap())
        .unwrap()
        .unwrap();
    let fingerprint = page.fingerprint.clone();
    let preview = Preview::from_page(page).unwrap();
    let (handle, view) = crate::ui_tests::app(cx);
    let items = vec![annotation(0, Kind::Ink)];
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| {
            view.preview = Some(preview.clone());
            view.reading.sizes = Arc::new(vec![preview.points; 4]);
            view.reading.source = Some(Source::Sample);
            view.reading.fingerprint = fingerprint.clone();
            view.books[0].annotations.fingerprint = fingerprint.clone();
            view.books[0].annotations.ready = true;
            view.books[0].annotations.history.items = items.clone();
            view.books[0].annotations.saved = items.clone();
            view.document_panel.show_annotations();
        });
        window.render_frame(cx);
        // A loaded and saved sidecar is not news, so the footer stays empty.
        assert!(window.try_find("annotation-status").is_none());
        assert!(window.try_find("annotation-footer").is_none());
        view.update(cx, |view, cx| {
            view.books[0]
                .annotations
                .history
                .items
                .push(annotation(1, Kind::Ink));
            cx.notify();
        });
        window.render_frame(cx);
        // An unsaved change is: it explains why saving is still pending.
        assert!(window.find("annotation-status").visible());
        assert!(window.find("annotation-footer").visible());
        view.update(cx, |view, cx| {
            view.books[0].annotations.history.items = items.clone();
            view.books[0].annotations.ready = false;
            cx.notify();
        });
        window.render_frame(cx);
        // A sidecar that is not ready still says so.
        assert!(window.find("annotation-status").visible());
    })
    .unwrap();
}
