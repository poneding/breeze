mod common;
use breeze_core::document::EpubLocation;
use breeze_reflow::epub::{Book, Request};

#[test]
fn publication_navigation_reflow_pixels_and_source_integrity() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("book.epub");
    let bytes = common::publication(3, 12);
    std::fs::write(&path, &bytes).unwrap();
    let mut book = Book::open(&path).unwrap();
    assert_eq!(book.metadata.chapters.len(), 3);
    assert_eq!(book.metadata.outline.len(), 3);
    let mut request = Request {
        path: path.clone(),
        location: EpubLocation::default(),
        width: 600.,
        height: 800.,
        scale: 1.,
        font_size: 18.,
        turn: 0,
        scroll: 0.,
        find: None,
    };
    let first = book.render(&request).unwrap();
    assert_eq!(first.chapter, 0);
    assert!(first.text.contains("Chapter 0"));
    assert!(first.chapter_pages > 1);
    assert_eq!(
        first.bgra.len(),
        first.width as usize * first.height as usize * 4
    );
    assert!(
        first
            .bgra
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[2] > 100 && p[2] > p[0].saturating_add(40))
    );
    request.location.href = book.metadata.outline[2].href.clone();
    let last = book.render(&request).unwrap();
    assert_eq!(last.chapter, 2);
    assert!(last.text.contains("Chapter 2"));
    request.location = last.location;
    request.turn = -1;
    let previous = book.render(&request).unwrap();
    assert_eq!(previous.chapter, 1);
    assert_eq!(previous.chapter_page + 1, previous.chapter_pages);
    request.turn = 0;
    request.location = previous.location;
    request.font_size = 24.;
    let reflow = book.render(&request).unwrap();
    assert_eq!(reflow.chapter, 1);
    assert_eq!(reflow.location.progression, 1.);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    request.scale = 4.;
    request.width = 4096.;
    assert!(book.render(&request).is_err());
}

#[test]
fn malformed_input_is_reported() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("invalid.epub");
    std::fs::write(&path, b"not an EPUB").unwrap();
    assert!(Book::open(&path).is_err());
}

#[test]
fn search_reads_chapter_text_and_obeys_cancellation() {
    use std::sync::atomic::AtomicBool;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("book.epub");
    std::fs::write(&path, common::publication(3, 5)).unwrap();
    let (_, hits) =
        breeze_reflow::epub::search(&path, "CHAPTER 2", &AtomicBool::new(false)).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].href, "OEBPS/ch2.xhtml");
    assert!(breeze_reflow::epub::search(&path, "text", &AtomicBool::new(true)).is_err());
}

#[test]
fn encrypted_and_unsafe_packages_are_rejected_before_layout() {
    use std::io::{Cursor, Write};
    use zip::{ZipWriter, write::SimpleFileOptions};
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("restricted.epub");
    for name in ["META-INF/encryption.xml", "../outside.xhtml"] {
        let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
        archive
            .start_file(name, SimpleFileOptions::default())
            .unwrap();
        archive.write_all(b"restricted").unwrap();
        std::fs::write(&path, archive.finish().unwrap().into_inner()).unwrap();
        let error = match Book::open(&path) {
            Ok(_) => panic!("restricted package opened"),
            Err(error) => error,
        };
        assert!(
            error.to_string().contains("Encrypted") || error.to_string().contains("Unsafe"),
            "{error:#}"
        );
    }
}
