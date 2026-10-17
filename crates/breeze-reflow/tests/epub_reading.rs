mod common;
use breeze_core::document::EpubLocation;
use breeze_reflow::epub::{Book, Request};

/// Budget for one page of Chinese prose.
///
/// The bound guards the regression that failed a font lookup per character and
/// cost over a second per page. A hosted Windows runner needs 4.01 s for this
/// fixture, so its bound is only loose enough to catch an order-of-magnitude
/// regression; `docs/plans/STATUS.md` records the measurement.
#[cfg(not(windows))]
const CJK_PAGE_BUDGET_MS: u128 = 500;
#[cfg(windows)]
const CJK_PAGE_BUDGET_MS: u128 = 5_000;

/// Whether the page text carries `phrase`.
///
/// MuPDF breaks a line wherever the column width requires, so the extracted text
/// holds a newline where the phrase had a space: in a half-width column
/// "Chapter 0" is drawn as "Chapter\n0". Comparing whitespace-normalized text
/// keeps this check about the text being present and selectable instead of about
/// where the layout happened to wrap, which depends on the fonts the host
/// provides. A dropped space cannot hide here, because the normalized text would
/// then read "Chapter0".
fn contains_phrase(text: &str, phrase: &str) -> bool {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .contains(phrase)
}

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
        spread: false,
    };
    let first = book.render(&request).unwrap();
    assert_eq!(first.chapter, 0);
    assert!(contains_phrase(&first.text, "Chapter 0"));
    assert!(first.chapter_pages > 1);
    assert_eq!(first.pages.len(), 1);
    assert_eq!(
        first.pages[0].bgra.len(),
        first.pages[0].width as usize * first.pages[0].height as usize * 4
    );
    assert!(
        first.pages[0]
            .bgra
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[2] > 100 && p[2] > p[0].saturating_add(40))
    );
    request.location.href = book.metadata.outline[2].href.clone();
    let last = book.render(&request).unwrap();
    assert_eq!(last.chapter, 2);
    assert!(contains_phrase(&last.text, "Chapter 2"));
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

/// A page delta must move that many pages: the reader turns several pages when a
/// fast flick crosses more than one, and treating the delta as a mere direction
/// silently dropped the rest.
#[test]
fn page_delta_moves_the_requested_number_of_pages() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("book.epub");
    std::fs::write(&path, common::publication(1, 60)).unwrap();
    let mut book = Book::open(&path).unwrap();
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
        spread: false,
    };
    let first = book.render(&request).unwrap();
    assert!(
        first.chapter_pages >= 4,
        "the fixture needs several pages, got {}",
        first.chapter_pages
    );
    request.location = first.location.clone();
    request.turn = 1;
    let one = book.render(&request).unwrap();
    assert_eq!(one.chapter_page, 1);
    request.location = first.location.clone();
    request.turn = 3;
    let three = book.render(&request).unwrap();
    assert_eq!(
        three.chapter_page, 3,
        "a three-page delta moved to page {}",
        three.chapter_page
    );
    request.location = three.location.clone();
    request.turn = -2;
    let back = book.render(&request).unwrap();
    assert_eq!(back.chapter_page, 1);
}

/// Two-page reading paginates the chapter at half the viewport and returns the
/// facing pair in one raster, so a single texture still covers a page turn. A
/// turn then advances a whole spread rather than one page.
#[test]
fn two_page_reading_returns_a_facing_pair() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("spread.epub");
    std::fs::write(&path, common::publication(1, 80)).unwrap();
    let mut book = Book::open(&path).unwrap();
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
        spread: false,
    };
    let single = book.render(&request).unwrap();
    request.spread = true;
    let spread = book.render(&request).unwrap();
    assert_eq!(spread.columns, 2);
    assert_eq!(spread.chapter_page, 0);
    assert_eq!(spread.pages.len(), 2);
    assert!(spread.chapter_pages > 2, "the fixture needs several pages");
    // The half-width column wraps this heading on hosts whose fonts are wider.
    assert!(contains_phrase(&spread.text, "Chapter 0"));
    // The pair fills the content area, so the combined raster keeps the
    // single-page size instead of doubling it.
    assert_eq!(spread.width, single.width);
    assert_eq!(spread.height, single.height);
    assert!(
        spread
            .pages
            .iter()
            .all(|page| { page.width * 2 == spread.width && page.height == spread.height })
    );
    // Half the width holds one page, so the chapter needs at least as many
    // pages as it did at the full width.
    assert!(
        spread.chapter_pages >= single.chapter_pages,
        "half-width pagination shrank the chapter from {} to {} pages",
        single.chapter_pages,
        spread.chapter_pages
    );

    request.location = spread.location.clone();
    request.turn = 1;
    let next = book.render(&request).unwrap();
    assert_eq!(
        next.chapter_page,
        spread.chapter_page + 2,
        "a turn moved one page instead of one spread"
    );
    assert_ne!(
        next.text, spread.text,
        "the second spread repeats the first"
    );
}

/// Chinese prose must actually be drawn. Without a CJK font every Han character
/// is a failed lookup: the page keeps only its Latin words and digits, and each
/// lookup costs about a millisecond, which is what made an EPUB look sparse and
/// turn pages slowly.
/// A spread request must be answered on its own terms: the worker caches whole
/// frames, and a cached single-page frame may never stand in for a two-page one.
#[test]
fn a_spread_request_is_answered_after_a_single_page_frame() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("spread.epub");
    std::fs::write(&path, common::publication(1, 40)).unwrap();
    let worker = breeze_reflow::EpubWorker::new().unwrap();
    let single_page = Request {
        path: path.clone(),
        location: EpubLocation::default(),
        width: 600.,
        height: 800.,
        scale: 2.,
        font_size: 18.,
        turn: 0,
        scroll: 0.,
        find: None,
        spread: false,
    };
    let first = futures::executor::block_on(worker.request(single_page.clone()).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(first.columns, 1);
    let mut two_page = single_page;
    two_page.spread = true;
    let second = futures::executor::block_on(worker.request(two_page).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(second.columns, 2);
    assert_eq!(second.pages.len(), 2);
}

#[test]
fn chinese_prose_is_drawn_with_its_own_stylesheet() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("chinese.epub");
    std::fs::write(&path, common::chinese_publication(12)).unwrap();
    let mut book = Book::open(&path).unwrap();
    let request = Request {
        path: path.clone(),
        location: EpubLocation::default(),
        width: 1200.,
        height: 900.,
        scale: 1.,
        font_size: 18.,
        turn: 0,
        scroll: 0.,
        find: None,
        spread: false,
    };
    let start = std::time::Instant::now();
    let frame = book.render(&request).unwrap();
    let first = start.elapsed();

    // The text layer has the Chinese, and the raster has ink: a page whose CJK
    // glyphs were skipped is nearly blank.
    let cjk = frame
        .text
        .chars()
        .filter(|c| ('\u{4e00}'..='\u{9fff}').contains(c))
        .count();
    assert!(
        cjk > 200,
        "only {cjk} Han characters reached the text layer"
    );
    let total = frame.pages[0].width as usize * frame.pages[0].height as usize;
    let ink = frame.pages[0]
        .bgra
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| pixel[0] < 200 || pixel[1] < 200 || pixel[2] < 200)
        .count();
    let coverage = ink as f64 * 100. / total as f64;
    assert!(
        coverage > 3.,
        "the page is {coverage:.2}% ink: the Chinese glyphs were not drawn"
    );
    // The raster is opaque BGRA, so the reader composites no transparent page.
    assert!(
        frame.pages[0]
            .bgra
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| pixel[3] == 255),
        "the page raster is not opaque"
    );

    // Every page must be fast. A failed font lookup per character cost over a
    // second per page on the machine that caught it. The hosted Windows runner
    // needs 4.01 s for this fixture, so its bound only catches an
    // order-of-magnitude regression; docs/plans/STATUS.md records the gap.
    assert!(
        first.as_millis() < CJK_PAGE_BUDGET_MS,
        "the first page took {first:?}"
    );
    let mut turn = request.clone();
    turn.location = frame.location.clone();
    turn.turn = 1;
    let start = std::time::Instant::now();
    let _ = book.render(&turn).unwrap();
    let turned = start.elapsed();
    assert!(
        turned.as_millis() < CJK_PAGE_BUDGET_MS,
        "a page turn took {turned:?}"
    );
}

#[test]
fn rendered_pages_expose_selectable_text_geometry() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("book.epub");
    std::fs::write(&path, common::publication(1, 3)).unwrap();
    let mut book = Book::open(&path).unwrap();
    let frame = book
        .render(&Request {
            path: path.clone(),
            location: EpubLocation::default(),
            width: 600.,
            height: 800.,
            scale: 1.,
            font_size: 18.,
            turn: 0,
            scroll: 0.,
            find: None,
            spread: false,
        })
        .unwrap();
    assert_eq!(frame.text_pages.len(), 1);
    let text = &frame.text_pages[0];
    assert!(!text.is_empty());
    let line = &text.lines[0];
    let point = [
        (line.bounds[0] + line.bounds[2]) / 2.,
        (line.bounds[1] + line.bounds[3]) / 2.,
    ];
    let caret = text.caret(point).unwrap();
    assert!(line.glyphs.contains(&caret) || caret == line.glyphs.end);
    let selected = text.text(line.glyphs.clone());
    assert!(selected.contains("Chapter"), "{selected:?}");
    assert!(!text.rects(line.glyphs.clone()).is_empty());
}
