//! Page-turn presentation: one device region per viewport, sliced back into the
//! unchanged per-cell cache, plus prediction of the pages a reader is about to
//! reach. The region-versus-cell pixel equivalence itself is checked in
//! `breeze-pdf`'s `device_tiles` test, which can render the real engine.
use super::*;
use gpui_kit::test::TestWindowExt;

fn metadata() -> breeze_pdf::Page {
    futures::executor::block_on(
        PdfWorker::new()
            .unwrap()
            .request_metadata(Source::Sample, 0)
            .unwrap(),
    )
    .unwrap()
    .unwrap()
}
fn seed(view: &mut Breeze, page: breeze_pdf::Page, zoom: Zoom) {
    view.reading.sizes = page.page_sizes.clone();
    view.reading.fingerprint = page.fingerprint.clone();
    let preview = Preview::from_page(page).unwrap();
    view.reading.insert(0, preview.clone());
    view.preview = Some(preview);
    view.books[0].state.zoom = zoom;
    view.books[0].state.layout = ReadingLayout::Single;
    view.loading = true;
}
fn bounds(keys: &[Key]) -> (u32, u32, u32, u32) {
    let left = keys.iter().map(|key| key.tile.x).min().unwrap();
    let top = keys.iter().map(|key| key.tile.y).min().unwrap();
    let right = keys
        .iter()
        .map(|key| key.tile.x + key.tile.width)
        .max()
        .unwrap();
    let bottom = keys
        .iter()
        .map(|key| key.tile.y + key.tile.height)
        .max()
        .unwrap();
    (left, top, right - left, bottom - top)
}
fn pixels_in(group: &Group) -> Pixels {
    let (_, _, width, height) = group.region.region();
    Pixels {
        width,
        height,
        bgra: vec![255; width as usize * height as usize * 4],
    }
}
fn assert_contains(group: &Group) {
    for key in &group.keys {
        assert!(key.tile.x >= group.region.x && key.tile.y >= group.region.y);
        assert!(key.tile.x + key.tile.width <= group.region.x + group.region.width);
        assert!(key.tile.y + key.tile.height <= group.region.y + group.region.height);
    }
}

#[gpui_kit::test]
fn one_request_covers_the_whole_visible_viewport(cx: &mut TestAppContext) {
    let page = metadata();
    let (handle, view) = crate::ui_tests::app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| seed(view, page, Zoom::FitPage));
        window.render_frame(cx);
    })
    .unwrap();
    view.update(cx, |view, _| {
        let wanted = view.raster.wanted.clone();
        assert!(
            wanted.len() > 1,
            "the fixture must need more than one cell for this test to mean anything"
        );
        let group = view.raster.next_group(false).expect("a visible region");
        assert_eq!(
            group.keys.len(),
            wanted.len(),
            "one request must cover every visible cell instead of one cell per request"
        );
        assert_eq!(group.region.region(), bounds(&wanted));
        assert!(
            u64::from(group.region.width) * u64::from(group.region.height) <= MAX_REGION_PIXELS
        );
        assert_contains(&group);
    });
}

#[gpui_kit::test]
fn high_zoom_regions_stay_within_the_ceiling_and_still_group_cells(cx: &mut TestAppContext) {
    let page = metadata();
    let (handle, view) = crate::ui_tests::app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| seed(view, page, Zoom::Percent(400)));
        window.render_frame(cx);
    })
    .unwrap();
    view.update(cx, |view, _| {
        let group = view.raster.next_group(false).expect("a visible region");
        assert!(
            group.keys.len() > 1,
            "a zoomed viewport must still be grouped, not rendered cell by cell"
        );
        assert!(
            u64::from(group.region.width) * u64::from(group.region.height) <= MAX_REGION_PIXELS
        );
        assert_eq!(group.region.region(), bounds(&group.keys));
        assert_contains(&group);
    });
}

#[gpui_kit::test]
fn rotated_views_group_cells_of_their_own_rotation(cx: &mut TestAppContext) {
    let page = metadata();
    let (handle, view) = crate::ui_tests::app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| {
            seed(view, page, Zoom::FitPage);
            view.books[0].state.rotation = Rotation::Right;
        });
        window.render_frame(cx);
    })
    .unwrap();
    view.update(cx, |view, _| {
        let wanted = view.raster.wanted.clone();
        assert!(!wanted.is_empty());
        assert!(wanted.iter().all(|key| key.rotation == Rotation::Right));
        let group = view.raster.next_group(false).expect("a visible region");
        assert!(group.rotation == Rotation::Right);
        assert!(
            group.keys.iter().all(|key| key.rotation == group.rotation),
            "a region may not mix rotations"
        );
        assert_eq!(group.region.region(), bounds(&group.keys));
    });
}

#[gpui_kit::test]
fn two_pages_are_predicted_before_the_reader_turns(cx: &mut TestAppContext) {
    let page = metadata();
    let (handle, view) = crate::ui_tests::app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| seed(view, page, Zoom::FitPage));
        window.render_frame(cx);
    })
    .unwrap();
    view.update(cx, |view, _| {
        let predicted = view.raster.prefetch_pages();
        assert!(
            predicted.contains(&1) && predicted.contains(&2),
            "both following pages should be prepared, predicted {predicted:?}"
        );
        assert_eq!(
            view.books[0].state.navigation.page(),
            0,
            "prefetch is not navigation"
        );
        assert!(
            view.raster
                .prefetch
                .iter()
                .map(|key| key.bytes())
                .sum::<usize>()
                <= PREFETCH_BUDGET
        );
    });
}

#[gpui_kit::test]
fn a_region_is_sliced_into_its_cells_with_unrelated_pixels_ignored(cx: &mut TestAppContext) {
    let page = metadata();
    let (handle, view) = crate::ui_tests::app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| seed(view, page, Zoom::FitPage));
        window.render_frame(cx);
    })
    .unwrap();
    view.update(cx, |view, cx| {
        let group = view.raster.next_group(false).expect("a visible region");
        let pixels = pixels_in(&group);
        view.raster.insert_group(&group, pixels, cx).unwrap();
        assert!(
            group.keys.iter().all(|key| view.raster.contains(*key)),
            "every cell of the region must be cached after one result"
        );
        assert!(view.raster.visible_ready());
    });
}

#[gpui_kit::test]
fn a_failed_region_marks_every_cell_it_covered(cx: &mut TestAppContext) {
    cx.update(|_| {
        let mut raster = RasterView::default();
        let keys = keys(Viewport {
            page: 3,
            rotation: Rotation::None,
            points: (600., 800.),
            scale: 2.,
            rect: [0., 0., 600., 400.],
        })
        .unwrap();
        assert!(keys.len() > 1);
        raster.fail(
            &Group {
                page: 3,
                rotation: Rotation::None,
                keys: keys.clone(),
                region: keys[0].tile,
            },
            "region failed".into(),
        );
        assert!(keys.iter().all(|key| raster.errors.contains_key(key)));
        assert!(raster.next_group(false).is_none());
    });
}
