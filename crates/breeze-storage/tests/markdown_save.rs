//! P4 coverage: Markdown source publication and recovery records.
use breeze_storage::markdown::{self, Draft, Target};
use std::path::Path;

fn draft(document: &Path, source: &str) -> Draft {
    Draft {
        version: 1,
        document: document.to_string_lossy().into_owned(),
        target: None,
        baseline: "baseline".into(),
        published: None,
        dirty: true,
        selection: (0, 0),
        saved_at: 0,
        source: source.into(),
    }
}

#[test]
fn a_new_destination_is_published_without_clobbering() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("notes.md");
    let fingerprint = markdown::publish(&path, b"# Title\n", &Target::New).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "# Title\n");
    assert_eq!(fingerprint, breeze_storage::bytes_fingerprint(b"# Title\n"));

    let error = markdown::publish(&path, b"other\n", &Target::New).unwrap_err();
    assert!(error.to_string().contains("already exists"), "{error:#}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "# Title\n");
}

#[test]
fn replacing_requires_the_fingerprint_the_draft_started_from() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("notes.md");
    std::fs::write(&path, b"one\n").unwrap();
    let loaded = markdown::load(&path).unwrap();
    assert_eq!(loaded.bytes, b"one\n");

    let target = Target::Existing {
        fingerprint: loaded.fingerprint.clone(),
    };
    markdown::publish(&path, b"one\ntwo\n", &target).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "one\ntwo\n");

    let error = markdown::publish(&path, b"three\n", &target).unwrap_err();
    assert!(error.to_string().contains("changed on disk"), "{error:#}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "one\ntwo\n");

    let current = markdown::load(&path).unwrap();
    markdown::publish(
        &path,
        b"three\n",
        &Target::Existing {
            fingerprint: current.fingerprint,
        },
    )
    .unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "three\n");
}

#[test]
fn loading_hashes_the_bytes_it_reads() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("notes.md");
    std::fs::write(&path, "中文 draft\n".as_bytes()).unwrap();
    let loaded = markdown::load(&path).unwrap();
    assert_eq!(
        loaded.fingerprint,
        breeze_storage::bytes_fingerprint(&loaded.bytes)
    );
    assert_eq!(std::str::from_utf8(&loaded.bytes).unwrap(), "中文 draft\n");

    std::fs::write(&path, [0xff, 0xfe]).unwrap();
    assert!(markdown::load(&path).is_err());
    std::fs::write(&path, vec![b'a'; markdown::MAX_SOURCE_BYTES + 1]).unwrap();
    assert!(markdown::load(&path).is_err());
}

#[test]
fn recovery_records_round_trip_and_prune_clean_records_first() {
    let root = tempfile::tempdir().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let document = directory.path().join("notes.md");
    let record = draft(&document, "# draft\n");

    markdown::write_recovery(root.path(), &record).unwrap();
    let read = markdown::read_recovery(root.path(), &document)
        .unwrap()
        .unwrap();
    assert_eq!(read.source, "# draft\n");
    assert!(read.dirty);
    assert!(read.is_valid());

    markdown::clear_recovery(root.path(), &document).unwrap();
    assert!(
        markdown::read_recovery(root.path(), &document)
            .unwrap()
            .is_none()
    );

    std::fs::write(
        markdown::recovery_path(root.path(), &document),
        b"{not json",
    )
    .unwrap();
    assert!(markdown::read_recovery(root.path(), &document).is_err());

    for index in 0..4 {
        let mut clean = draft(&directory.path().join(format!("doc{index}.md")), "clean\n");
        clean.dirty = false;
        markdown::write_recovery(root.path(), &clean).unwrap();
    }
    markdown::write_recovery(root.path(), &record).unwrap();
    let total = markdown::prune_recovery(root.path(), &document).unwrap();
    assert!(total > 0);
    assert!(
        markdown::read_recovery(root.path(), &document)
            .unwrap()
            .is_some(),
        "the protected draft survives"
    );
}

#[test]
fn an_invalid_record_is_refused_before_it_is_written() {
    let root = tempfile::tempdir().unwrap();
    let document = Path::new("/tmp/notes.md");
    let mut invalid = draft(document, "x");
    invalid.version = 99;
    assert!(markdown::write_recovery(root.path(), &invalid).is_err());
    assert!(
        markdown::read_recovery(root.path(), document)
            .unwrap()
            .is_none(),
        "nothing was written"
    );
}

#[cfg(unix)]
#[test]
fn a_symlink_destination_is_refused() {
    let directory = tempfile::tempdir().unwrap();
    let real = directory.path().join("real.md");
    let link = directory.path().join("link.md");
    std::fs::write(&real, b"one\n").unwrap();
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let error = markdown::publish(
        &link,
        b"two\n",
        &Target::Existing {
            fingerprint: breeze_storage::bytes_fingerprint(b"one\n"),
        },
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("not a regular file"),
        "{error:#}"
    );
    assert_eq!(std::fs::read_to_string(&real).unwrap(), "one\n");
}
