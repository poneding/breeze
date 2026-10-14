//! A malformed outline is truncated instead of stalling the reader or
//! recursing without bound.
use breeze_pdf::{PdfWorker, Source};
use lopdf::{Document, Object, Stream, dictionary};

fn deep_outline(levels: usize) -> Document {
    let mut doc = Document::with_version("1.7");
    let pages = doc.new_object_id();
    let content = doc.add_object(Stream::new(
        dictionary! {},
        b"0.2 0.4 0.6 rg 10 10 80 80 re f".to_vec(),
    ));
    let page = doc.add_object(dictionary! {
        "Type"=>"Page",
        "Parent"=>pages,
        "MediaBox"=>vec![0.into(),0.into(),100.into(),100.into()],
        "Resources"=>dictionary! {},
        "Contents"=>content,
    });
    doc.objects.insert(
        pages,
        Object::Dictionary(dictionary! {"Type"=>"Pages","Kids"=>vec![page.into()],"Count"=>1}),
    );

    // One outline item per level, each the only child of the previous one.
    let root = doc.new_object_id();
    let ids: Vec<_> = (0..levels).map(|_| doc.new_object_id()).collect();
    for (index, id) in ids.iter().enumerate() {
        let mut item = dictionary! {
            "Title"=>format!("Level {}", index + 1),
            "Parent"=>if index == 0 { root } else { ids[index - 1] },
            "Dest"=>vec![page.into(), "Fit".into()],
        };
        if let Some(child) = ids.get(index + 1) {
            item.set("First", *child);
            item.set("Last", *child);
            item.set("Count", (levels - index - 1) as i64);
        }
        doc.objects.insert(*id, Object::Dictionary(item));
    }
    doc.objects.insert(
        root,
        Object::Dictionary(dictionary! {
            "Type"=>"Outlines",
            "First"=>ids[0],
            "Last"=>ids[0],
            "Count"=>levels as i64,
        }),
    );
    let catalog = doc.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages,"Outlines"=>root});
    doc.trailer.set("Root", catalog);
    doc
}

#[test]
fn a_deeply_nested_outline_is_truncated_at_the_depth_bound() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("deep-outline.pdf");
    deep_outline(80).save(&path).unwrap();
    let worker = PdfWorker::new().unwrap();
    let page = futures::executor::block_on(
        worker
            .request_metadata(Source::File(path), 0)
            .expect("the worker accepts the request"),
    )
    .expect("the request completes")
    .expect("the outline must not fail the document");

    assert!(!page.outline.is_empty(), "the outline is still available");
    assert!(
        page.outline.len() <= 33,
        "at most 32 nesting levels are retained, got {}",
        page.outline.len()
    );
    let deepest = page.outline.iter().map(|item| item.depth).max().unwrap();
    assert_eq!(deepest, 32, "the depth bound is explicit");
    assert!(
        page.outline
            .iter()
            .all(|item| item.title.chars().count() <= 512),
        "titles stay bounded"
    );
}
