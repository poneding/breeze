//! Whole-page rasterization at the density the reader's surface asks for.
//!
//! The engine takes an integer DPI and reduces the scale when a request exceeds
//! its own output budget. Breeze therefore rounds the request up and refuses a
//! result that came back smaller than asked, because presenting a reduced raster
//! as the requested density is exactly the blurry page this path exists to avoid.
use breeze_core::Rotation;
use breeze_pdf::{Page, PdfWorker, Source};

fn request(worker: &PdfWorker, page: usize, rotation: Rotation, scale: f32) -> Page {
    futures::executor::block_on(
        worker
            .request_page(Source::Sample, page, rotation, scale)
            .unwrap(),
    )
    .unwrap()
    .unwrap()
}

fn expected(points: (f32, f32), rendered_scale: f32) -> (u32, u32) {
    (
        (f64::from(points.0) * f64::from(rendered_scale)).ceil() as u32,
        (f64::from(points.1) * f64::from(rendered_scale)).ceil() as u32,
    )
}

/// The engine rounds a fractional extent its own way, so a pixel of slack is
/// expected; a reduced scale would move the extent far more than that.
fn assert_covers(actual: (u32, u32), wanted: (u32, u32), what: &str) {
    assert!(
        actual.0.abs_diff(wanted.0) <= 1 && actual.1.abs_diff(wanted.1) <= 1,
        "{what}: expected about {wanted:?}, got {actual:?}"
    );
}

#[test]
fn a_page_request_returns_the_density_it_was_asked_for_rounded_up() {
    let worker = PdfWorker::new().unwrap();
    for scale in [1., 2., 2.3466, 4.] {
        let page = request(&worker, 0, Rotation::None, scale);
        let dpi = page
            .rendered_dpi
            .expect("a page request reports its density");
        assert_eq!(
            dpi,
            (f64::from(scale) * 72.).ceil() as u32,
            "scale {scale} must round up to whole DPI, never down"
        );
        let rendered = dpi as f32 / 72.;
        assert!(
            rendered >= scale,
            "scale {scale} came back at {rendered}, which would be blurry"
        );
        let pixels = page.pixels.expect("pixels");
        assert_covers(
            (pixels.width, pixels.height),
            expected(page.points, rendered),
            "the raster must cover the page at the reported density",
        );
        assert_eq!(
            pixels.bgra.len(),
            (pixels.width * pixels.height * 4) as usize
        );
    }
}

#[test]
fn a_denser_request_returns_more_pixels() {
    let worker = PdfWorker::new().unwrap();
    let normal = request(&worker, 0, Rotation::None, 2.).pixels.unwrap();
    let dense = request(&worker, 0, Rotation::None, 4.).pixels.unwrap();
    assert!(
        dense.width > normal.width && dense.height > normal.height,
        "4x must carry more pixels than 2x: {}x{} vs {}x{}",
        dense.width,
        dense.height,
        normal.width,
        normal.height
    );
    // Twice the density is twice the extent on each axis, up to one pixel of
    // rounding.
    assert!((i64::from(dense.width) - 2 * i64::from(normal.width)).abs() <= 1);
}

#[test]
fn a_quarter_turn_swaps_the_page_extents() {
    let worker = PdfWorker::new().unwrap();
    let upright = request(&worker, 0, Rotation::None, 2.);
    let turned = request(&worker, 0, Rotation::Right, 2.);
    let upright = upright.pixels.unwrap();
    let turned = turned.pixels.unwrap();
    assert_eq!(
        (turned.width, turned.height),
        (upright.height, upright.width),
        "a quarter turn exchanges the raster extents"
    );
}

/// Metadata and page rasters are separate requests: text geometry is expensive
/// and does not belong on every density change, and pixels do not belong on a
/// metadata request.
#[test]
fn metadata_has_no_raster_and_a_page_request_has_no_glyphs() {
    let worker = PdfWorker::new().unwrap();
    let metadata = futures::executor::block_on(worker.request_metadata(Source::Sample, 0).unwrap())
        .unwrap()
        .unwrap();
    assert!(metadata.pixels.is_none());
    assert!(metadata.rendered_dpi.is_none());
    assert!(!metadata.glyphs.is_empty());

    let page = request(&worker, 0, Rotation::None, 2.);
    assert!(page.pixels.is_some());
    assert!(page.glyphs.is_empty());
    assert_eq!(page.fingerprint, metadata.fingerprint);
}

/// A thumbnail is a page raster too, but at its own fixed low density.
#[test]
fn a_thumbnail_reports_its_own_density() {
    let worker = PdfWorker::new().unwrap();
    let thumbnail = futures::executor::block_on(
        worker
            .request_thumbnail(Source::Sample, 0, Rotation::None)
            .unwrap(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(thumbnail.rendered_dpi, Some(24));
    let pixels = thumbnail.pixels.unwrap();
    assert_covers(
        (pixels.width, pixels.height),
        expected(thumbnail.points, 24. / 72.),
        "a thumbnail covers its page at its own density",
    );
}

#[test]
fn an_unsupported_scale_is_rejected() {
    let worker = PdfWorker::new().unwrap();
    for scale in [0., -1., f32::NAN, 1e6] {
        let result = futures::executor::block_on(
            worker
                .request_page(Source::Sample, 0, Rotation::None, scale)
                .unwrap(),
        )
        .unwrap();
        assert!(result.is_err(), "scale {scale} must be refused");
    }
}

#[test]
fn a_page_beyond_the_document_is_rejected() {
    let worker = PdfWorker::new().unwrap();
    for (mode, result) in [
        (
            "render",
            futures::executor::block_on(
                worker
                    .request(Source::Sample, 99)
                    .expect("the worker accepts the request"),
            )
            .expect("the request completes"),
        ),
        (
            "metadata",
            futures::executor::block_on(
                worker
                    .request_metadata(Source::Sample, 99)
                    .expect("the worker accepts the request"),
            )
            .expect("the request completes"),
        ),
    ] {
        match result {
            Ok(page) => panic!("{mode}: page 100 was served as page {}", page.page + 1),
            Err(error) => assert!(
                matches!(
                    error.downcast_ref::<breeze_pdf::PdfError>(),
                    Some(breeze_pdf::PdfError::PageOutOfRange { page: 99, .. })
                ),
                "{mode}: unexpected error: {error:#}"
            ),
        }
    }
}
