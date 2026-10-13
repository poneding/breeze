use breeze_core::{
    Rotation,
    annotations::{Annotation, Kind, Point},
};
use breeze_pdf::{
    PdfWorker, Source,
    export::{ExportRequest, export_annotations},
};
use lopdf::{Document, Object, Stream, dictionary};
use pdf_oxide::{
    PdfDocument,
    rendering::{PageRenderer, RenderOptions},
};

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
        _ => worker.request_metadata(source, 0),
    }
    .unwrap();
    futures::executor::block_on(receiver).unwrap()
}

/// The engine a stock dependency gives us logs a failed image, mask, Form or
/// shading draw and returns a successful raster with that content missing, so a
/// page request cannot detect it. This test states the requirement it does not
/// currently meet: it passed against the retired `vendor/pdf_oxide` copy, whose
/// rendering-integrity change made those draws fail loudly. Re-enable it when
/// that change reaches an upstream release, or when the page path can tell an
/// incomplete raster from a complete one.
#[ignore = "requires the retired engine patch that propagates draw failures"]
#[test]
fn drawing_failures_never_return_successful_pixels_and_do_not_poison_next_page() {
    let root = tempfile::tempdir().unwrap();
    for kind in [
        "image",
        "image-mask",
        "truncated-mask",
        "inline-image",
        "inline-mask",
        "form",
        "cyclic-form",
        "smask",
        "mask",
        "missing",
        "shading",
    ] {
        let path = root.path().join(format!("{kind}.pdf"));
        fixture(kind).save(&path).unwrap();
        let source = Source::File(path.clone());
        let before = std::fs::read(&path).unwrap();
        let worker = PdfWorker::new().unwrap();
        assert!(
            request(&worker, source.clone(), 3).is_ok(),
            "{kind}: metadata remains available"
        );
        for mode in 0..3 {
            match request(&worker, source.clone(), mode) {
                Ok(_) => panic!("{kind}/{mode}: published incomplete pixels"),
                Err(error) => assert!(
                    format!("{error:#}").contains("第 1 页渲染失败"),
                    "{kind}: {error:#}"
                ),
            }
        }
        let good = futures::executor::block_on(worker.request(source, 1).unwrap())
            .unwrap()
            .unwrap();
        assert!(good.pixels.is_some());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        if let Some(dir) = std::env::var_os("BREEZE_RENDER_REVIEW_DIR") {
            let dir = std::path::PathBuf::from(dir);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::copy(path, dir.join(format!("{kind}.pdf"))).unwrap();
        }
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
            .set("CropBox", vec![10.into(), 10.into(), 90.into(), 90.into()]);
        fixture.get_dictionary_mut(page).unwrap().set("Rotate", 90);
        fixture.get_dictionary_mut(page).unwrap().set("UserUnit", 1);
        fixture.save(&path).unwrap();
        let worker = PdfWorker::new().unwrap();
        let source = Source::File(path.clone());
        let page = request(&worker, source.clone(), 0).unwrap();
        assert_eq!(page.points, (80., 80.));
        let before = breeze_storage::fingerprint(&path).unwrap();
        let destination = root.path().join(format!("{kind}-export.pdf"));
        export_annotations(ExportRequest {
            source,
            source_fingerprint: before.clone(),
            destination: destination.clone(),
            annotations: vec![note()],
        })
        .unwrap();
        let result = PdfDocument::open(&destination).unwrap();
        PageRenderer::new(RenderOptions::with_dpi(72).as_raw())
            .render_page(&result, 0)
            .unwrap();
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
