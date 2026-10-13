//! Page-level scheduling over real canvases: one request per visible page, the
//! pages a reader is about to reach prepared ahead, and a failure reported for
//! the page it belongs to. The rendering contract itself (density, rounding,
//! rotation) is covered by `breeze-pdf`'s `page_requests` test, which can render
//! the real engine.
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
fn one_page_request_covers_the_whole_visible_page(cx: &mut TestAppContext) {
    let page = metadata();
    let (handle, view) = crate::ui_tests::app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| seed(view, page, Zoom::FitPage));
        window.render_frame(cx);
    })
    .unwrap();
    view.update(cx, |view, _| {
        let wanted = view.raster.wanted.clone();
        assert_eq!(
            wanted.len(),
            1,
            "the visible page is one raster, not a grid"
        );
        assert_eq!(wanted[0].page, 0);
        assert_eq!(view.raster.next_key(false), Some(wanted[0]));
        // The density the surface asks for is the density that is requested, with
        // no viewport-sized ceiling applied to it.
        let viewport = view.raster.views[0];
        assert_eq!(wanted[0].scale(), viewport.scale);
        assert!(wanted[0].requested_dpi() as f32 / 72. >= viewport.scale);
    });
}

#[gpui_kit::test]
fn a_deep_zoom_still_asks_for_the_whole_page_at_that_density(cx: &mut TestAppContext) {
    let page = metadata();
    let (handle, view) = crate::ui_tests::app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| seed(view, page, Zoom::Percent(800)));
        window.render_frame(cx);
    })
    .unwrap();
    view.update(cx, |view, _| {
        let wanted = view.raster.wanted.clone();
        assert_eq!(wanted.len(), 1);
        let asked = wanted[0].scale();
        assert!(asked > 8., "800% asks for a high density, got {asked}");
        // Nothing refuses or reduces the request at scheduling time: the reader's
        // surface gets the density it asked for, and memory follows it.
        assert!(view.raster.next_key(false).is_some());
        assert_eq!(
            wanted[0].requested_dpi(),
            (f64::from(asked) * 72.).ceil() as u32
        );
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
        assert_eq!(wanted.len(), 1);
        assert_eq!(wanted[0].rotation, Rotation::Right);
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
fn one_result_satisfies_the_visible_demand(cx: &mut TestAppContext) {
    let page = metadata();
    let (handle, view) = crate::ui_tests::app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, _| seed(view, page, Zoom::FitPage));
        window.render_frame(cx);
    })
    .unwrap();
    view.update(cx, |view, cx| {
        assert!(!view.raster.visible_ready());
        let key = view
            .raster
            .next_key(false)
            .expect("a visible raster to prepare");
        view.raster.insert(key, raster_for(key), cx).unwrap();
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
        let second = view.raster.next_key(false).expect("the new density");
        assert_ne!(second, Key::new(0, Rotation::None, 0.));
        view.raster.insert(second, raster_for(second), cx).unwrap();
        assert!(
            view.raster
                .cache
                .iter()
                .all(|entry| entry.0.scale() == second.scale()),
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
        assert!(
            view.raster.next_key(false).is_none(),
            "a failed page is not requested again until the demand changes"
        );
        cx.notify();
    });
}
