use breeze_core::{
    Rotation,
    annotations::{Annotation, Kind, Point, Rect},
};
use breeze_pdf::{
    PdfWorker, Source,
    export::{ExportRequest, export_annotations},
};
use lopdf::{Document, Object, Stream, dictionary};
use mupdf::pdf::PdfDocument as MuPdfDocument;

fn fixture(kind: &str) -> Document {
    let mut doc = Document::with_version("1.7");
    let pages = doc.new_object_id();
    let mut resources = dictionary! {};
    let mut content = b"q 0.2 0.4 0.6 rg 10 10 80 80 re f Q ".to_vec();
    match kind {
        "image" | "image-mask" | "truncated-mask" | "smask" | "mask" | "valid-smask"
        | "valid-mask" => {
            let mut image = dictionary! {"Type"=>"XObject", "Subtype"=>"Image", "Width"=>1, "Height"=>1, "BitsPerComponent"=>8, "ColorSpace"=>"DeviceRGB"};
            let mut pixels = vec![255, 0, 0];
            match kind {
                "image" => {
                    image.set("Filter", "DCTDecode");
                    pixels = b"damaged JPEG".to_vec();
                }
                "image-mask" => {
                    image.set("ImageMask", true);
                }
                "truncated-mask" => {
                    image.set("ImageMask", true);
                    image.set("BitsPerComponent", 1);
                    image.set("Height", 8);
                    pixels = vec![0];
                }
                "smask" | "mask" | "valid-smask" | "valid-mask" => {
                    let mut mask = dictionary! {"Type"=>"XObject", "Subtype"=>"Image", "Width"=>1, "Height"=>1, "BitsPerComponent"=>8, "ColorSpace"=>"DeviceGray"};
                    let samples = if kind == "valid-mask" {
                        mask.set("ImageMask", true);
                        mask.set("BitsPerComponent", 1);
                        vec![0]
                    } else if kind.starts_with("valid") {
                        vec![128]
                    } else {
                        mask.set("Filter", "DCTDecode");
                        b"damaged JPEG mask".to_vec()
                    };
                    let mask = doc.add_object(Stream::new(mask, samples));
                    image.set(
                        if kind.ends_with("smask") {
                            "SMask"
                        } else {
                            "Mask"
                        },
                        mask,
                    );
                }
                _ => {}
            }
            let object = doc.add_object(Stream::new(image, pixels));
            resources.set("XObject", dictionary! {"Broken"=>object});
            content.extend_from_slice(b"q 60 0 0 60 20 20 cm /Broken Do Q");
        }
        "inline-image" => content.extend_from_slice(
            b"q 60 0 0 60 20 20 cm BI /W 1 /H 1 /BPC 8 /CS /RGB /F /DCT ID brokenJPEG EI Q",
        ),
        "inline-mask" => content
            .extend_from_slice(b"q 60 0 0 60 20 20 cm BI /W 1 /H 1 /BPC 8 /IM true ID x EI Q"),
        "form" | "cyclic-form" => {
            let object = doc.new_object_id();
            let nested = if kind == "cyclic-form" {
                dictionary! {"XObject"=>dictionary! {"Again"=>object}}
            } else {
                dictionary! {}
            };
            doc.objects.insert(object, Object::Stream(Stream::new(
                dictionary! {"Type"=>"XObject", "Subtype"=>"Form", "BBox"=>vec![0.into(),0.into(),100.into(),100.into()], "Resources"=>nested},
                if kind == "cyclic-form" {b"/Again Do".to_vec()} else {b"/Absent Do".to_vec()},
            )));
            resources.set("XObject", dictionary! {"Broken"=>object});
            content.extend_from_slice(b"/Broken Do");
        }
        "shading" => {
            resources.set(
                "Shading",
                dictionary! {"Broken"=>dictionary! {"ShadingType"=>99,"ColorSpace"=>"DeviceRGB"}},
            );
            content.extend_from_slice(b"/Broken sh");
        }
        "missing" => content.extend_from_slice(b"/Absent Do"),
        _ => {}
    }
    let content = doc.add_object(Stream::new(dictionary! {}, content));
    let good_content = doc.add_object(Stream::new(
        dictionary! {},
        b"0.2 0.4 0.6 rg 10 10 80 80 re f".to_vec(),
    ));
    let first = doc.add_object(dictionary! {"Type"=>"Page","Parent"=>pages, "MediaBox"=>vec![0.into(),0.into(),100.into(),100.into()],"Resources"=>resources,"Contents"=>content});
    let second = doc.add_object(dictionary! {"Type"=>"Page","Parent"=>pages, "MediaBox"=>vec![0.into(),0.into(),100.into(),100.into()],"Resources"=>dictionary! {},"Contents"=>good_content});
    doc.objects.insert(
        pages,
        Object::Dictionary(
            dictionary! {"Type"=>"Pages","Kids"=>vec![first.into(),second.into()],"Count"=>2},
        ),
    );
    let root = doc.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages});
    doc.trailer.set("Root", root);
    doc
}

fn request(worker: &PdfWorker, source: Source, kind: usize) -> anyhow::Result<breeze_pdf::Page> {
    let receiver = match kind {
        0 => worker.request(source, 0),
        1 => worker.request_thumbnail(source, 0, Rotation::None),
        2 => worker.request_page(source, 0, Rotation::None, 1.),
        3 => worker.request_metadata(source, 0),
        4 => worker.request_region(
            source,
            0,
            Rotation::None,
            1.,
            Rect {
                x: 0.,
                y: 0.,
                width: 100.,
                height: 100.,
            },
        ),
        _ => unreachable!(),
    }
    .unwrap();
    futures::executor::block_on(receiver).unwrap()
}

fn assert_region_matches_full_page(
    worker: &PdfWorker,
    source: Source,
    page_index: usize,
    base_points: (f32, f32),
) {
    for rotation in [
        Rotation::None,
        Rotation::Right,
        Rotation::Half,
        Rotation::Left,
    ] {
        let points = rotation.apply(base_points);
        let whole = futures::executor::block_on(
            worker
                .request_page(source.clone(), page_index, rotation, 1.)
                .unwrap(),
        )
        .unwrap()
        .unwrap();
        let whole = whole.pixels.unwrap();
        let region = Rect {
            x: 5.,
            y: 7.,
            width: (points.0 - 10.).min(32.),
            height: (points.1 - 14.).min(30.),
        };
        let clipped = futures::executor::block_on(
            worker
                .request_region(source.clone(), page_index, rotation, 1., region)
                .unwrap(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(clipped.region, Some(region));
        let clipped = clipped.pixels.unwrap();
        assert_eq!(
            (clipped.width, clipped.height),
            (region.width as u32, region.height as u32)
        );
        for row in 0..clipped.height as usize {
            let source_start =
                ((region.y as usize + row) * whole.width as usize + region.x as usize) * 4;
            let target_start = row * clipped.width as usize * 4;
            let row_bytes = clipped.width as usize * 4;
            assert_eq!(
                &clipped.bgra[target_start..target_start + row_bytes],
                &whole.bgra[source_start..source_start + row_bytes],
                "cropped region differs at rotation {rotation:?}, row {row}"
            );
        }
    }
}

/// MuPDF draws malformed content defensively and reports nothing through its
/// `Cookie`, so the adapter verifies referenced content itself. Every case here
/// would otherwise publish a page with part of its content missing.
#[test]
fn damaged_content_is_reported_instead_of_rendered_as_a_partial_page() {
    let root = tempfile::tempdir().unwrap();
    for kind in [
        // A DCTDecode stream the engine cannot decode, as a named XObject and
        // inside an inline image.
        "image",
        "inline-image",
        // Forms that draw an unresolved XObject and a form that recurses.
        "form",
        "cyclic-form",
        // A broken soft mask or explicit mask behind an otherwise valid image.
        "smask",
        "mask",
        // A page drawing an unresolved XObject, and an unsupported shading.
        "missing",
        "shading",
        // An image mask with fewer scanlines than its declared height.
        "truncated-mask",
    ] {
        assert_page_render_fails(root.path(), kind);
    }
}

/// Damage the engine recovers from must stay readable. These streams are short
/// or unusual rather than undecodable, and MuPDF pads or clamps them, so failing
/// the page would turn a tolerant decode into a false report.
#[test]
fn recoverable_malformed_content_still_renders() {
    let root = tempfile::tempdir().unwrap();
    for kind in ["image-mask", "inline-mask"] {
        assert_page_renders(root.path(), kind);
    }
}

fn assert_page_render_fails(root: &std::path::Path, kind: &str) {
    let path = root.join(format!("{kind}.pdf"));
    fixture(kind).save(&path).unwrap();
    let source = Source::File(path.clone());
    let before = std::fs::read(&path).unwrap();
    let worker = PdfWorker::new().unwrap();
    assert!(
        request(&worker, source.clone(), 3).is_ok(),
        "{kind}: metadata remains available"
    );
    for mode in 0..=2 {
        match request(&worker, source.clone(), mode) {
            Ok(_) => panic!("{kind}/{mode}: published incomplete pixels"),
            Err(error) => assert!(
                matches!(
                    error.downcast_ref::<breeze_pdf::PdfError>(),
                    Some(breeze_pdf::PdfError::PageRenderFailed { page: 0 })
                ),
                "{kind}: {error:#}"
            ),
        }
    }
    if request(&worker, source.clone(), 4).is_ok() {
        panic!("{kind}/region: published incomplete pixels");
    }
    assert_recovery_after_damage(&worker, source, &path, &before, kind);
}

fn assert_page_renders(root: &std::path::Path, kind: &str) {
    let path = root.join(format!("{kind}.pdf"));
    fixture(kind).save(&path).unwrap();
    let source = Source::File(path.clone());
    let before = std::fs::read(&path).unwrap();
    let worker = PdfWorker::new().unwrap();
    for mode in 0..=2 {
        let rendered = request(&worker, source.clone(), mode)
            .unwrap_or_else(|error| panic!("{kind}/{mode}: {error:#}"));
        assert!(rendered.pixels.is_some(), "{kind}/{mode}: no pixels");
    }
    let region = request(&worker, source.clone(), 4)
        .unwrap_or_else(|error| panic!("{kind}/region: {error:#}"));
    assert!(region.pixels.is_some(), "{kind}/region: no pixels");
    assert_recovery_after_damage(&worker, source, &path, &before, kind);
}

/// The next page still renders and the source file is untouched, whatever the
/// damaged page reported.
fn assert_recovery_after_damage(
    worker: &PdfWorker,
    source: Source,
    path: &std::path::Path,
    before: &[u8],
    kind: &str,
) {
    let good = futures::executor::block_on(worker.request(source, 1).unwrap())
        .unwrap()
        .unwrap_or_else(|error| panic!("{kind}: healthy page failed: {error:#}"));
    assert!(good.pixels.is_some(), "{kind}: healthy page has no pixels");
    assert_eq!(
        std::fs::read(path).unwrap(),
        before,
        "{kind}: source modified"
    );
    if let Some(dir) = std::env::var_os("BREEZE_RENDER_REVIEW_DIR") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(path, dir.join(format!("{kind}.pdf"))).unwrap();
    }
}

#[test]
fn valid_masks_and_default_geometry_still_render_and_export() {
    let root = tempfile::tempdir().unwrap();
    for kind in ["valid", "valid-smask", "valid-mask"] {
        let path = root.path().join(format!("{kind}.pdf"));
        let mut fixture = fixture(kind);
        let page = fixture.get_pages()[&1];
        fixture
            .get_dictionary_mut(page)
            .unwrap()
            .set("CropBox", vec![10.into(), 20.into(), 90.into(), 70.into()]);
        fixture.get_dictionary_mut(page).unwrap().set("Rotate", 90);
        fixture.get_dictionary_mut(page).unwrap().set("UserUnit", 1);
        fixture.save(&path).unwrap();
        let worker = PdfWorker::new().unwrap();
        let source = Source::File(path.clone());
        let page = request(&worker, source.clone(), 0).unwrap();
        assert_eq!(page.points, (50., 80.));
        if kind == "valid" {
            assert_region_matches_full_page(&worker, source.clone(), 1, page.points);
        }
        let rendered = request(&worker, source.clone(), 2).unwrap();
        let pixels = rendered.pixels.expect("cropped/rotated page raster");
        assert_eq!((pixels.width, pixels.height), (50, 80));
        let before = breeze_storage::fingerprint(&path).unwrap();
        let destination = root.path().join(format!("{kind}-export.pdf"));
        export_annotations(ExportRequest {
            source,
            source_fingerprint: before.clone(),
            destination: destination.clone(),
            annotations: vec![note()],
        })
        .unwrap();
        let reader = MuPdfDocument::open(destination.as_path()).unwrap();
        assert_eq!(reader.page_count().unwrap(), 2);
        let rendered = request(
            &PdfWorker::new().unwrap(),
            Source::File(destination.clone()),
            0,
        )
        .unwrap();
        assert!(rendered.pixels.is_some());
        assert_eq!(breeze_storage::fingerprint(&path).unwrap(), before);
        if let Some(dir) = std::env::var_os("BREEZE_RENDER_REVIEW_DIR") {
            std::fs::create_dir_all(&dir).unwrap();
            let dir = std::path::PathBuf::from(dir);
            std::fs::copy(&path, dir.join(format!("{kind}.pdf"))).unwrap();
            std::fs::copy(&destination, dir.join(format!("{kind}-export.pdf"))).unwrap();
        }
    }
}

fn note() -> Annotation {
    Annotation {
        page: 0,
        kind: Kind::Note,
        points: vec![Point::new(20., 20.)],
        quads: vec![],
        color: [0., 0., 0.],
        opacity: 1.,
        width: 1.,
        text: "Unit validation".into(),
    }
}

#[test]
fn reader_and_export_refuse_nondefault_or_invalid_leaf_units_on_every_request_path() {
    let root = tempfile::tempdir().unwrap();
    for (index, unit) in [
        Object::Integer(2),
        Object::Real(0.5),
        Object::Integer(0),
        Object::Integer(-1),
        Object::Name(b"wrong".to_vec()),
        Object::Null,
        Object::Reference((9999, 0)),
    ]
    .into_iter()
    .enumerate()
    {
        let path = root.path().join(format!("unit-{index}.pdf"));
        let mut doc = fixture("valid");
        let unit = if index % 2 == 0 && index < 5 {
            Object::Reference(doc.add_object(unit))
        } else {
            unit
        };
        let page = doc.get_pages()[&1];
        doc.get_dictionary_mut(page).unwrap().set("UserUnit", unit);
        doc.save(&path).unwrap();
        let source = Source::File(path.clone());
        let worker = PdfWorker::new().unwrap();
        for mode in 0..4 {
            assert!(
                request(&worker, source.clone(), mode).is_err(),
                "unit {index}/{mode}"
            );
        }
        let fingerprint = breeze_storage::fingerprint(&path).unwrap();
        let destination = root.path().join(format!("unit-{index}-export.pdf"));
        assert!(
            export_annotations(ExportRequest {
                source,
                source_fingerprint: fingerprint.clone(),
                destination: destination.clone(),
                annotations: vec![note()]
            })
            .is_err()
        );
        assert!(!destination.exists());
        assert_eq!(breeze_storage::fingerprint(&path).unwrap(), fingerprint);
        if let Some(dir) = std::env::var_os("BREEZE_RENDER_REVIEW_DIR") {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::copy(
                path,
                std::path::PathBuf::from(dir).join(format!("unit-{index}.pdf")),
            )
            .unwrap();
        }
    }
}

#[test]
fn user_unit_is_not_inherited_and_unit_one_does_not_rescale_coordinates() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("unit-one.pdf");
    let mut doc = fixture("valid");
    let page = doc.get_pages()[&1];
    let parent = doc
        .get_dictionary(page)
        .unwrap()
        .get(b"Parent")
        .unwrap()
        .as_reference()
        .unwrap();
    doc.get_dictionary_mut(parent).unwrap().set("UserUnit", 2);
    doc.save(&path).unwrap();
    let worker = PdfWorker::new().unwrap();
    assert_eq!(
        request(&worker, Source::File(path.clone()), 3)
            .unwrap()
            .points,
        (100., 100.)
    );
    let unit = doc.add_object(Object::Real(1.0));
    doc.get_dictionary_mut(page).unwrap().set("UserUnit", unit);
    doc.save(&path).unwrap();
    assert_eq!(
        request(&PdfWorker::new().unwrap(), Source::File(path), 0)
            .unwrap()
            .points,
        (100., 100.)
    );
}
