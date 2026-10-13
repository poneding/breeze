use super::*;
use gpui_kit::test::TestAppContextExt;

pub(super) fn note(text: &str) -> Annotation {
    Annotation {
        page: 0,
        kind: Kind::Note,
        points: vec![DocPoint::new(50., 50.)],
        quads: Vec::new(),
        color: [0., 0., 0.],
        opacity: 1.,
        width: 1.,
        text: text.into(),
    }
}

pub(super) fn saved_document(
    cx: &mut TestAppContext,
    path: &std::path::Path,
) -> (AnyWindowHandle, Entity<Breeze>) {
    let hash = breeze_storage::bytes_fingerprint(include_bytes!(
        "../../../../assets/samples/field-notes.pdf"
    ));
    let items = vec![note("Original")];
    let revision =
        breeze_storage::save_annotations(path, None, &hash, None, items.clone()).unwrap();
    let (handle, view) = crate::ui_tests::app(cx);
    view.update(cx, |view, _| {
        let data = &mut view.books[0].annotations;
        data.fingerprint = hash;
        data.path = Some(path.into());
        data.revision = Some(revision);
        data.ready = true;
        data.saved = items.clone();
        data.history.items = items;
    });
    (handle, view)
}

#[gpui_kit::test]
async fn save_as_failure_preserves_association_and_saved_baseline(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let original = root.path().join("original.breeze.json");
    let occupied = root.path().join("occupied.breeze.json");
    std::fs::write(&occupied, "existing destination").unwrap();
    let (handle, view) = saved_document(cx, &original);
    cx.update_window(handle, |_, window, cx| {
        use gpui_kit::test::TestWindowExt;
        window.render_frame(cx);
    })
    .unwrap();
    let before = view.read_with(cx, |view, _| view.snapshot());
    let original_bytes = std::fs::read(&original).unwrap();
    for destination in [occupied.clone(), root.path().join("missing/new.json")] {
        view.update(cx, |view, cx| view.save_annotations_as(cx));
        view.update(cx, |view, cx| {
            assert!(!view.allow_close(cx));
            assert_eq!(view.snapshot(), before);
        });
        cx.simulate_new_path_selection(|_| Some(destination));
        cx.wait_for(handle, std::time::Duration::from_secs(5), |_, cx| {
            !view.read(cx).books[0].annotations.saving
        })
        .await;
        view.update(cx, |view, cx| {
            assert_eq!(view.snapshot(), before);
            assert!(!view.books[0].annotations.dirty());
            assert!(view.books[0].annotations.error.is_none());
            assert!(view.message.contains("另存失败"));
            assert!(view.allow_close(cx));
        });
    }
    assert_eq!(std::fs::read(original).unwrap(), original_bytes);
    assert_eq!(
        std::fs::read_to_string(occupied).unwrap(),
        "existing destination"
    );
}

#[gpui_kit::test]
async fn transient_save_errors_retry_without_dropping_pending_annotations(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("sidecar.json");
    let (handle, view) = saved_document(cx, &path);
    view.update(cx, |view, cx| {
        view.books[0].annotations.history.add(note("Pending"));
        view.books[0].annotations.error = Some("Temporary I/O failure".into());
        view.books[0].annotations.failure = Some(AnnotationFailure::Io);
        view.retry_annotations(cx);
    });
    cx.wait_for(handle, std::time::Duration::from_secs(5), |_, cx| {
        !view.read(cx).books[0].annotations.saving
    })
    .await;
    view.update(cx, |view, cx| {
        let data = &view.books[0].annotations;
        assert!(!data.dirty());
        assert!(data.error.is_none());
        assert!(data.failure.is_none());
        assert_eq!(data.history.items.len(), 2);
        assert_eq!(
            breeze_storage::load_annotations(&path, &data.fingerprint)
                .unwrap()
                .annotations,
            data.history.items
        );
        assert!(view.allow_close(cx));
    });
}

#[gpui_kit::test]
async fn retry_refuses_external_sidecar_and_source_changes(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("sidecar.json");
    let (handle, view) = saved_document(cx, &path);
    let external = b"changed by another writer";
    std::fs::write(&path, external).unwrap();
    view.update(cx, |view, cx| {
        view.books[0].annotations.history.add(note("Pending"));
        view.retry_annotations(cx);
    });
    cx.wait_for(handle, std::time::Duration::from_secs(5), |_, cx| {
        !view.read(cx).books[0].annotations.saving
    })
    .await;
    view.update(cx, |view, cx| {
        assert_eq!(
            view.books[0].annotations.failure,
            Some(AnnotationFailure::SidecarConflict)
        );
        assert!(view.books[0].annotations.dirty());
        assert!(!view.allow_close(cx));
    });
    assert_eq!(std::fs::read(&path).unwrap(), external);
    let source = root.path().join("source.pdf");
    std::fs::write(&source, b"different PDF contents").unwrap();
    view.update(cx, |view, cx| {
        view.books[0].source = Source::File(source.clone());
        view.retry_annotations(cx);
    });
    cx.wait_for(handle, std::time::Duration::from_secs(5), |_, cx| {
        !view.read(cx).books[0].annotations.saving
    })
    .await;
    view.update(cx, |view, cx| {
        assert_eq!(
            view.books[0].annotations.failure,
            Some(AnnotationFailure::SourceConflict)
        );
        assert!(view.books[0].annotations.source_conflict);
        assert!(view.books[0].annotations.dirty());
        assert!(!view.can_export_pdf());
        assert!(!view.allow_close(cx));
    });
    assert_eq!(std::fs::read(&source).unwrap(), b"different PDF contents");
    assert_eq!(std::fs::read(&path).unwrap(), external);
}

#[gpui_kit::test]
async fn startup_source_conflict_retains_validated_history_without_applying_it(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("old.json");
    let (handle, view) = saved_document(cx, &path);
    let old_hash = view.read_with(cx, |view, _| view.books[0].annotations.fingerprint.clone());
    view.update(cx, |view, cx| {
        view.books[0].annotations = DocumentAnnotations {
            path: Some(path.clone()),
            ..Default::default()
        };
        view.prepare_annotations("current-source".into(), cx);
    });
    cx.wait_for(handle, std::time::Duration::from_secs(5), |_, cx| {
        view.read(cx).books[0].annotations.ready
    })
    .await;
    view.read_with(cx, |view, _| {
        let data = &view.books[0].annotations;
        assert!(data.source_conflict);
        assert_eq!(data.fingerprint, old_hash);
        assert_eq!(data.history.items, vec![note("Original")]);
        assert!(!view.can_export_pdf());
    });
    assert!(breeze_storage::load_annotations(&path, "current-source").is_err());
}

#[gpui_kit::test]
fn source_changes_keep_clean_history_available_for_recovery(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (_, view) = saved_document(cx, &root.path().join("old.json"));
    view.update(cx, |view, cx| {
        let old_hash = view.books[0].annotations.fingerprint.clone();
        view.prepare_annotations("changed source".into(), cx);
        let data = &view.books[0].annotations;
        assert!(data.ready && data.source_conflict);
        assert_eq!(data.fingerprint, old_hash);
        assert_eq!(data.history.items, vec![note("Original")]);
        assert_eq!(data.saved, vec![note("Original")]);
        assert!(!data.dirty());
        assert!(!view.can_export_pdf());
    });
}

#[gpui_kit::test]
async fn broken_sidecar_can_be_retried_after_external_repair(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("sidecar.json");
    let (handle, view) = saved_document(cx, &path);
    let good = std::fs::read(&path).unwrap();
    let hash = view.read_with(cx, |view, _| view.books[0].annotations.fingerprint.clone());
    std::fs::write(&path, b"broken").unwrap();
    view.update(cx, |view, cx| {
        view.reading.source = Some(Source::Sample);
        view.reading.fingerprint = hash.clone();
        view.books[0].annotations = DocumentAnnotations {
            path: Some(path.clone()),
            ..Default::default()
        };
        view.prepare_annotations(hash.clone(), cx);
    });
    cx.wait_for(handle, std::time::Duration::from_secs(5), |_, cx| {
        view.read(cx).books[0].annotations.error.is_some()
    })
    .await;
    assert_eq!(std::fs::read(&path).unwrap(), b"broken");
    std::fs::write(&path, good).unwrap();
    view.update(cx, |view, cx| view.retry_annotations(cx));
    cx.wait_for(handle, std::time::Duration::from_secs(5), |_, cx| {
        view.read(cx).books[0].annotations.ready
    })
    .await;
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.books[0].annotations.history.items,
            vec![note("Original")]
        )
    });
}

#[gpui_kit::test]
async fn save_as_cancellation_preserves_conflict_and_releases_save_slot(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (handle, view) = saved_document(cx, &root.path().join("original.json"));
    view.update(cx, |view, cx| {
        view.books[0].annotations.error = Some("Existing sidecar conflict".into());
        view.save_annotations_as(cx);
        view.save_annotations_as(cx);
    });
    cx.simulate_new_path_selection(|_| None);
    cx.wait_for(handle, std::time::Duration::from_secs(5), |_, cx| {
        !view.read(cx).books[0].annotations.saving
    })
    .await;
    assert!(!cx.did_prompt_for_new_path());
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.books[0].annotations.error.as_deref(),
            Some("Existing sidecar conflict")
        )
    });
}

#[gpui_kit::test]
fn save_as_rejects_unloaded_and_source_conflicted_histories(cx: &mut TestAppContext) {
    let (_, view) = crate::ui_tests::app(cx);
    for (ready, conflict) in [(false, false), (true, true)] {
        view.update(cx, |view, cx| {
            let data = &mut view.books[0].annotations;
            data.fingerprint = "known source".into();
            data.ready = ready;
            data.source_conflict = conflict;
            view.save_annotations_as(cx);
            assert!(!view.books[0].annotations.saving);
            assert!(view.books[0].annotations.path.is_none());
        });
        assert!(!cx.did_prompt_for_new_path());
    }
}

#[gpui_kit::test]
async fn successful_save_as_acknowledges_only_written_snapshot(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let original = root.path().join("original.json");
    let destination = root.path().join("new.json");
    let (handle, view) = saved_document(cx, &original);
    view.update(cx, |view, cx| {
        view.books[0].annotations.error = Some("Previous write failed".into());
        view.save_annotations_to(0, Some(destination.clone()), cx);
        // Simulate an edit while the immutable snapshot is being written.
        view.books[0]
            .annotations
            .history
            .add(note("Concurrent edit"));
        assert_eq!(view.books[0].annotations.path.as_ref(), Some(&original));
    });
    cx.wait_for(handle, std::time::Duration::from_secs(5), |_, cx| {
        view.read(cx).books[0].annotations.path.as_ref() == Some(&destination)
    })
    .await;
    let hash = view.update(cx, |view, _| {
        let data = &mut view.books[0].annotations;
        assert_eq!(data.saved, vec![note("Original")]);
        assert_eq!(data.history.items.len(), 2);
        assert!(data.dirty());
        assert!(data.error.is_none());
        data.fingerprint.clone()
    });
    assert_eq!(
        breeze_storage::load_annotations(&destination, &hash)
            .unwrap()
            .annotations,
        vec![note("Original")]
    );
    assert_eq!(
        breeze_storage::load_annotations(&original, &hash)
            .unwrap()
            .annotations,
        vec![note("Original")]
    );
}

#[gpui_kit::test]
async fn stale_picker_cannot_attach_to_a_reloaded_document(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (handle, view) = saved_document(cx, &root.path().join("original.json"));
    view.update(cx, |view, cx| {
        view.save_annotations_as(cx);
        view.books[0].annotations = DocumentAnnotations {
            fingerprint: "replacement source".into(),
            ..Default::default()
        };
    });
    let destination = root.path().join("stale.json");
    cx.simulate_new_path_selection(|_| Some(destination.clone()));
    cx.run_until_parked();
    cx.update_window(handle, |_, _, cx| {
        let data = &view.read(cx).books[0].annotations;
        assert!(!data.ready);
        assert!(!data.saving);
        assert!(data.path.is_none());
    })
    .unwrap();
    assert!(!destination.exists());
}

#[gpui_kit::test]
async fn edits_stay_unsaved_until_the_user_saves(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("sidecar.json");
    let (handle, view) = saved_document(cx, &path);
    let before = std::fs::read(&path).unwrap();
    view.update(cx, |view, cx| {
        view.books[0].annotations.history.add(note("Pending"));
        assert!(view.books[0].annotations.dirty());
        assert!(!view.allow_close(cx));
    });
    // No timer or background write may persist an edit on its own: even after the
    // retired debounce window the file still holds only the saved snapshot.
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(2));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let data = &view.books[0].annotations;
        assert!(!data.saving);
        assert_eq!(data.saved, vec![note("Original")]);
        assert!(data.dirty());
    });
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let hash = view.read_with(cx, |view, _| view.books[0].annotations.fingerprint.clone());
    view.update(cx, |view, cx| view.save_annotations(cx));
    cx.wait_for(handle, std::time::Duration::from_secs(5), |_, cx| {
        !view.read(cx).books[0].annotations.saving
    })
    .await;
    view.read_with(cx, |view, _| assert!(!view.books[0].annotations.dirty()));
    assert_eq!(
        breeze_storage::load_annotations(&path, &hash)
            .unwrap()
            .annotations,
        vec![note("Original"), note("Pending")]
    );
}
