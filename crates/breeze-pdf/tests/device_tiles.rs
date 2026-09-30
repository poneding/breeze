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
        let page =
            futures::executor::block_on(worker.request_tile(Source::Sample, 0, key).unwrap())
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
