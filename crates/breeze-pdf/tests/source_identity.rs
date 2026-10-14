//! The worker reloads a document when its source file is replaced, even if the
//! replacement keeps the same size and modification time.
use breeze_pdf::{PdfWorker, Source};
use lopdf::{Document, Object, Stream, dictionary};
use std::time::{Duration, SystemTime};

fn document(blue: &str) -> Document {
    let mut doc = Document::with_version("1.7");
    let pages = doc.new_object_id();
    let content = doc.add_object(Stream::new(
        dictionary! {},
        format!("{blue} 0.4 0.6 rg 10 10 80 80 re f").into_bytes(),
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
    let root = doc.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages});
    doc.trailer.set("Root", root);
    doc
}

fn fingerprint(worker: &PdfWorker, path: &std::path::Path) -> String {
    futures::executor::block_on(worker.request(Source::File(path.to_path_buf()), 0).unwrap())
        .unwrap()
        .unwrap()
        .fingerprint
}

#[test]
fn a_replaced_source_is_reloaded_despite_identical_size_and_mtime() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("document.pdf");
    let replacement = dir.path().join("replacement.pdf");
    let original = dir.path().join("original.pdf");

    // Two documents of identical length that differ only in one content byte.
    document("0.2").save(&original).unwrap();
    document("0.3").save(&replacement).unwrap();
    let original_bytes = std::fs::read(&original).unwrap();
    let replacement_bytes = std::fs::read(&replacement).unwrap();
    assert_eq!(
        original_bytes.len(),
        replacement_bytes.len(),
        "the fixtures must have equal length"
    );
    assert_ne!(original_bytes, replacement_bytes);

    std::fs::copy(&original, &path).unwrap();
    let stamp = SystemTime::now() - Duration::from_secs(3_600);
    let stamp_file = |file: &std::path::Path| {
        let times = std::fs::FileTimes::new().set_modified(stamp);
        std::fs::OpenOptions::new()
            .write(true)
            .open(file)
            .unwrap()
            .set_times(times)
            .unwrap();
    };
    stamp_file(&path);
    stamp_file(&replacement);

    let worker = PdfWorker::new().unwrap();
    let first = fingerprint(&worker, &path);
    let before = std::fs::metadata(&path).unwrap().modified().unwrap();

    // Replace the file through rename, the way an editor or installer does.
    std::fs::rename(&replacement, &path).unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().modified().unwrap(),
        before,
        "the replacement preserves the modification time"
    );
    assert_eq!(
        std::fs::metadata(&path).unwrap().len(),
        original_bytes.len() as u64
    );

    let second = fingerprint(&worker, &path);
    assert_ne!(
        first, second,
        "a replaced source file must be reloaded instead of served from cache"
    );
}
