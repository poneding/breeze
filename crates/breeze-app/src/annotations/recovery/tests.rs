use super::*;
use gpui_kit::test::{TestAppContextExt, TestWindowExt};

fn conflicted(
    cx: &mut TestAppContext,
    path: &std::path::Path,
) -> (AnyWindowHandle, Entity<Breeze>) {
    let (handle, view) = super::super::tests::saved_document(cx, path);
    view.update(cx, |view, _| {
        let data = &mut view.books[0].annotations;
        data.history.add(super::super::tests::note("未保存🙂"));
        data.source_conflict = true;
        data.failure = Some(AnnotationFailure::SourceConflict);
        data.error = Some("Source conflict".into());
    });
    (handle, view)
}

#[gpui_kit::test]
async fn recovery_copy_preserves_old_fingerprint_and_never_acknowledges_normal_save(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let original = root.path().join("original.json");
    let destination = root.path().join("recovery.json");
    let (handle, view) = conflicted(cx, &original);
    let before = std::fs::read(&original).unwrap();
    let (hash, items) = view.update(cx, |view, cx| {
        view.choose_annotation_recovery(cx);
        assert!(!view.allow_close(cx));
        let data = &view.books[0].annotations;
        (data.fingerprint.clone(), data.history.items.clone())
    });
    cx.simulate_new_path_selection(|_| Some(destination.clone()));
    cx.wait_for(handle, std::time::Duration::from_secs(5), |_, cx| {
        !view.read(cx).books[0].annotations.saving
    })
    .await;
    view.update(cx, |view, cx| {
        let data = &view.books[0].annotations;
        assert_eq!(data.recovery_path.as_ref(), Some(&destination));
        assert_eq!(data.path.as_ref(), Some(&original));
        assert_eq!(data.saved.len(), 1);
        assert_eq!(data.history.items, items);
        assert!(data.source_conflict && data.dirty());
        assert!(!view.allow_close(cx));
        assert!(view.document_panel.open);
    });
    assert_eq!(
        breeze_storage::load_annotations(&destination, &hash)
            .unwrap()
            .annotations,
        items
    );
    assert!(breeze_storage::load_annotations(&destination, "new-source").is_err());
    assert_eq!(std::fs::read(&original).unwrap(), before);
}

#[gpui_kit::test]
async fn recovery_cancellation_and_failure_preserve_every_annotation(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let original = root.path().join("original.json");
    let occupied = root.path().join("occupied.json");
    std::fs::write(&occupied, b"occupied").unwrap();
    let (handle, view) = conflicted(cx, &original);
    let before = view.read_with(cx, |view, _| {
        view.books[0].annotations.history.items.clone()
    });
    for destination in [
        None,
        Some(occupied.clone()),
        Some(root.path().join("missing/new.json")),
    ] {
        view.update(cx, |view, cx| view.choose_annotation_recovery(cx));
        cx.simulate_new_path_selection(|_| destination);
        cx.wait_for(handle, std::time::Duration::from_secs(5), |_, cx| {
            !view.read(cx).books[0].annotations.saving
        })
        .await;
        view.read_with(cx, |view, _| {
            let data = &view.books[0].annotations;
            assert_eq!(data.history.items, before);
            assert!(data.dirty() && data.source_conflict);
            assert!(data.recovery_path.is_none());
            assert_eq!(data.path.as_ref(), Some(&original));
        });
    }
    assert_eq!(std::fs::read(&occupied).unwrap(), b"occupied");
}

#[gpui_kit::test]
async fn discard_requires_confirmation_and_cannot_discard_changes_created_after_prompt(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let original = root.path().join("original.json");
    let (handle, view) = conflicted(cx, &original);
    let bytes = std::fs::read(&original).unwrap();
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, cx| view.confirm_discard_annotations(window, cx));
    })
    .unwrap();
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("取消");
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.books[0].annotations.dirty()));
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.confirm_discard_annotations(window, cx);
            view.books[0]
                .annotations
                .history
                .add(super::super::tests::note("Later"));
        });
    })
    .unwrap();
    cx.simulate_prompt_answer("放弃更改");
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, _| view.books[0].annotations.history.items.len()),
        3
    );
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, cx| view.confirm_discard_annotations(window, cx));
    })
    .unwrap();
    cx.simulate_prompt_answer("放弃更改");
    cx.run_until_parked();
    view.update(cx, |view, cx| {
        assert!(!view.books[0].annotations.dirty());
        assert!(!view.books[0].annotations.history.can_undo());
        assert!(!view.books[0].annotations.history.can_redo());
        assert!(view.books[0].annotations.source_conflict);
        assert!(view.allow_close(cx));
    });
    assert_eq!(std::fs::read(&original).unwrap(), bytes);
}

fn current_source(
    root: &std::path::Path,
    view: &Entity<Breeze>,
    cx: &mut TestAppContext,
) -> (std::path::PathBuf, String) {
    let path = root.join("current.pdf");
    std::fs::write(&path, b"Current source fixture").unwrap();
    let hash = breeze_storage::fingerprint(&path).unwrap();
    view.update(cx, |view, _| {
        view.books[0].source = Source::File(path.clone());
        view.reading.source = Some(Source::File(path.clone()));
        view.reading.fingerprint = hash.clone();
        view.reading.sizes = Arc::new(vec![(612., 792.); 8]);
    });
    (path, hash)
}

#[gpui_kit::test]
async fn explicit_reassociation_writes_new_sidecar_and_rechecks_source(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let original = root.path().join("original.json");
    let destination = root.path().join("new.json");
    let (handle, view) = conflicted(cx, &original);
    let bytes = std::fs::read(&original).unwrap();
    let (source, hash) = current_source(root.path(), &view, cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.confirm_annotation_reassociation(window, cx)
        });
    })
    .unwrap();
    cx.simulate_prompt_answer("取消");
    cx.run_until_parked();
    assert!(!cx.did_prompt_for_new_path());
    assert!(view.read_with(cx, |view, _| view.books[0].annotations.source_conflict));
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.confirm_annotation_reassociation(window, cx)
        });
    })
    .unwrap();
    cx.simulate_prompt_answer("确认重新关联");
    cx.run_until_parked();
    assert!(cx.did_prompt_for_new_path());
    cx.simulate_new_path_selection(|_| Some(destination.clone()));
    cx.wait_for(handle, std::time::Duration::from_secs(5), |_, cx| {
        !view.read(cx).books[0].annotations.saving
    })
    .await;
    view.update(cx, |view, cx| {
        let data = &view.books[0].annotations;
        assert!(!data.source_conflict && !data.dirty());
        assert!(data.error.is_none());
        assert_eq!(data.fingerprint, hash);
        assert_eq!(data.path.as_ref(), Some(&destination));
        assert_eq!(
            breeze_storage::load_annotations(&destination, &hash)
                .unwrap()
                .annotations,
            data.history.items
        );
        assert!(view.allow_close(cx));
    });
    assert_eq!(std::fs::read(&original).unwrap(), bytes);
    assert_eq!(std::fs::read(source).unwrap(), b"Current source fixture");
}

#[gpui_kit::test]
async fn reassociation_refuses_source_change_after_confirmation(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let original = root.path().join("original.json");
    let destination = root.path().join("new.json");
    let (handle, view) = conflicted(cx, &original);
    let (source, _) = current_source(root.path(), &view, cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.confirm_annotation_reassociation(window, cx)
        });
    })
    .unwrap();
    cx.simulate_prompt_answer("确认重新关联");
    cx.run_until_parked();
    std::fs::write(&source, b"Source changed again").unwrap();
    cx.simulate_new_path_selection(|_| Some(destination.clone()));
    cx.wait_for(handle, std::time::Duration::from_secs(5), |_, cx| {
        !view.read(cx).books[0].annotations.saving
    })
    .await;
    view.read_with(cx, |view, _| {
        let data = &view.books[0].annotations;
        assert!(data.dirty() && data.source_conflict);
        assert_eq!(data.path.as_ref(), Some(&original));
    });
    assert!(!destination.exists());
}

#[gpui_kit::test]
fn mode_shortcuts_and_navigation_cancel_in_progress_gestures(cx: &mut TestAppContext) {
    let (handle, view) = crate::ui_tests::app(cx);
    let worker = PdfWorker::new().unwrap();
    let page = futures::executor::block_on(worker.request_metadata(Source::Sample, 0).unwrap())
        .unwrap()
        .unwrap();
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| {
            view.preview = Some(Preview::from_page(page).unwrap());
            view.books[0].annotations.ready = true;
            view.gesture = vec![DocPoint::new(10., 10.), DocPoint::new(20., 20.)];
        });
        window.render_frame(cx);
        window.press("secondary-2", cx);
        assert!(view.read(cx).gesture.is_empty());
        assert!(view.read(cx).books[0].annotations.history.items.is_empty());
        view.update(cx, |view, cx| {
            view.gesture = vec![DocPoint::new(10., 10.), DocPoint::new(20., 20.)];
            assert!(view.annotation_navigation_allowed(cx));
            assert!(view.gesture.is_empty());
            view.pointer_up(&MouseUpEvent::default(), window, cx);
            assert!(view.books[0].annotations.history.items.is_empty());
        });
    })
    .unwrap();
}

#[gpui_kit::test]
async fn stale_recovery_picker_cannot_attach_to_same_path_and_fingerprint_after_reload(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let original = root.path().join("original.json");
    let destination = root.path().join("stale.json");
    let (handle, view) = conflicted(cx, &original);
    view.update(cx, |view, cx| {
        view.choose_annotation_recovery(cx);
        let fingerprint = view.books[0].annotations.fingerprint.clone();
        view.books[0].annotations = DocumentAnnotations {
            path: Some(original.clone()),
            fingerprint,
            ready: true,
            ..Default::default()
        };
    });
    cx.simulate_new_path_selection(|_| Some(destination.clone()));
    cx.run_until_parked();
    cx.update_window(handle, |_, _, cx| {
        let data = &view.read(cx).books[0].annotations;
        assert!(!data.saving);
        assert!(!data.dirty());
        assert!(data.recovery_path.is_none());
        assert_eq!(data.path.as_ref(), Some(&original));
    })
    .unwrap();
    assert!(!destination.exists());
}

#[gpui_kit::test]
fn conflict_controls_and_reassociation_page_limits_work(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (handle, view) = conflicted(cx, &root.path().join("original.json"));
    current_source(root.path(), &view, cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| view.document_panel.show_annotations());
        window.render_frame(cx);
        assert!(window.find("recover-annotations").visible());
        assert!(window.find("reassociate-annotations").visible());
        assert!(window.find("discard-annotation-changes").visible());
        view.update(cx, |view, _| {
            view.books[0].annotations.history.items[0].page = 99
        });
        window.click("reassociate-annotations", cx);
        assert!(!view.read(cx).books[0].annotations.saving);
        assert!(view.read(cx).message.contains("不存在的页码"));
    })
    .unwrap();
    assert!(!cx.has_pending_prompt());
    assert!(!cx.did_prompt_for_new_path());
}
