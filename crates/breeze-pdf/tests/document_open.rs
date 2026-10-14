//! Open-cost guard for the document-wide geometry pass.
//!
//! Rendering needs every page's view size up front, so loading a document
//! computes one geometry per page and flattens its outline. This test keeps that
//! pass bounded: it may not degrade into an opening stall as page counts grow.
use breeze_pdf::{PdfWorker, Source};
use lopdf::{Document, Object, Stream, dictionary};
use std::time::Instant;

fn build(pages: usize) -> Document {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let content = doc.add_object(Stream::new(
        dictionary! {},
        b"0.2 0.4 0.6 rg 10 10 80 80 re f".to_vec(),
    ));
    let mut kids = Vec::new();
    for index in 0..pages {
        // Vary the size so the pass cannot be satisfied by one shared page box.
        let width = 600 + (index % 5) as i64;
        let id = doc.add_object(dictionary! {
            "Type"=>"Page",
            "Parent"=>pages_id,
            "MediaBox"=>vec![0.into(),0.into(),width.into(),800.into()],
            "Resources"=>dictionary! {},
            "Contents"=>content,
        });
        kids.push(id.into());
    }
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {"Type"=>"Pages","Kids"=>kids,"Count"=>pages as i64}),
    );
    let root = doc.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages_id});
    doc.trailer.set("Root", root);
    doc
}

fn open_ms(path: std::path::PathBuf) -> (f64, usize) {
    let worker = PdfWorker::new().unwrap();
    let start = Instant::now();
    let page = futures::executor::block_on(worker.request_metadata(Source::File(path), 0).unwrap())
        .unwrap()
        .unwrap();
    (start.elapsed().as_secs_f64() * 1000., page.page_sizes.len())
}

#[test]
fn geometry_for_a_large_document_stays_bounded() {
    let root = tempfile::tempdir().unwrap();
    let mut previous = 0.;
    for pages in [100, 1_000, 20_000] {
        let path = root.path().join(format!("{pages}.pdf"));
        build(pages).save(&path).unwrap();
        let (elapsed, sizes) = open_ms(path);
        println!("pages={pages} open_ms={elapsed:.1} sizes={sizes}");
        assert_eq!(sizes, pages, "every page reports a size");
        assert!(
            elapsed < 2_000.,
            "{pages} pages must not stall opening: {elapsed:.1} ms"
        );
        if pages == 20_000 {
            assert!(
                elapsed > previous,
                "the 20k case should be the slowest measured case"
            );
        }
        previous = elapsed;
    }
}
