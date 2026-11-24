use super::*;
use lopdf::{Document, Object, Stream, dictionary};
use std::io::Read;

fn layout_fixture(path: &Path) {
    let mut document = Document::with_version("1.7");
    let pages = document.new_object_id();
    let font = document.add_object(
        dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" },
    );
    let bold = document.add_object(
        dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica-Bold" },
    );
    let image = document.add_object(Stream::new(dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => 3, "Height" => 2, "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8 }, vec![255,0,0, 0,255,0, 0,0,255, 255,255,0, 0,255,255, 255,0,255]));
    let resources = dictionary! { "Font" => dictionary! { "F1" => font, "F2" => bold }, "XObject" => dictionary! { "Im1" => image } };
    let content = b"BT /F2 22 Tf 50 730 Td (Layout title) Tj ET\nBT /F1 12 Tf 50 680 Td (Left column) Tj 0 -20 Td (Left second line) Tj ET\nBT /F1 12 Tf 330 680 Td (Right column) Tj 0 -20 Td (Right second line) Tj ET\nq 120 0 0 80 50 520 cm /Im1 Do Q\n0.5 w 50 380 400 100 re S 250 380 m 250 480 l S 50 430 m 450 430 l S\nBT /F2 12 Tf 65 455 Td (Name) Tj 200 0 Td (Value) Tj ET\nBT /F1 12 Tf 65 405 Td (Alpha) Tj 200 0 Td (42) Tj ET\n";
    let stream = document.add_object(Stream::new(dictionary! {}, content.to_vec()));
    let first = document.add_object(dictionary! { "Type" => "Page", "Parent" => pages, "MediaBox" => vec![0.into(),0.into(),595.into(),842.into()], "Resources" => resources.clone(), "Contents" => stream });
    let scan = document.add_object(Stream::new(
        dictionary! {},
        b"q 300 0 0 200 50 500 cm /Im1 Do Q".to_vec(),
    ));
    let second = document.add_object(dictionary! { "Type" => "Page", "Parent" => pages, "MediaBox" => vec![0.into(),0.into(),595.into(),842.into()], "Resources" => resources, "Contents" => scan });
    document.objects.insert(pages, dictionary! { "Type" => "Pages", "Kids" => vec![Object::Reference(first), Object::Reference(second)], "Count" => 2 }.into());
    let catalog = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
    document.trailer.set("Root", catalog);
    document.save(path).unwrap();
}

#[test]
fn layout_conversion_retains_columns_table_positions_images_and_scanned_pages() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("layout.pdf");
    layout_fixture(&path);
    let fingerprint = breeze_storage::fingerprint(&path).unwrap();
    let result = extract(&Source::File(path.clone()), &fingerprint).unwrap();
    assert_eq!(result.pages.len(), 2);
    assert_eq!(result.empty_pages, 1);
    assert!(result.images >= 2);
    let text = result.pages[0]
        .elements
        .iter()
        .filter_map(|element| {
            if let Element::Text { bounds, runs, .. } = element {
                Some((
                    bounds.x,
                    runs.iter().map(|run| run.text.as_str()).collect::<String>(),
                ))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert!(
        text.iter()
            .any(|(x, value)| *x < 100. && value.contains("Left column"))
    );
    assert!(
        text.iter()
            .any(|(x, value)| *x > 300. && value.contains("Right column"))
    );
    let word = directory.path().join("layout.docx");
    result.write_word(&word).unwrap();
    let mut zip = zip::ZipArchive::new(fs::File::open(&word).unwrap()).unwrap();
    let mut xml = String::new();
    zip.by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    for value in [
        "w:framePr",
        "w:x=\"6600\"",
        "w:pgSz",
        "w:b",
        "Left column",
        "Right column",
        "Alpha",
        "wp:anchor",
        "behindDoc=\"1\"",
        "allowOverlap=\"1\"",
        "w:ascii=\"Arial\"",
    ] {
        assert!(xml.contains(value), "missing {value}");
    }
    assert!(!xml.contains("Helvetica"));
    assert_eq!(
        zip.file_names()
            .map(Result::unwrap)
            .filter(|name| name.starts_with("word/media/") && !name.ends_with('/'))
            .count(),
        result.images,
        "every page image must be included in the Word package"
    );
    let assets = directory.path().join("assets");
    fs::create_dir(&assets).unwrap();
    let md = directory.path().join("layout.md");
    result.write_markdown(&md, &assets, "assets").unwrap();
    let source = fs::read_to_string(&md).unwrap();
    assert!(source.contains("Layout title"));
    assert!(source.contains("Right column"));
    assert!(source.find("Left second line").unwrap() < source.find("Right column").unwrap());
    assert!(source.contains("<!-- Page 2 -->"));
    assert!(source.contains("assets/image-"));
    if let Some(output) = std::env::var_os("BREEZE_EXPORT_QA_DIR") {
        let output = std::path::PathBuf::from(output);
        fs::create_dir_all(&output).unwrap();
        fs::copy(&path, output.join("layout.pdf")).unwrap();
        fs::copy(&word, output.join("layout.docx")).unwrap();
    }
}

#[test]
fn sample_exports_editable_positioned_text_and_markdown_assets() {
    let fingerprint = breeze_storage::bytes_fingerprint(include_bytes!(
        "../../../../assets/samples/field-notes.pdf"
    ));
    let result = extract(&Source::Sample, &fingerprint).unwrap();
    assert!(!result.pages.is_empty());
    assert!(result.pages.iter().flat_map(|page| &page.elements).any(|element| matches!(element, Element::Text { runs, .. } if runs.iter().any(|run| !run.text.trim().is_empty()))));
    assert!(extract(&Source::Sample, "changed").is_err());
    let directory = tempfile::tempdir().unwrap();
    result
        .write_word(&directory.path().join("result.docx"))
        .unwrap();
    result
        .write_markdown(
            &directory.path().join("result.md"),
            directory.path(),
            "assets",
        )
        .unwrap();
    let source = fs::read_to_string(directory.path().join("result.md")).unwrap();
    assert!(source.contains("<!-- Page 1 -->"));
    assert_eq!(escape_markdown("# * 中文"), "\\# \\* 中文");
}
