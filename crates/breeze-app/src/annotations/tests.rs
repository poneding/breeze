use super::*;
use gpui_kit::test::TestAppContextExt;

#[test]
fn stroked_geometry_has_round_caps_and_no_miter_spikes() {
    let contains = |path: &gpui_kit::Path<gpui_kit::Pixels>, p: Point<gpui_kit::Pixels>| {
        path.vertices.as_chunks::<3>().0.iter().any(|triangle| {
            let [a, b, c] = triangle.each_ref().map(|v| v.xy_position);
            let cross = |a: Point<gpui_kit::Pixels>, b: Point<gpui_kit::Pixels>| {
                f32::from(b.x - a.x) * f32::from(p.y - a.y)
                    - f32::from(b.y - a.y) * f32::from(p.x - a.x)
            };
            let signs = [cross(a, b), cross(b, c), cross(c, a)];
            signs.iter().all(|s| *s >= 0.) || signs.iter().all(|s| *s <= 0.)
        })
    };
    for device_scale in [1., 2.] {
        let mut line = annotation_stroke(10., device_scale);
        line.move_to(point(px(20.), px(30.)));
        line.line_to(point(px(80.), px(30.)));
        let line = line.build().unwrap();
        for x in [15.5, 84.5] {
            assert!(
                contains(&line, point(px(x), px(30.))),
                "Caps extend past the anchors"
            );
            assert!(
                !contains(&line, point(px(x), px(34.5))),
                "Caps have no square corners"
            );
        }
        let mut ink = annotation_stroke(10., device_scale);
        ink.move_to(point(px(20.), px(50.)));
        ink.line_to(point(px(50.), px(20.)));
        ink.line_to(point(px(52.), px(50.)));
        let ink = ink.build().unwrap();
        assert!(ink.vertices.iter().all(|v| v.xy_position.y >= px(15.)));
    }
}

#[test]
fn long_round_pen_strokes_keep_all_their_segments() {
    let points: Vec<_> = (0..20_000)
        .map(|i| DocPoint::new(i as f32 * 0.1, if i % 2 == 0 { 0. } else { 20. }))
        .collect();
    for width in [2., 100.] {
        let path =
            annotation_polyline(&points, false, |p| point(px(p.x), px(p.y)), width, 2.).unwrap();
        assert!(path.vertices.len() > 20_000);
        assert!(path.bounds.left() < px(0.));
        assert!(path.bounds.right() > px(1999.9));
    }
}

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
