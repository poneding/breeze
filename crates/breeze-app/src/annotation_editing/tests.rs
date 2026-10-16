use super::*;
use breeze_core::Rotation;
use breeze_core::annotations::PageGeometry;
use gpui_kit::test::TestAppContextExt;
use gpui_kit::test::TestWindowExt;

#[test]
fn squiggle_selection_follows_the_rendered_crest() {
    let mut a = line();
    a.kind = Kind::Squiggle;
    a.width = 8.;
    a.points = vec![DocPoint::new(20., 50.), DocPoint::new(120., 50.)];
    // At this width the first crest is 12.5 points along the gesture, not the
    // former fixed eight-point zigzag's crest near the start.
    assert!(geometry::hit(&a, DocPoint::new(32.5, 58.), 0.1));
    assert!(!geometry::hit(&a, DocPoint::new(32.5, 42.), 0.1));
}

#[test]
fn shape_handles_resize_display_axes_across_page_and_reader_rotations() {
    for kind in [Kind::Ellipse, Kind::Rectangle, Kind::Area] {
        for rotation in [0, 90, 180, 270] {
            for reader_rotation in [
                Rotation::None,
                Rotation::Right,
                Rotation::Half,
                Rotation::Left,
            ] {
                let page = PageGeometry {
                    crop: breeze_core::annotations::Rect {
                        x: 10.,
                        y: 20.,
                        width: 300.,
                        height: 400.,
                    },
                    rotation,
                };
                let view = page.view(reader_rotation);
                let mut original = line();
                original.kind = kind;
                original.points = vec![DocPoint::new(60., 80.), DocPoint::new(180., 200.)];
                let before = geometry::shape_bounds(&original, view).unwrap();
                for handle in geometry::ResizeHandle::ALL {
                    let (x, y) = handle.position();
                    let pointer = view.document(DocPoint::new(
                        before.x + before.width * x + 17.,
                        before.y + before.height * y + 23.,
                    ));
                    let resized = geometry::resized(&original, handle, pointer, view);
                    assert!(resized.validate().is_ok());
                    assert_eq!(resized.color, original.color);
                    assert_eq!(resized.opacity, original.opacity);
                    let after = geometry::shape_bounds(&resized, view).unwrap();
                    assert!((after.x - before.x - if x == 0. { 17. } else { 0. }).abs() < 0.001);
                    assert!((after.y - before.y - if y == 0. { 23. } else { 0. }).abs() < 0.001);
                    assert!(
                        (after.x + after.width
                            - before.x
                            - before.width
                            - if x == 1. { 17. } else { 0. })
                        .abs()
                            < 0.001
                    );
                    assert!(
                        (after.y + after.height
                            - before.y
                            - before.height
                            - if y == 1. { 23. } else { 0. })
                        .abs()
                            < 0.001
                    );
                }
                for (handle, pointer) in [
                    (
                        geometry::ResizeHandle::TopLeft,
                        DocPoint::new(10_000., 10_000.),
                    ),
                    (
                        geometry::ResizeHandle::BottomRight,
                        DocPoint::new(-10_000., -10_000.),
                    ),
                ] {
                    let resized =
                        geometry::resized(&original, handle, view.document(pointer), view);
                    let bounds = geometry::shape_bounds(&resized, view).unwrap();
                    assert_eq!(bounds.width, 0.5);
                    assert_eq!(bounds.height, 0.5);
                    assert!(resized.validate().is_ok());
                }
                let expanded = geometry::resized(
                    &original,
                    geometry::ResizeHandle::BottomRight,
                    view.document(DocPoint::new(10_000., 10_000.)),
                    view,
                );
                let bounds = geometry::shape_bounds(&expanded, view).unwrap();
                assert_eq!(
                    (bounds.x + bounds.width, bounds.y + bounds.height),
                    view.size()
                );
            }
        }
    }
}

#[gpui_kit::test]
fn shape_resize_handles_commit_once_and_undo(cx: &mut TestAppContext) {
    let (handle, view) = setup(cx);
    cx.update_window(handle, |_, window, cx| {
        for kind in [Kind::Ellipse, Kind::Rectangle, Kind::Area] {
            let mut original = line();
            original.kind = kind;
            original.points = vec![DocPoint::new(80., 400.), DocPoint::new(220., 600.)];
            view.update(cx, |view, cx| {
                view.books[0].annotations.history.items = vec![original.clone()];
                view.select_annotation(0, window, cx);
                view.tool = SELECT_TOOL;
            });
            window.render_frame(cx);
            assert!(window.find("annotation-selection-outline").visible());
            for index in 0..8usize {
                assert!(window.find(("annotation-resize", index)).visible());
                assert_eq!(
                    window.find(("annotation-resize", index)).bounds().size,
                    size(px(16.), px(16.))
                );
            }
            let revision = view.read(cx).books[0].annotations.history.revision();
            let start = window.find(("annotation-resize", 3usize)).bounds().center();
            window.drag(start, start + point(px(35.), px(0.)), cx);
            let resized = view.read(cx).books[0].annotations.history.items[0].clone();
            let page = view
                .read(cx)
                .view_geometry(view.read(cx).preview.as_ref().unwrap().geometry);
            let before = geometry::shape_bounds(&original, page).unwrap();
            let after = geometry::shape_bounds(&resized, page).unwrap();
            assert!(after.width > before.width);
            assert_eq!(after.height, before.height);
            assert_eq!(after.x, before.x);
            assert_eq!(
                view.read(cx).books[0].annotations.history.revision(),
                revision + 1
            );
            window.press("secondary-z", cx);
            assert_eq!(
                view.read(cx).books[0].annotations.history.items,
                vec![original]
            );
            window.press("secondary-shift-z", cx);
            assert_eq!(
                view.read(cx).books[0].annotations.history.items,
                vec![resized]
            );
        }
    })
    .unwrap();
}

fn setup(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Breeze>) {
    let worker = PdfWorker::new().unwrap();
    let page = futures::executor::block_on(worker.request(Source::Sample, 0).unwrap())
        .unwrap()
        .unwrap();
    let hash = page.fingerprint.clone();
    let preview = Preview::from_page(page).unwrap();
    let (handle, view) = crate::ui_tests::app(cx);
    view.update(cx, |view, _| {
        view.preview = Some(preview.clone());
        view.reading.sizes = Arc::new(vec![preview.points; 8]);
        view.reading.source = Some(Source::Sample);
        view.reading.fingerprint = hash.clone();
        view.books[0].annotations.fingerprint = hash;
        view.books[0].annotations.ready = true;
        view.books[0].state.layout = ReadingLayout::Single;
        view.books[0].state.mode = Mode::Annotate;
        view.tool = 5;
    });
    (handle, view)
}

fn line() -> Annotation {
    Annotation {
        page: 0,
        kind: Kind::Line,
        points: vec![DocPoint::new(50., 600.), DocPoint::new(200., 600.)],
        quads: Vec::new(),
        color: [1., 0., 0.],
        opacity: 1.,
        width: 2.,
        text: String::new(),
    }
}

#[gpui_kit::test]
fn click_type_enter_creates_one_annotation_and_escape_cancels(cx: &mut TestAppContext) {
    let (handle, view) = setup(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click_at("page-surface", point(px(60.), px(160.)), cx);
        assert!(window.find("annotation-text-editor").visible());
        assert!(
            view.read(cx)
                .annotation_editing
                .editor
                .as_ref()
                .unwrap()
                .input
                .read(cx)
                .value()
                .is_empty()
        );
        let input = view
            .read(cx)
            .annotation_editing
            .editor
            .as_ref()
            .unwrap()
            .input
            .clone();
        assert!(
            input.read(cx).focus_handle(cx).is_focused(window),
            "Editor lost focus after the placement click"
        );
        window.input("你好，Breeze", cx);
        assert_eq!(input.read(cx).value().as_ref(), "你好，Breeze");
        assert!(view.read(cx).books[0].annotations.history.items.is_empty());
        view.update(cx, |view, cx| {
            assert!(!view.allow_close(cx));
            view.next_page(1, cx);
            view.show_home(cx);
            assert_eq!(view.books[0].state.navigation.page(), 0);
            assert!(view.screen == Screen::Reader);
        });
        assert!(
            input.read(cx).focus_handle(cx).is_focused(window),
            "The new annotation input must retain keyboard focus"
        );
        window.press("enter", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            view.read(cx).annotation_editing.editor.is_none(),
            "{}",
            view.read(cx).message
        );
        let items = &view.read(cx).books[0].annotations.history.items;
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].text, "你好，Breeze");
        let saved = serde_json::to_string(items).unwrap();
        assert_eq!(
            serde_json::from_str::<Vec<Annotation>>(&saved).unwrap(),
            *items
        );
        window.press("secondary-z", cx);
        assert!(view.read(cx).books[0].annotations.history.items.is_empty());
        window.press("secondary-shift-z", cx);
        assert_eq!(view.read(cx).books[0].annotations.history.items.len(), 1);
        window.click_at("page-surface", point(px(280.), px(330.)), cx);
        window.input("Cancel me", cx);
        window.press("escape", cx);
        assert!(view.read(cx).annotation_editing.editor.is_none());
        assert_eq!(view.read(cx).books[0].annotations.history.items.len(), 1);
    })
    .unwrap();
}

#[gpui_kit::test]
fn text_can_be_selected_edited_and_deleted_without_eating_input_keys(cx: &mut TestAppContext) {
    let (handle, view) = setup(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| {
            let mut annotation = line();
            annotation.kind = Kind::Note;
            annotation.text = "Original".into();
            view.books[0].annotations.history.items = vec![annotation];
        });
        window.render_frame(cx);
        window.double_click(("page-annotation-text", 0usize), cx);
        assert!(window.find("annotation-text-editor").visible());
        window.press("secondary-a", cx);
        window.input("Revised!", cx);
        window.press("backspace", cx);
        assert_eq!(view.read(cx).books[0].annotations.history.items.len(), 1);
        window.press("enter", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            view.read(cx).books[0].annotations.history.items[0].text,
            "Revised"
        );
        window.click("delete-page-annotation", cx);
        assert!(view.read(cx).books[0].annotations.history.items.is_empty());
        window.press("secondary-z", cx);
        assert_eq!(
            view.read(cx).books[0].annotations.history.items[0].text,
            "Revised"
        );
        window.press("secondary-z", cx);
        assert_eq!(
            view.read(cx).books[0].annotations.history.items[0].text,
            "Original"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn selection_slider_is_one_transaction_and_stale_targets_are_rejected(cx: &mut TestAppContext) {
    let (handle, view) = setup(cx);
    let original = line();
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.books[0].annotations.history.items = vec![original.clone()];
            view.select_annotation(0, window, cx);
        });
        window.render_frame(cx);
        let slider = view
            .read(cx)
            .annotation_editing
            .selection
            .as_ref()
            .unwrap()
            .opacity
            .clone();
        let bounds = window.find(("slider", slider.entity_id())).bounds();
        window.drag(
            bounds.origin + point(bounds.size.width - px(1.), bounds.size.height / 2.),
            bounds.origin + point(bounds.size.width * 0.4, bounds.size.height / 2.),
            cx,
        );
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!((view.read(cx).books[0].annotations.history.items[0].opacity - 0.4).abs() < 0.02);
        window.press("secondary-z", cx);
        assert_eq!(
            view.read(cx).books[0].annotations.history.items,
            vec![original.clone()]
        );
        assert!(!view.read(cx).books[0].annotations.history.can_undo());
        view.update(cx, |view, cx| {
            view.books[0]
                .annotations
                .history
                .items
                .push(original.clone());
            view.select_annotation(0, window, cx);
            view.books[0]
                .annotations
                .history
                .replace(vec![original.clone()]);
            view.delete_selected_annotation(window, cx);
            assert_eq!(
                view.books[0].annotations.history.items,
                vec![original.clone()]
            );
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn line_hit_movement_and_keyboard_delete_are_page_local(cx: &mut TestAppContext) {
    let (handle, view) = setup(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| {
            view.books[0].annotations.history.items = vec![line()];
            view.tool = SELECT_TOOL;
        });
        window.render_frame(cx);
        let bounds = window.find("page-surface").bounds();
        let preview = view.read(cx).preview.as_ref().unwrap();
        let p = preview.geometry.display(DocPoint::new(120., 600.));
        let start = bounds.origin
            + point(
                px(p.x / preview.points.0 * f32::from(bounds.size.width)),
                px(p.y / preview.points.1 * f32::from(bounds.size.height)),
            );
        window.drag(start, start + point(px(20.), px(10.)), cx);
        assert_ne!(
            view.read(cx).books[0].annotations.history.items[0].points,
            line().points
        );
        assert!(window.find("annotation-selection-controls").visible());
        window.press("delete", cx);
        assert!(view.read(cx).books[0].annotations.history.items.is_empty());
        window.press("secondary-z", cx);
        window.press("secondary-z", cx);
        assert_eq!(
            view.read(cx).books[0].annotations.history.items,
            vec![line()]
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn composition_does_not_commit_a_partial_text_annotation(cx: &mut TestAppContext) {
    let (handle, view) = setup(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click_at("page-surface", point(px(50.), px(160.)), cx);
        let input = view
            .read(cx)
            .annotation_editing
            .editor
            .as_ref()
            .unwrap()
            .input
            .clone();
        input.update(cx, |input, cx| {
            input.replace_and_mark_text_in_range(None, "ni", Some(2..2), window, cx)
        });
        window.press("enter", cx);
    })
    .unwrap();
    view.read_with(cx, |view, _| {
        assert!(view.annotation_editing.editor.is_some());
        assert!(view.books[0].annotations.history.items.is_empty());
    });
}

#[test]
fn hit_testing_follows_shapes_and_rotated_crop_coordinates() {
    let mut a = line();
    a.kind = Kind::Rectangle;
    a.points = vec![DocPoint::new(20., 30.), DocPoint::new(120., 130.)];
    assert!(geometry::hit(&a, DocPoint::new(20., 80.), 3.));
    assert!(!geometry::hit(&a, DocPoint::new(70., 80.), 3.));
    a.kind = Kind::Ellipse;
    assert!(geometry::hit(&a, DocPoint::new(70., 30.), 3.));
    assert!(!geometry::hit(&a, DocPoint::new(70., 80.), 3.));
    for rotation in [0, 90, 180, 270] {
        let page = PageGeometry {
            crop: breeze_core::annotations::Rect {
                x: 10.,
                y: 20.,
                width: 300.,
                height: 400.,
            },
            rotation,
        };
        let p = DocPoint::new(70., 30.);
        assert!(geometry::hit(&a, page.document(page.display(p)), 3.));
        let bounds = geometry::display_bounds(&a, page.view(Rotation::None));
        assert!(bounds.valid());
    }
}

#[gpui_kit::test]
async fn edited_text_and_style_save_to_sidecar_without_changing_source(cx: &mut TestAppContext) {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source.pdf");
    let bytes = include_bytes!("../../../../assets/samples/field-notes.pdf");
    std::fs::write(&source, bytes).unwrap();
    let hash = breeze_storage::fingerprint(&source).unwrap();
    let sidecar = breeze_storage::sidecar_path(&source);
    let (handle, view) = setup(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| {
            view.books[0].source = Source::File(source.clone());
            view.books[0].annotations.path = Some(sidecar.clone());
            view.books[0].annotations.fingerprint = hash.clone();
        });
        window.render_frame(cx);
        window.click_at("page-surface", point(px(80.), px(180.)), cx);
        window.input("持久保存的文本", cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click(("selected-annotation-color", 5usize), cx);
        view.update(cx, |view, cx| view.save_annotations(cx));
    })
    .unwrap();
    cx.wait_for(handle, std::time::Duration::from_secs(5), |_, cx| {
        !view.read(cx).books[0].annotations.saving
    })
    .await;
    let restored = breeze_storage::load_annotations(&sidecar, &hash).unwrap();
    assert_eq!(restored.annotations.len(), 1);
    assert_eq!(restored.annotations[0].text, "持久保存的文本");
    assert_eq!(
        restored.annotations[0].color,
        colors::components(breeze_core::library::CategoryColor::Blue)
    );
    assert_eq!(std::fs::read(&source).unwrap(), bytes);
}

#[gpui_kit::test]
fn unfinished_text_rejects_a_changed_source_and_preserves_input(cx: &mut TestAppContext) {
    let (handle, view) = setup(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click_at("page-surface", point(px(80.), px(160.)), cx);
        window.input("Keep this draft", cx);
        view.update(cx, |view, _| {
            view.books[0].annotations.fingerprint = "changed".into()
        });
        window.press("enter", cx);
    })
    .unwrap();
    view.read_with(cx, |view, cx| {
        let draft = view.annotation_editing.editor.as_ref().unwrap();
        assert_eq!(draft.input.read(cx).value().as_ref(), "Keep this draft");
        assert!(view.books[0].annotations.history.items.is_empty());
    });
}

#[gpui_kit::test]
fn text_placement_uses_the_clicked_spread_page(cx: &mut TestAppContext) {
    let (handle, view) = setup(cx);
    let worker = PdfWorker::new().unwrap();
    let second = futures::executor::block_on(worker.request(Source::Sample, 1).unwrap())
        .unwrap()
        .unwrap();
    let second = Preview::from_page(second).unwrap();
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| {
            view.reading
                .insert(0, view.preview.as_ref().unwrap().clone());
            view.reading.insert(1, second.clone());
            view.books[0].state.layout = ReadingLayout::Spread;
        });
        window.render_frame(cx);
        window
            .within(("page-frame", 1usize))
            .click_at("page-surface", point(px(40.), px(80.)), cx);
        assert_eq!(
            view.read(cx)
                .annotation_editing
                .editor
                .as_ref()
                .unwrap()
                .annotation
                .page,
            1
        );
        window.input("Right page", cx);
        window.press("enter", cx);
    })
    .unwrap();
    view.read_with(cx, |view, _| {
        assert_eq!(view.books[0].annotations.history.items.len(), 1);
        assert_eq!(view.books[0].annotations.history.items[0].page, 1);
    });
}
