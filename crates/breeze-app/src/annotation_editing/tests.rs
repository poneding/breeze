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
fn handles_align_with_the_frame_edges() {
    // The frame centerlines and every handle square snap to the same device
    // grid. A side handle may slide along its own edge, but it must never sit
    // off that edge, and a corner handle must land exactly on the corner.
    for scale in [1., 2., 3.] {
        for offset in [0.0, 0.13, 0.5, 0.87] {
            let (x, y) = (58.31 + offset, 139.62 + offset);
            let (width, height) = (102.4, 145.9);
            let frame = selection_frame(x, y, width, height, scale);
            let half_tolerance = 0.5 / scale + 0.0001;
            for handle in geometry::ResizeHandle::ALL {
                let (fraction_x, fraction_y) = handle.position();
                let square =
                    handle_square(x + width * fraction_x, y + height * fraction_y, 4.5, scale);
                let center_x = (square[0] + square[2]) / 2.;
                let center_y = (square[1] + square[3]) / 2.;
                if fraction_x == 0. {
                    assert!((center_x - frame[0]).abs() < 0.0001, "{handle:?} left");
                } else if fraction_x == 1. {
                    assert!((center_x - frame[2]).abs() < 0.0001, "{handle:?} right");
                } else {
                    let middle = (frame[0] + frame[2]) / 2.;
                    assert!(
                        (center_x - middle).abs() <= half_tolerance,
                        "{handle:?} mid x"
                    );
                }
                if fraction_y == 0. {
                    assert!((center_y - frame[1]).abs() < 0.0001, "{handle:?} top");
                } else if fraction_y == 1. {
                    assert!((center_y - frame[3]).abs() < 0.0001, "{handle:?} bottom");
                } else {
                    let middle = (frame[1] + frame[3]) / 2.;
                    assert!(
                        (center_y - middle).abs() <= half_tolerance,
                        "{handle:?} mid y"
                    );
                }
            }
        }
    }
}

#[test]
fn handle_square_stays_symmetric_about_its_frame_point() {
    for scale in [1., 2., 3.] {
        for offset in [0.0, 0.13, 0.5, 0.87] {
            let (cx, cy) = (120.37 + offset, 241.61 + offset);
            let square = handle_square(cx, cy, 4.5, scale);
            let center = ((square[0] + square[2]) / 2., (square[1] + square[3]) / 2.);
            let snapped = selection_frame(cx, cy, 0., 0., scale);
            assert!((center.0 - snapped[0]).abs() < 0.001);
            assert!((center.1 - snapped[1]).abs() < 0.001);
            assert!((square[0] + square[2] - 2. * snapped[0]).abs() < 0.001);
            assert!((square[1] + square[3] - 2. * snapped[1]).abs() < 0.001);
            let width = square[2] - square[0];
            let height = square[3] - square[1];
            assert!((8. ..=11.).contains(&width) && (8. ..=11.).contains(&height));
        }
    }
}

#[test]
fn selection_frame_centres_edges_on_device_pixels() {
    // Page coordinates reach the overlay as fractions. Painting element borders
    // at those offsets gave each edge different antialiasing, which read as an
    // uneven frame on the top/left versus the bottom/right. Edges must land on
    // device pixel centres so every side covers its pixels identically.
    for scale in [1., 2., 3.] {
        for offset in [0.13, 0.5, 0.87] {
            let frame = selection_frame(120.37 + offset, 241.61 + offset, 180.4, 96.7, scale);
            for edge in frame {
                let device = edge * scale - 0.5;
                assert!(
                    (device - device.round()).abs() < 0.0001,
                    "edge {edge} is not centred on a device pixel at scale {scale}"
                );
            }
            assert!(frame[2] > frame[0] && frame[3] > frame[1]);
            for (edge, exact) in [
                (frame[0], 120.37 + offset),
                (frame[1], 241.61 + offset),
                (frame[2], 120.37 + offset + 180.4),
                (frame[3], 241.61 + offset + 96.7),
            ] {
                assert!((edge - exact).abs() <= 0.5 / scale + 0.0001);
            }
        }
    }
    // A degenerate scale factor must not produce NaN geometry.
    assert!(
        selection_frame(1.5, 2.5, 10., 10., f32::NAN)
            .iter()
            .all(|v| v.is_finite())
    );
}

#[test]
fn resize_handle_squares_stay_uniform_and_on_the_device_grid() {
    for scale in [1., 2., 3.] {
        for offset in [0.07, 0.5, 0.93] {
            let square = handle_square(300.4 + offset, 180.9 + offset, 4.5, scale);
            // Edges share the frame's half-pixel grid, so a stroked square keeps
            // identical weight on all four sides.
            for edge in square {
                let device = edge * scale - 0.5;
                // f32 multiply-add leaves ~1e-4 logical pixels of rounding noise
                // at fractional scales; anything larger is a real misalignment.
                assert!(
                    (device - device.round()).abs() < 0.001,
                    "edge {edge} is off the device grid at scale {scale}"
                );
            }
            let (width, height) = (square[2] - square[0], square[3] - square[1]);
            assert!((8. ..=11.).contains(&width), "width {width}");
            assert!((8. ..=11.).contains(&height), "height {height}");
            assert!((width - height).abs() < 0.0001, "the square is not square");
        }
    }
}

#[test]
fn selection_glow_layers_get_narrower_and_more_opaque() {
    // The frame is a soft halo under a crisp line: each inner stroke must be
    // narrower and less transparent than the one behind it, and the core must
    // still be narrow enough to read as a border rather than a filled band.
    let mut previous_width = f32::INFINITY;
    let mut previous_alpha = 0f32;
    for (width, color) in SELECTION_STROKES {
        let alpha = (color & 0xff) as f32 / 255.;
        assert!(width < previous_width, "layer {width} is not narrower");
        assert!(
            alpha > previous_alpha,
            "layer alpha {alpha} is not stronger"
        );
        assert!(alpha <= 1.);
        previous_width = width;
        previous_alpha = alpha;
    }
    assert_eq!(SELECTION_STROKES.last().unwrap().0, 1.5);
    assert!(previous_alpha > 0.8, "the core line is too faint");
    assert!(SELECTION_STROKES[0].1 & 0xff < 0x30, "the halo is not soft");
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
