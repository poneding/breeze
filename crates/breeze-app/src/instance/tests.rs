use super::*;
use futures::{StreamExt, executor::block_on};
use std::process::Command;

fn primary(root: &Path) -> (Instance, Receiver<Vec<PathBuf>>) {
    match start(root, Vec::new()).unwrap() {
        Launch::Primary(instance, requests) => (instance, requests),
        Launch::Forwarded => panic!("The first launch must own the instance"),
    }
}

#[test]
fn a_second_process_forwards_paths_and_a_plain_launch() {
    let directory = tempfile::tempdir().unwrap();
    // Canonicalize only the existing directory, not the missing source files.
    // macOS getcwd() can resolve /var to /private/var; Windows can omit the
    // verbatim prefix that canonicalize() adds to the same directory.
    let root = fs::canonicalize(directory.path()).unwrap();
    let (instance, mut requests) = primary(&root);
    // Unicode, spaces, multiple arguments and the child's working directory
    // must survive IPC without URL conversion or source-file writes.
    let relative = PathBuf::from("中文 field notes.pdf");
    let other = root.join("second.markdown");
    for paths in [vec![relative, other.clone()], Vec::new()] {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "instance::tests::forwarding_child",
                "--ignored",
                "--nocapture",
            ])
            .current_dir(&root)
            .env("BREEZE_INSTANCE_TEST_DIR", &root)
            .env(
                "BREEZE_INSTANCE_TEST_PATHS",
                serde_json::to_string(&paths).unwrap(),
            )
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let forwarded = block_on(requests.next()).unwrap();
        assert_eq!(forwarded.len(), paths.len());
        for (forwarded, path) in forwarded.into_iter().zip(paths) {
            if path.is_absolute() {
                assert_eq!(forwarded, path, "absolute arguments must be preserved");
            } else {
                assert!(forwarded.is_absolute());
                assert_eq!(forwarded.file_name(), path.file_name());
                assert_eq!(
                    fs::canonicalize(forwarded.parent().unwrap()).unwrap(),
                    root,
                    "relative arguments must use the child's working directory"
                );
            }
        }
    }
    assert!(
        !other.exists(),
        "forwarding does not create source documents"
    );
    drop(instance);
    assert!(!root.join("instance.json").exists());
}

#[test]
#[ignore = "subprocess entry point for the process-lock/IPC test"]
fn forwarding_child() {
    let root = PathBuf::from(std::env::var_os("BREEZE_INSTANCE_TEST_DIR").unwrap());
    let paths =
        serde_json::from_str(&std::env::var("BREEZE_INSTANCE_TEST_PATHS").unwrap()).unwrap();
    assert!(matches!(start(&root, paths).unwrap(), Launch::Forwarded));
}

#[test]
fn separate_data_directories_have_independent_instances() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let (_first, _) = primary(first.path());
    let (_second, _) = primary(second.path());
}

#[test]
fn a_stale_endpoint_is_replaced_when_the_lock_is_free() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("instance.json"), "stale endpoint").unwrap();
    let (instance, _) = primary(directory.path());
    let endpoint: Endpoint =
        serde_json::from_slice(&fs::read(directory.path().join("instance.json")).unwrap()).unwrap();
    assert_eq!(endpoint.port, instance.address.port());
    drop(instance);
    let (_restarted, _) = primary(directory.path());
}

#[test]
fn unauthenticated_oversized_and_relative_requests_are_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let (instance, mut requests) = primary(directory.path());
    let endpoint: Endpoint =
        serde_json::from_slice(&fs::read(directory.path().join("instance.json")).unwrap()).unwrap();
    for request in [
        Request {
            token: "wrong token".into(),
            paths: Vec::new(),
        },
        Request {
            token: endpoint.token.clone(),
            paths: vec![PathBuf::from("relative.pdf")],
        },
        Request {
            token: endpoint.token.clone(),
            paths: vec![directory.path().join("a.pdf"); MAX_PATHS + 1],
        },
    ] {
        let mut stream = TcpStream::connect(instance.address).unwrap();
        configure(&stream).unwrap();
        let bytes = serde_json::to_vec(&request).unwrap();
        stream
            .write_all(&(bytes.len() as u32).to_le_bytes())
            .unwrap();
        stream.write_all(&bytes).unwrap();
        assert!(stream.read_exact(&mut [0]).is_err());
    }
    let mut stream = TcpStream::connect(instance.address).unwrap();
    configure(&stream).unwrap();
    stream
        .write_all(&((MAX_REQUEST_BYTES + 1) as u32).to_le_bytes())
        .unwrap();
    assert!(stream.read_exact(&mut [0]).is_err());
    // Rejected clients must not break the real launch route.
    assert!(matches!(
        start(directory.path(), Vec::new()).unwrap(),
        Launch::Forwarded
    ));
    assert!(block_on(requests.next()).unwrap().is_empty());
}
