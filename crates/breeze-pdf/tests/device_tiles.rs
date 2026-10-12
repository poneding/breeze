use breeze_core::Rotation;
use breeze_pdf::{DeviceTile, PdfWorker, Source};
use pdf_oxide::{
    PdfDocument,
    rendering::{PageRenderer, RenderOptions},
};

fn compare(doc: &PdfDocument, page: usize, dpi: u32) {
    let mut options = RenderOptions::with_dpi(dpi).as_raw();
    options.max_output_pixels = 16_000_000;
    let mut renderer = PageRenderer::new(options);
    let full = renderer.render_page(doc, page).unwrap();
    let mut changed = 0;
    let mut max_error = 0;
    for y in (0..full.height).step_by(512) {
        for x in (0..full.width).step_by(512) {
            let w = 512.min(full.width - x);
            let h = 512.min(full.height - y);
            let tile = renderer
                .render_device_region(doc, page, dpi as f32 / 72., (x, y, w, h))
                .unwrap();
            assert_eq!((tile.width, tile.height), (w, h));
            for row in 0..h {
                let original = &full.data[(((y + row) * full.width + x) * 4) as usize
                    ..(((y + row) * full.width + x + w) * 4) as usize];
                let actual = &tile.data[(row * w * 4) as usize..((row + 1) * w * 4) as usize];
                for (a, b) in original.iter().zip(actual) {
                    let difference = a.abs_diff(*b);
                    changed += usize::from(difference > 2);
                    max_error = max_error.max(difference);
                }
            }
        }
    }
    assert!(
        changed < full.data.len() / 10000,
        "tile seams/content differences: {changed}, max={max_error}, bytes={}",
        full.data.len()
    );
}

#[test]
fn sample_tiles_match_full_page_at_native_and_fractional_scales() {
    let doc =
        PdfDocument::from_bytes(include_bytes!("../../../assets/samples/field-notes.pdf").to_vec())
            .unwrap();
    compare(&doc, 0, 144);
    compare(&doc, 1, 192);
}

#[test]
fn cropped_rotated_transparent_content_matches() {
    use lopdf::{Document, Object, Stream, dictionary};
    let mut source =
        Document::load_mem(include_bytes!("../../../assets/samples/field-notes.pdf")).unwrap();
    let id = source.get_pages()[&1];
    let extra = source.add_object(Stream::new(
        dictionary! {},
        b"q /GSx gs 0.1 0.3 0.9 rg 100 250 260 300 re f 0.9 0.2 0.1 rg 150 150 260 400 re f Q"
            .to_vec(),
    ));
    let resources_id = source
        .get_dictionary(id)
        .unwrap()
        .get(b"Resources")
        .unwrap()
        .as_reference()
        .unwrap();
    source.get_dictionary_mut(resources_id).unwrap().set(
        "ExtGState",
        dictionary! {"GSx"=>dictionary! {"Type"=>"ExtGState","ca"=>0.4}},
    );
    let page = source.get_dictionary_mut(id).unwrap();
    let content = page.get(b"Contents").unwrap().clone();
    page.set("Contents", vec![content, Object::Reference(extra)]);
    page.set(
        "CropBox",
        vec![20.into(), 30.into(), 590.into(), 760.into()],
    );
    for rotation in [0, 90, 180, 270] {
        source
            .get_dictionary_mut(id)
            .unwrap()
            .set("Rotate", rotation);
        let mut bytes = Vec::new();
        source.save_to(&mut bytes).unwrap();
        compare(&PdfDocument::from_bytes(bytes).unwrap(), 0, 144);
    }
}

#[test]
fn metadata_has_no_raster_and_zoom_requests_do_not_reuse_low_resolution() {
    let worker = PdfWorker::new().unwrap();
    let metadata = futures::executor::block_on(worker.request_metadata(Source::Sample, 0).unwrap())
        .unwrap()
        .unwrap();
    assert!(metadata.pixels.is_none());
    assert!(!metadata.glyphs.is_empty());
    for scale in [2., 4., 8.] {
        let key = DeviceTile::new(scale, 256, 256, 512, 512).unwrap();
        let page = futures::executor::block_on(
            worker
                .request_tile(Source::Sample, 0, key, Rotation::None)
                .unwrap(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(page.tile, Some(key));
        assert!(page.glyphs.is_empty());
        assert_eq!(page.fingerprint, metadata.fingerprint);
        let pixels = page.pixels.unwrap();
        assert_eq!(
            (pixels.width, pixels.height, pixels.bgra.len()),
            (512, 512, 512 * 512 * 4)
        );
    }
    assert!(DeviceTile::new(f32::NAN, 0, 0, 512, 512).is_err());
    assert!(DeviceTile::new(2., 0, 0, 100000, 100000).is_err());
}

/// Rotates a whole page independently of the worker so rotated tiles can be
/// checked against a picture built outside the region-mapping code.
fn naive_rotate(source: &[u8], width: u32, height: u32, rotation: Rotation) -> (u32, u32, Vec<u8>) {
    let (out_w, out_h) = if rotation.swaps_axes() {
        (height, width)
    } else {
        (width, height)
    };
    let mut out = vec![0u8; source.len()];
    for y in 0..out_h {
        for x in 0..out_w {
            let (sx, sy) = match rotation {
                Rotation::Right => (y, height - 1 - x),
                Rotation::Half => (width - 1 - x, height - 1 - y),
                Rotation::Left => (width - 1 - y, x),
                Rotation::None => (x, y),
            };
            let from = ((sy * width + sx) * 4) as usize;
            let to = ((y * out_w + x) * 4) as usize;
            out[to..to + 4].copy_from_slice(&source[from..from + 4]);
        }
    }
    (out_w, out_h, out)
}

#[test]
fn rotated_tiles_reassemble_into_the_rotated_page() {
    let worker = PdfWorker::new().unwrap();
    let full = futures::executor::block_on(worker.request(Source::Sample, 0).unwrap())
        .unwrap()
        .unwrap();
    let source = full.pixels.unwrap();
    let (width, height) = (source.width, source.height);
    for rotation in [Rotation::Right, Rotation::Half, Rotation::Left] {
        let (view_w, view_h, expected) = naive_rotate(&source.bgra, width, height, rotation);
        // Cover the rotated page with the same tile grid the reader uses.
        let mut y = 0;
        while y < view_h {
            let h = 512.min(view_h - y);
            let mut x = 0;
            while x < view_w {
                let w = 512.min(view_w - x);
                let tile = DeviceTile::new(2., x, y, w, h).unwrap();
                let page = futures::executor::block_on(
                    worker
                        .request_tile(Source::Sample, 0, tile, rotation)
                        .unwrap(),
                )
                .unwrap()
                .unwrap();
                let pixels = page.pixels.unwrap();
                assert_eq!((pixels.width, pixels.height), (w, h));
                // Device-region rasterization differs from a full-page render by
                // antialiased edge pixels, so the comparison allows a small
                // fraction of bytes; a misplaced tile would differ almost everywhere.
                let mut changed = 0;
                for row in 0..h {
                    let from = (((y + row) * view_w + x) * 4) as usize;
                    let want = &expected[from..from + (w * 4) as usize];
                    let got = &pixels.bgra[(row * w * 4) as usize..((row + 1) * w * 4) as usize];
                    assert_eq!(got.len(), want.len());
                    changed += got
                        .iter()
                        .zip(want)
                        .filter(|(a, b)| a.abs_diff(**b) > 2)
                        .count();
                }
                assert!(
                    changed * 200 < pixels.bgra.len(),
                    "rotation {rotation:?} tile {x},{y}: {changed} differing bytes"
                );
                x += w;
            }
            y += h;
        }
    }
}
