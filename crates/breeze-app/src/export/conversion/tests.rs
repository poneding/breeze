use super::*;
use std::io::Read;

#[test]
fn conversion_preserves_sources_existing_files_and_conflicts() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("source.md");
    let original = "# Saved source\n\nBody 中文\n";
    fs::write(&path, original).unwrap();
    let fingerprint = breeze_storage::fingerprint(&path).unwrap();
    let request = |destination: PathBuf, format| ConversionRequest {
        source: Source::File(path.clone()),
        snapshot: DocumentSnapshot::Markdown {
            text: "# Unsaved draft 中文\n".into(),
            fingerprint: fingerprint.clone(),
        },
        format,
        destination,
    };
    let output = directory.path().join("export.docx");
    let receipt = convert(request(output.clone(), ExportFormat::Word)).unwrap();
    assert!(receipt.bytes > 0);
    let mut archive = zip::ZipArchive::new(fs::File::open(&output).unwrap()).unwrap();
    let mut xml = String::new();
    archive
        .by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    assert!(xml.contains("Unsaved draft 中文"));
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    let existing = fs::read(&output).unwrap();
    assert!(convert(request(output.clone(), ExportFormat::Word)).is_err());
    assert_eq!(fs::read(&output).unwrap(), existing);
    assert!(convert(request(path.clone(), ExportFormat::Pdf)).is_err());
    assert!(
        convert(request(
            directory.path().join("wrong.md"),
            ExportFormat::Word
        ))
        .is_err()
    );
    fs::write(&path, "Changed externally").unwrap();
    let new_output = directory.path().join("conflict.pdf");
    assert!(convert(request(new_output.clone(), ExportFormat::Pdf)).is_err());
    assert!(!new_output.exists());
}

#[test]
fn pdf_converts_to_word_and_markdown_with_its_image_directory() {
    let directory = tempfile::tempdir().unwrap();
    let fingerprint = breeze_storage::bytes_fingerprint(include_bytes!(
        "../../../../../assets/samples/field-notes.pdf"
    ));
    for format in [ExportFormat::Word, ExportFormat::Markdown] {
        let output = directory
            .path()
            .join(format!("output.{}", format.extension()));
        let result = convert(ConversionRequest {
            source: Source::Sample,
            snapshot: DocumentSnapshot::Pdf {
                fingerprint: fingerprint.clone(),
            },
            format,
            destination: output.clone(),
        })
        .unwrap();
        assert!(result.bytes > 0);
        assert!(output.exists());
    }
    let md = fs::read_to_string(directory.path().join("output.md")).unwrap();
    for line in md.lines().filter(|line| line.starts_with("![")) {
        let link = line.split("](").nth(1).unwrap().trim_end_matches(')');
        assert!(directory.path().join(link).exists(), "missing image {link}");
    }
}
