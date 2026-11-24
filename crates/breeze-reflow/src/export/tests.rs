use super::*;
use std::io::Read;

fn xml(path: &Path, entry: &str) -> String {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut text = String::new();
    archive
        .by_name(entry)
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    text
}

#[test]
fn markdown_exports_unicode_structure_styles_links_and_images() {
    let directory = tempfile::tempdir().unwrap();
    let source_path = directory.path().join("source.md");
    image::RgbaImage::from_pixel(24, 16, image::Rgba([255, 30, 30, 255]))
        .save(directory.path().join("image.png"))
        .unwrap();
    let source = "# 标题 Title\n\n**Bold** and *italic*, ~~strike~~, [link](https://example.com).\n\n1. First\n2. Second\n\n| Left | Right |\n| :--- | ---: |\n| 中文 | value |\n\n```rust\nlet x = 1;\n```\n\n![local](image.png)\n";
    let word = directory.path().join("result.docx");
    let document = prepare_markdown(source, &source_path).unwrap();
    assert_eq!(document.simplified, 0);
    document.write_word(&word).unwrap();
    let body = xml(&word, "word/document.xml");
    for value in [
        "标题 Title",
        "w:tbl",
        "w:numPr",
        "w:hyperlink",
        "w:b",
        "w:i",
        "w:drawing",
        "中文",
        "let x = 1;",
    ] {
        assert!(body.contains(value), "missing {value}");
    }
    assert!(xml(&word, "word/numbering.xml").contains("decimal"));
    assert!(xml(&word, "word/styles.xml").contains("Heading1"));
    let pdf = directory.path().join("result.pdf");
    prepare_markdown(source, &source_path)
        .unwrap()
        .write_pdf(&pdf)
        .unwrap();
    let document = mupdf::pdf::PdfDocument::open(pdf.to_str().unwrap()).unwrap();
    let text = document
        .load_page(0)
        .unwrap()
        .to_text_page(mupdf::TextPageFlags::empty())
        .unwrap()
        .to_text()
        .unwrap();
    assert!(text.contains("标题"), "{text}");
    assert!(text.contains("Bold"));
    let pixels = document
        .load_page(0)
        .unwrap()
        .to_pixmap(
            &mupdf::Matrix::IDENTITY,
            &mupdf::Colorspace::device_rgb(),
            false,
            false,
        )
        .unwrap();
    assert!(
        pixels
            .samples()
            .as_chunks::<3>()
            .0
            .iter()
            .any(|pixel| pixel[0] > 220 && pixel[1] < 80 && pixel[2] < 80),
        "the local image must survive PDF export"
    );
}

#[test]
fn special_constructs_are_disclosed_and_bad_assets_fail() {
    let directory = tempfile::tempdir().unwrap();
    let source_path = directory.path().join("source.md");
    let document = prepare_markdown(
        "$$\nx^2\n$$\n\n```mermaid\ngraph TD; A-->B;\n```\n\n![remote](https://example.com/a.png)",
        &source_path,
    )
    .unwrap();
    assert_eq!(document.simplified, 3);
    assert!(document.html.contains("x^2"));
    assert!(prepare_markdown("![missing](missing.png)", &source_path).is_err());
    assert!(prepare_markdown(&"x".repeat(ast::MAX_PARSE_BYTES + 1), &source_path).is_err());
}
