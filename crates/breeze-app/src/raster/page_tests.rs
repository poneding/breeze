//! Region-level scheduling over real canvases: visible tiles arrive before
//! predicted work, and failures remain attached to the page they belong to.
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
fn raster_for(_key: Key) -> Pixels {
    Pixels {
        width: 4,
        height: 4,
        bgra: vec![255; 64],
    }
}

#[gpui_kit::test]
fn visible_page_is_split_into_device_regions(cx: &mut TestAppContext) {
    let page = metadata();
    let (handle, view) = crate::ui_tests::app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| seed(view, page, Zoom::FitPage));
        window.render_frame(cx);
    })
    .unwrap();
    view.update(cx, |view, _| {
        let wanted = view.raster.wanted.clone();
        assert!(wanted.len() > 1, "the page viewport should request tiles");
        assert!(wanted.iter().all(|key| key.page == 0));
        assert_eq!(view.raster.next_key(false), Some(wanted[0]));
        let viewport = view.raster.views[0];
        assert!(wanted.iter().all(|key| key.scale() == viewport.scale));
        assert!(wanted.iter().all(|key| key.intersects(viewport.rect)));
    });
}

#[gpui_kit::test]
fn deep_zoom_keeps_each_region_at_the_requested_density(cx: &mut TestAppContext) {
    let page = metadata();
    let (handle, view) = crate::ui_tests::app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| seed(view, page, Zoom::Percent(800)));
        window.render_frame(cx);
    })
    .unwrap();
    view.update(cx, |view, _| {
        let wanted = view.raster.wanted.clone();
        assert!(!wanted.is_empty());
        let asked = wanted[0].scale();
        assert!(asked > 8., "800% asks for a high density, got {asked}");
        assert!(view.raster.next_key(false).is_some());
        assert!(wanted.iter().all(|key| key.scale() == asked));
    });
}

#[gpui_kit::test]
fn rotated_views_request_their_own_orientation(cx: &mut TestAppContext) {
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
        assert_eq!(view.raster.views[0].rotation, Rotation::Right);
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
                .all(|key| !view.raster.wanted.contains(key))
        );
    });
}

#[gpui_kit::test]
fn region_results_satisfy_the_visible_demand(cx: &mut TestAppContext) {
    let page = metadata();
    let (handle, view) = crate::ui_tests::app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| seed(view, page, Zoom::FitPage));
        window.render_frame(cx);
    })
    .unwrap();
    view.update(cx, |view, cx| {
        assert!(!view.raster.visible_ready());
        while let Some(key) = view.raster.next_key(false) {
            view.raster.insert(key, raster_for(key), cx).unwrap();
        }
        assert!(view.raster.visible_ready());
        assert_eq!(view.raster.next_key(false), None);
    });
}

#[gpui_kit::test]
fn a_replaced_raster_does_not_leave_two_densities_resident(cx: &mut TestAppContext) {
    let page = metadata();
    let (handle, view) = crate::ui_tests::app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| seed(view, page, Zoom::FitPage));
        window.render_frame(cx);
    })
    .unwrap();
    view.update(cx, |view, cx| {
        let first = view.raster.next_key(false).unwrap();
        view.raster.insert(first, raster_for(first), cx).unwrap();
        view.set_zoom(Zoom::Percent(200), cx);
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
    })
    .unwrap();
    view.update(cx, |view, cx| {
        let mut second_scale = None;
        while let Some(second) = view.raster.next_key(false) {
            second_scale = Some(second.scale());
            view.raster.insert(second, raster_for(second), cx).unwrap();
        }
        let second_scale = second_scale.expect("the new density");
        assert!(
            view.raster
                .cache
                .iter()
                .all(|entry| entry.0.scale() == second_scale),
            "the superseded density is retired once the requested one is resident"
        );
        assert!(view.raster.visible_ready());
    });
}

#[gpui_kit::test]
async fn a_failed_page_is_reported_for_its_own_page_only(cx: &mut TestAppContext) {
    let page = metadata();
    let (handle, view) = crate::ui_tests::app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| {
            seed(view, page, Zoom::FitPage);
            view.loading = false;
        });
        window.render_frame(cx);
    })
    .unwrap();
    view.update(cx, |view, cx| {
        let key = view
            .raster
            .next_key(false)
            .expect("a visible raster to prepare");
        view.raster.fail(key, "damaged page".into());
        assert!(view.raster.page_failed(key.page));
        assert!(!view.raster.page_failed(key.page + 1));
        assert_ne!(view.raster.next_key(false), Some(key));
        assert!(view.raster.errors.contains_key(&key));
        cx.notify();
    });
}
