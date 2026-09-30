use super::*;
use breeze_core::annotations::{Point, Rect};
use std::sync::{Arc, atomic::AtomicBool};

fn sample_request(destination: PathBuf, annotations: Vec<Annotation>) -> ExportRequest {
    ExportRequest {
        source: Source::Sample,
        source_fingerprint: breeze_storage::bytes_fingerprint(
            &read_source(&Source::Sample).unwrap(),
        ),
        annotations,
        destination,
    }
}
fn annotation(kind: Kind, first: Point, last: Point) -> Annotation {
    Annotation {
        page: 0,
        kind,
        points: vec![first, last],
        quads: vec![],
        color: [0.85, 0.28, 0.18],
        opacity: 0.8,
        width: 2.,
        text: String::new(),
    }
}
fn all_kinds() -> Vec<Annotation> {
    let mut items = Vec::new();
    for (kind, rect) in [
        (
            Kind::Highlight,
            Rect {
                x: 54.,
                y: 620.,
                width: 460.,
                height: 38.,
            },
        ),
        (
            Kind::Underline,
            Rect {
                x: 54.,
                y: 298.,
                width: 442.,
                height: 12.,
            },
        ),
        (
            Kind::Strikeout,
            Rect {
                x: 54.,
                y: 260.,
                width: 448.,
                height: 12.,
            },
        ),
    ] {
        let mut a = annotation(
            kind,
            Point::new(rect.x, rect.y),
            Point::new(rect.x + rect.width, rect.y + rect.height),
        );
        a.quads = vec![rect];
        if kind == Kind::Highlight {
            a.opacity = 0.25;
            a.color = [1., 0.75, 0.1];
        }
        items.push(a);
    }
    let mut area = annotation(Kind::Area, Point::new(350., 440.), Point::new(460., 500.));
    area.opacity = 0.3;
    items.push(area);
    let mut ink = annotation(Kind::Ink, Point::new(75., 370.), Point::new(170., 360.));
    ink.points.insert(1, Point::new(120., 380.));
    items.push(ink);
    items.push(annotation(
        Kind::Ellipse,
        Point::new(75., 70.),
        Point::new(125., 105.),
    ));
    items.push(annotation(
        Kind::Rectangle,
        Point::new(180., 70.),
        Point::new(245., 105.),
    ));
    items.push(annotation(
        Kind::Line,
        Point::new(285., 82.),
        Point::new(340., 101.),
    ));
    items.push(annotation(
        Kind::Arrow,
        Point::new(375., 84.),
        Point::new(430., 110.),
    ));
    items.push(annotation(
        Kind::Squiggle,
        Point::new(455., 85.),
        Point::new(535., 102.),
    ));
    let mut note = annotation(Kind::Note, Point::new(530., 680.), Point::new(530., 680.));
    note.text = "中文便笺 · Unicode 🦀\nSecond line: keep every character.".into();
    items.push(note);
    items
}
#[test]
fn all_annotation_kinds_preserve_original_content_and_unicode() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("annotations.pdf");
    let items = all_kinds();
    let receipt = export_annotations(sample_request(output.clone(), items.clone())).unwrap();
    assert_eq!(receipt.annotations, 11);
    assert_eq!(receipt.sticky_notes, 1);
    assert!(receipt.bytes > 0);
    let source = Document::load_mem(&read_source(&Source::Sample).unwrap()).unwrap();
    let written = Document::load(&output).unwrap();
    for (page, id) in source.get_pages() {
        assert_eq!(
            source.get_page_content(id),
            written.get_page_content(written.get_pages()[&page])
        );
    }
    let annotations = written
        .get_page_annotations(written.get_pages()[&1])
        .unwrap();
    let subtypes: Vec<_> = annotations
        .iter()
        .map(|a| a.get(b"Subtype").unwrap().as_name().unwrap())
        .collect();
    assert_eq!(
        subtypes,
        vec![
            b"Highlight".as_slice(),
            b"Underline",
            b"StrikeOut",
            b"Square",
            b"Ink",
            b"Circle",
            b"Square",
            b"Line",
            b"Line",
            b"Ink",
            b"Text"
        ]
    );
    let note = annotations
        .last()
        .unwrap()
        .get(b"Contents")
        .unwrap()
        .as_str()
        .unwrap();
    assert_eq!(&note[..2], &[0xfe, 0xff]);
    let units: Vec<_> = note[2..]
        .chunks_exact(2)
        .map(|p| u16::from_be_bytes([p[0], p[1]]))
        .collect();
    assert_eq!(
        String::from_utf16(&units).unwrap(),
        items.last().unwrap().text
    );
    let reader = pdf_oxide::PdfDocument::open(&output).unwrap();
    let original =
        pdf_oxide::PdfDocument::from_bytes(read_source(&Source::Sample).unwrap()).unwrap();
    assert_eq!(
        reader.extract_text(0).unwrap(),
        original.extract_text(0).unwrap()
    );
    // The current reader decodes annotation Contents as UTF-8 even for UTF-16 PDF strings.
    // Keep the standard encoding; exact Unicode is checked above and by an external parser.
    assert_eq!(
        reader
            .get_annotations(0)
            .unwrap()
            .last()
            .unwrap()
            .subtype
            .as_deref(),
        Some("Text")
    );
    assert_eq!(
        reader.get_outline().unwrap().unwrap()[0].title,
        original.get_outline().unwrap().unwrap()[0].title
    );
    let results = super::super::search(
        Source::File(output),
        "quiet art of focus".into(),
        Arc::new(AtomicBool::new(false)),
    )
    .unwrap();
    assert_eq!(results[0].page, 0);
}
#[test]
fn existing_and_indirect_shared_annotation_arrays_survive() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("source.pdf");
    let output = dir.path().join("copy.pdf");
    let mut doc = Document::load_mem(&read_source(&Source::Sample).unwrap()).unwrap();
    let pages = doc.get_pages();
    let existing=doc.add_object(dictionary!{"Type"=>"Annot","Subtype"=>"Link","Rect"=>appearance::array([30.,30.,60.,45.]),"Contents"=>Object::string_literal("Existing link"),"Dest"=>vec![Object::Reference(pages[&2]),Object::Name(b"Fit".to_vec())]});
    let shared = doc.add_object(Object::Array(vec![existing.into()]));
    for page in [pages[&1], pages[&2]] {
        doc.get_dictionary_mut(page).unwrap().set("Annots", shared);
    }
    doc.save(&input).unwrap();
    let fingerprint = breeze_storage::fingerprint(&input).unwrap();
    export_annotations(ExportRequest {
        source: Source::File(input.clone()),
        source_fingerprint: fingerprint.clone(),
        annotations: all_kinds(),
        destination: output.clone(),
    })
    .unwrap();
    let written = Document::load(output).unwrap();
    assert_eq!(written.get_page_annotations(pages[&1]).unwrap().len(), 12);
    assert_eq!(written.get_page_annotations(pages[&2]).unwrap().len(), 1);
    assert_eq!(breeze_storage::fingerprint(&input).unwrap(), fingerprint);
}
#[test]
fn rejects_original_existing_and_stale_destinations_without_overwriting() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("original.pdf");
    let bytes = read_source(&Source::Sample).unwrap();
    fs::write(&input, &bytes).unwrap();
    let mut request = sample_request(input.clone(), all_kinds());
    request.source = Source::File(input.clone());
    assert!(export_annotations(request).is_err());
    assert_eq!(fs::read(&input).unwrap(), bytes);
    let output = dir.path().join("existing.pdf");
    fs::write(&output, b"keep me").unwrap();
    assert!(export_annotations(sample_request(output.clone(), all_kinds())).is_err());
    assert_eq!(fs::read(output).unwrap(), b"keep me");
    let output = dir.path().join("stale.pdf");
    let mut request = sample_request(output.clone(), all_kinds());
    request.source_fingerprint = "outdated".into();
    assert!(export_annotations(request).is_err());
    assert!(!output.exists());
}
#[test]
fn invalid_geometry_page_or_style_never_publishes() {
    let dir = tempfile::tempdir().unwrap();
    for index in 0..4 {
        let output = dir.path().join(format!("bad-{index}.pdf"));
        let mut a = all_kinds().remove(0);
        match index {
            0 => a.page = 99,
            1 => a.opacity = f32::NAN,
            2 => a.quads.clear(),
            _ => a.points.clear(),
        };
        assert!(export_annotations(sample_request(output.clone(), vec![a])).is_err());
        assert!(!output.exists());
    }
}
#[test]
fn rejects_signed_forms_and_nondefault_user_unit() {
    let dir = tempfile::tempdir().unwrap();
    for kind in ["signature", "form", "widget", "unit", "encryption"] {
        let input = dir.path().join(format!("{kind}.pdf"));
        let output = dir.path().join(format!("{kind}-out.pdf"));
        let mut doc = Document::load_mem(&read_source(&Source::Sample).unwrap()).unwrap();
        match kind {
            "signature" => {
                doc.add_object(dictionary!{"Type"=>"Sig","ByteRange"=>vec![0.into(),1.into(),2.into(),3.into()]});
            }
            "form" => doc
                .catalog_mut()
                .unwrap()
                .set("AcroForm", dictionary! {"Fields"=>Object::Array(vec![])}),
            "widget" => {
                doc.add_object(dictionary! {"Type"=>"Annot","Subtype"=>"Widget"});
            }
            "unit" => {
                let id = doc.get_pages()[&1];
                doc.get_dictionary_mut(id).unwrap().set("UserUnit", 2);
            }
            _ => {
                doc.trailer
                    .set("Encrypt", dictionary! {"Filter"=>"Standard"});
            }
        }
        doc.save(&input).unwrap();
        let hash = breeze_storage::fingerprint(&input).unwrap();
        assert!(
            export_annotations(ExportRequest {
                source: Source::File(input),
                source_fingerprint: hash,
                annotations: all_kinds(),
                destination: output.clone()
            })
            .is_err()
        );
        assert!(!output.exists());
    }
}
#[test]
fn rotated_and_cropped_page_geometry_is_preserved() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("rotated.pdf");
    let output = dir.path().join("rotated-export.pdf");
    let mut doc = Document::load_mem(&read_source(&Source::Sample).unwrap()).unwrap();
    let id = doc.get_pages()[&1];
    doc.get_dictionary_mut(id).unwrap().set("Rotate", 90);
    doc.get_dictionary_mut(id)
        .unwrap()
        .set("CropBox", appearance::array([20., 20., 580., 760.]));
    doc.save(&input).unwrap();
    export_annotations(ExportRequest {
        source: Source::File(input.clone()),
        source_fingerprint: breeze_storage::fingerprint(&input).unwrap(),
        annotations: all_kinds(),
        destination: output.clone(),
    })
    .unwrap();
    let result = Document::load(output).unwrap();
    assert_eq!(
        result
            .get_dictionary(id)
            .unwrap()
            .get(b"Rotate")
            .unwrap()
            .as_i64()
            .unwrap(),
        90
    );
    assert_eq!(
        result
            .get_dictionary(id)
            .unwrap()
            .get(b"CropBox")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_float().unwrap())
            .collect::<Vec<_>>(),
        vec![20., 20., 580., 760.]
    );
}
#[test]
fn object_signature_detects_stream_mutations_but_ignores_storage_positions() {
    let mut stream = lopdf::Stream::new(dictionary! {}, b"BT /F1 10 Tf (same) Tj ET".to_vec());
    let hash = signature(&Object::Stream(stream.clone()), false).unwrap();
    stream.start_position = Some(500);
    assert_eq!(
        signature(&Object::Stream(stream.clone()), false).unwrap(),
        hash
    );
    stream.content.push(b' ');
    assert_ne!(signature(&Object::Stream(stream), false).unwrap(), hash);
}
#[test]
#[ignore = "Writes fresh PDFs below target/export-review for independent visual inspection"]
fn write_visual_export_fixtures() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/export-review");
    fs::create_dir_all(&root).unwrap();
    // Unique subdirectories make repeated verification non-destructive and preserve prior results.
    let folder = tempfile::Builder::new()
        .prefix("run-")
        .tempdir_in(&root)
        .unwrap()
        .keep();
    let output = folder.join("annotations.pdf");
    let receipt = export_annotations(sample_request(output.clone(), all_kinds())).unwrap();
    let source = folder.join("source.pdf");
    fs::write(&source, read_source(&Source::Sample).unwrap()).unwrap();
    let doc = pdf_oxide::PdfDocument::open(&output).unwrap();
    let mut renderer =
        pdf_oxide::rendering::PageRenderer::new(pdf_oxide::rendering::RenderOptions::with_dpi(144));
    renderer
        .render_page(&doc, 0)
        .unwrap()
        .save(folder.join("oxide-annotated.png"))
        .unwrap();
    println!(
        "FIXTURE_DIRECTORY={} bytes={} elapsed_ms={:.2}",
        folder.display(),
        receipt.bytes,
        receipt.elapsed_ms
    );
}

#[cfg(unix)]
#[test]
fn aliases_and_dangling_symlinks_are_not_export_targets() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("source.pdf");
    fs::write(&source, read_source(&Source::Sample).unwrap()).unwrap();
    let original = breeze_storage::fingerprint(&source).unwrap();
    let hardlink = folder.path().join("hardlink.pdf");
    fs::hard_link(&source, &hardlink).unwrap();
    let symlink = folder.path().join("symlink.pdf");
    std::os::unix::fs::symlink(&source, &symlink).unwrap();
    let dangling = folder.path().join("dangling.pdf");
    std::os::unix::fs::symlink(folder.path().join("absent.pdf"), &dangling).unwrap();
    for destination in [hardlink, symlink, dangling] {
        assert!(
            export_annotations(ExportRequest {
                source: Source::File(source.clone()),
                source_fingerprint: original.clone(),
                annotations: all_kinds(),
                destination
            })
            .is_err()
        );
    }
    assert_eq!(breeze_storage::fingerprint(&source).unwrap(), original);
    assert!(!folder.path().join("absent.pdf").exists());
}

#[test]
fn sparse_object_identifiers_are_rejected_before_writing() {
    let mut doc = Document::load_mem(&read_source(&Source::Sample).unwrap()).unwrap();
    doc.max_id = u32::MAX;
    assert!(preflight(&doc).is_err());
}

#[test]
fn old_pdf_header_is_upgraded_for_transparent_appearances() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("legacy.pdf");
    let destination = folder.path().join("updated.pdf");
    let mut doc = Document::with_version("1.2");
    let pages = doc.new_object_id();
    let font =
        doc.add_object(dictionary! {"Type"=>"Font","Subtype"=>"Type1","BaseFont"=>"Helvetica"});
    let content = doc.add_object(lopdf::Stream::new(
        dictionary! {},
        b"BT /F1 12 Tf 40 40 Td (Old PDF) Tj ET".to_vec(),
    ));
    let page=doc.add_object(dictionary!{"Type"=>"Page","Parent"=>pages,"MediaBox"=>appearance::array([0.,0.,612.,792.]),"Resources"=>dictionary!{"Font"=>dictionary!{"F1"=>font}},"Contents"=>content});
    doc.objects.insert(
        pages,
        Object::Dictionary(
            dictionary! {"Type"=>"Pages","Kids"=>vec![Object::Reference(page)],"Count"=>1},
        ),
    );
    let root = doc.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages});
    doc.trailer.set("Root", root);
    doc.save(&source).unwrap();
    let original = breeze_storage::fingerprint(&source).unwrap();
    export_annotations(ExportRequest {
        source: Source::File(source.clone()),
        source_fingerprint: original.clone(),
        annotations: all_kinds(),
        destination: destination.clone(),
    })
    .unwrap();
    assert_eq!(Document::load(&destination).unwrap().version, "1.7");
    assert_eq!(breeze_storage::fingerprint(&source).unwrap(), original);
    assert!(
        pdf_oxide::PdfDocument::open(&destination)
            .unwrap()
            .extract_text(0)
            .unwrap()
            .contains("Old PDF")
    );
}

#[test]
fn geometry_preflight_checks_inherited_boxes_and_rotation() {
    let mut doc = Document::load_mem(&read_source(&Source::Sample).unwrap()).unwrap();
    let page = doc.get_pages()[&1];
    let parent = doc
        .get_dictionary(page)
        .unwrap()
        .get(b"Parent")
        .unwrap()
        .as_reference()
        .unwrap();
    doc.get_dictionary_mut(page).unwrap().remove(b"MediaBox");
    doc.get_dictionary_mut(parent)
        .unwrap()
        .set("MediaBox", appearance::array([-20., -30., 612., 792.]));
    doc.get_dictionary_mut(parent).unwrap().set("Rotate", 90);
    assert!(preflight(&doc).is_ok());
    doc.get_dictionary_mut(page).unwrap().set("Rotate", 45);
    assert!(preflight(&doc).is_err());
    doc.get_dictionary_mut(page).unwrap().remove(b"Rotate");
    doc.get_dictionary_mut(page)
        .unwrap()
        .set("CropBox", appearance::array([700., 800., 900., 1000.]));
    assert!(preflight(&doc).is_err());
}
