use super::install::WindowsPackage;
use super::*;
use gpui_kit::test::TestWindowExt;
use sha2::{Digest as _, Sha256};
use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::PathBuf,
    thread,
};
#[cfg(target_os = "macos")]
use std::{path::Path, process::Command};

/// The release page redirect for a published stable tag.
fn redirect_to(location: &str) -> String {
    format!("HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\n\r\n")
}

/// A response on the download path: a redirect to the stored file when the
/// asset exists, a plain 404 when it does not.
fn download_route(present: bool) -> String {
    if present {
        redirect_to("https://release-assets.invalid/file")
    } else {
        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".into()
    }
}

/// The fixture server closes the connection after every response. Saying so is
/// required: a keep-alive response invites the client to reuse a socket that is
/// already gone, which surfaces as `Connection reset by peer` on the next
/// request and failed the download tests intermittently under load.
fn close_headers(headers: &str) -> String {
    format!("{headers}\r\nConnection: close\r\n\r\n")
}

/// Serve one response per accepted connection, in order. Downloads request the
/// checksum first and a check requests the release redirect first, so each test
/// can pin the request paths it expects. Each connection is closed after its
/// response, which the response headers must admit.
fn serve(responses: Vec<String>) -> (String, thread::JoinHandle<Vec<String>>) {
    serve_after(Duration::ZERO, responses)
}

/// `serve` with a delay before each response, so a client's total deadline can
/// be shown to cover waiting rather than only reading.
fn serve_after(
    delay: Duration,
    responses: Vec<String>,
) -> (String, thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let origin = base.clone();
    let worker = thread::spawn(move || {
        let mut requests = Vec::new();
        for response in responses {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            let mut byte = [0];
            while !request.ends_with(b"\r\n\r\n") {
                socket.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
                assert!(request.len() < 8192);
            }
            // Response fixtures name the loopback origin as `{base}`, which is
            // only known once the listener is bound.
            let response = response.replace("{base}", &origin);
            let (headers, body) = response.split_once("\r\n\r\n").unwrap();
            thread::sleep(delay);
            // A client whose deadline already expired has closed the socket;
            // writing to it is best-effort so the fixture thread can finish.
            let _ = socket.write_all(format!("{}{body}", close_headers(headers)).as_bytes());
            requests.push(String::from_utf8(request).unwrap());
        }
        requests
    });
    (base, worker)
}

fn http_body(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn test_client(timeout: Duration) -> ureq::Agent {
    client(timeout, false)
}

/// Run one check against a loopback fixture: the release redirect plus whatever
/// download-path answers the check may reach. Response fixtures name the origin
/// as `{base}`. Returns the check result and every request the server saw.
fn check_fixture(
    target: &InstallTarget,
    current: &str,
    responses: Vec<String>,
) -> (Result<CheckStatus, CheckError>, Vec<String>) {
    let (base, server) = serve(responses);
    let result = fetch(
        &test_client(TIMEOUT),
        &format!("{base}/latest"),
        &format!("{base}/download"),
        current,
        target,
    );
    (result, server.join().unwrap())
}

/// Method and path of each request, lowercased for comparison.
fn request_paths(requests: &[String]) -> Vec<String> {
    requests
        .iter()
        .map(|request| {
            request
                .split_whitespace()
                .take(2)
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase()
        })
        .collect()
}

#[test]
fn release_redirects_name_only_stable_versions() {
    for location in [
        "https://github.com/poneding/breeze/releases/tag/v0.10.0",
        "https://github.com/poneding/breeze/releases/tag/0.10.0",
        "https://github.com/poneding/breeze/releases/tag/V1.0.0",
        "https://github.com/poneding/breeze/releases/tag/1.0.0+build.5",
    ] {
        assert!(tag_from_location(location).is_some(), "{location}");
    }
    // A repository without a stable release redirects to the release list,
    // which names no version; neither does a target that adds path segments.
    for location in [
        "https://github.com/poneding/breeze/releases",
        "https://github.com/poneding/breeze/releases/tag/",
        "https://github.com/poneding/breeze/releases/tag/1.0.0/../../evil",
        "https://github.com/poneding/breeze/releases/tag/1.0.0?x=1",
        "https://untrusted.invalid/install",
        "",
    ] {
        assert_eq!(tag_from_location(location), None, "{location}");
    }

    for tag in ["v0.10.0", "0.10.0", "V1.0.0", "1.0.0+build.5"] {
        assert!(tag_version(tag).is_ok(), "{tag}");
    }
    for tag in [
        "latest",
        "v1",
        "1.0.01",
        "1.0.0-beta.1",
        "1.0.0/../../evil",
        "https://evil.invalid",
        "",
    ] {
        assert_eq!(tag_version(tag), Err(CheckError::InvalidMetadata), "{tag}");
    }
    assert_eq!(
        tag_version(&format!("1.0.0+{}", "a".repeat(TAG_LIMIT))),
        Err(CheckError::InvalidMetadata)
    );

    // An unsupported target asks for no package, so these results need no
    // network: the version comparison is the whole subject here.
    let agent = test_client(TIMEOUT);
    let unsupported = InstallTarget::Unsupported;
    let CheckStatus::Available {
        version,
        tag,
        url,
        install,
    } = check_status(
        &agent,
        "http://download.invalid",
        "v0.10.0",
        "0.9.0",
        &unsupported,
    )
    .unwrap()
    else {
        panic!("a newer stable tag is available")
    };
    assert_eq!(version, "0.10.0");
    assert_eq!(tag, "v0.10.0");
    // The link is built from the fixed repository, never from a response.
    assert_eq!(url, format!("{RELEASES}/tag/v0.10.0"));
    assert!(install.is_none());
    for tag in ["0.9.0", "v0.8.9", "0.9.0+different.build"] {
        assert_eq!(
            check_status(
                &agent,
                "http://download.invalid",
                tag,
                "0.9.0",
                &unsupported
            ),
            Ok(CheckStatus::Current {
                latest: tag_version(tag).unwrap().to_string()
            })
        );
    }
    assert!(matches!(
        check_status(
            &agent,
            "http://download.invalid",
            "1.0.0",
            "1.0.0-rc.1",
            &unsupported
        ),
        Ok(CheckStatus::Available { .. })
    ));
}

#[test]
fn install_plans_match_published_package_names() {
    let version = "0.2.0";
    let mac = if cfg!(target_arch = "aarch64") {
        "aarch64-apple-darwin"
    } else {
        "x86_64-apple-darwin"
    };
    for (target, expected) in [
        (
            InstallTarget::MacBundle(PathBuf::from("/Applications/Breeze.app")),
            Some(format!("Breeze-{version}-{mac}.dmg")),
        ),
        (
            InstallTarget::WindowsSetup(WindowsPackage::Inno),
            Some(format!("Breeze-{version}-x86_64-pc-windows-msvc.exe")),
        ),
        (
            InstallTarget::WindowsSetup(WindowsPackage::Msi),
            Some(format!("Breeze-{version}-x86_64-pc-windows-msvc.msi")),
        ),
        (
            InstallTarget::AppImage(PathBuf::from("/opt/Breeze.AppImage")),
            Some(format!(
                "Breeze-{version}-x86_64-unknown-linux-gnu.AppImage"
            )),
        ),
        (
            InstallTarget::DebianPackage,
            Some(format!("Breeze-{version}-x86_64-unknown-linux-gnu.deb")),
        ),
        (InstallTarget::Unsupported, None),
    ] {
        let plan = target.plan(version);
        assert_eq!(plan.as_ref().map(|plan| plan.asset.clone()), expected);
        if let Some(plan) = plan {
            assert_eq!(plan.checksum, format!("{}.sha256", plan.asset));
        }
    }
}

#[test]
fn a_release_offers_an_install_only_when_its_assets_are_published() {
    let target = InstallTarget::MacBundle(PathBuf::from("/Applications/Breeze.app"));
    let plan = target.plan("0.2.0").unwrap();
    let tag_redirect = redirect_to("{base}/releases/tag/v0.2.0");

    let (result, requests) = check_fixture(
        &target,
        "0.1.0",
        vec![
            tag_redirect.clone(),
            download_route(true),
            download_route(true),
        ],
    );
    let CheckStatus::Available { install, tag, .. } = result.unwrap() else {
        panic!("a newer release")
    };
    assert_eq!(tag, "v0.2.0");
    assert_eq!(install, Some(plan.clone()));
    // Discovery and confirmation are bodyless HEAD requests on the fixed paths,
    // so neither the release body nor an asset is read to answer the check.
    assert_eq!(
        request_paths(&requests),
        [
            "head /latest".to_string(),
            format!("head /download/v0.2.0/{}", plan.asset).to_lowercase(),
            format!("head /download/v0.2.0/{}", plan.checksum).to_lowercase(),
        ]
    );

    // A package without its checksum must not be offered for automatic install.
    let (result, _) = check_fixture(
        &target,
        "0.1.0",
        vec![
            tag_redirect.clone(),
            download_route(true),
            download_route(false),
        ],
    );
    let CheckStatus::Available { install, .. } = result.unwrap() else {
        panic!("still a newer release")
    };
    assert!(install.is_none(), "a package without its checksum");

    // A missing package ends the check without asking for the checksum.
    let (result, requests) =
        check_fixture(&target, "0.1.0", vec![tag_redirect, download_route(false)]);
    let CheckStatus::Available { install, .. } = result.unwrap() else {
        panic!("still a newer release")
    };
    assert!(install.is_none(), "no package for this system");
    assert_eq!(request_paths(&requests).len(), 2);

    // An unsupported platform can only open the release page.
    let (result, requests) = check_fixture(
        &InstallTarget::Unsupported,
        "0.1.0",
        vec![redirect_to("{base}/releases/tag/v0.2.0")],
    );
    let CheckStatus::Available { install, .. } = result.unwrap() else {
        panic!("still a newer release")
    };
    assert!(install.is_none());
    assert_eq!(request_paths(&requests), ["head /latest".to_string()]);
}

#[test]
fn real_http_transport_sends_headers_and_maps_unexpected_statuses() {
    let (result, requests) = check_fixture(
        &InstallTarget::Unsupported,
        "0.1.0",
        vec![redirect_to("{base}/releases/tag/v0.2.0")],
    );
    assert_eq!(
        result,
        Ok(CheckStatus::Available {
            version: "0.2.0".into(),
            tag: "v0.2.0".into(),
            url: format!("{RELEASES}/tag/v0.2.0"),
            install: None,
        })
    );
    let request = requests.join("").to_lowercase();
    assert!(request.starts_with("head /latest http/1.1"));
    // The user agent carries the built version, not the version under test.
    assert!(request.contains(concat!("user-agent: breeze/", env!("CARGO_PKG_VERSION"))));
    assert!(request.contains("accept: text/html"));
    // Discovery is anonymous: no token is embedded or expected.
    assert!(!request.contains("authorization"));
    for (response, expected) in [
        (
            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_string(),
            Ok(CheckStatus::Unpublished),
        ),
        (
            "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n".to_string(),
            Err(CheckError::Http(403)),
        ),
        (
            "HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\n\r\n".to_string(),
            Err(CheckError::RateLimited),
        ),
        (
            "HTTP/1.1 500 Oops\r\nContent-Length: 0\r\n\r\n".to_string(),
            Err(CheckError::Http(500)),
        ),
        // A success status without a redirect names no version.
        (
            "HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n".to_string(),
            Ok(CheckStatus::Unpublished),
        ),
        // Neither does a redirect that is not a release tag. The check never
        // follows one, so an unexpected target cannot redirect the download.
        (
            redirect_to("https://untrusted.invalid/install"),
            Ok(CheckStatus::Unpublished),
        ),
        (
            redirect_to("https://github.com/poneding/breeze/releases"),
            Ok(CheckStatus::Unpublished),
        ),
    ] {
        let (result, _) = check_fixture(&InstallTarget::Unsupported, "0.1.0", vec![response]);
        assert_eq!(result, expected);
    }
}

#[test]
fn transport_bounds_total_time() {
    let (base, server) = serve_after(
        Duration::from_millis(250),
        vec![redirect_to("{base}/releases/tag/v0.2.0")],
    );
    assert_eq!(
        fetch(
            &test_client(Duration::from_millis(50)),
            &format!("{base}/latest"),
            &format!("{base}/download"),
            "0.1.0",
            &InstallTarget::Unsupported,
        ),
        Err(CheckError::Timeout)
    );
    server.join().unwrap();
    assert!(client(TIMEOUT, true).config().https_only());
}

/// Live probe against the real repository, ignored so the suite stays offline.
/// Run it with `cargo test -p breeze-app --lib -- --ignored live_release`.
#[test]
#[ignore = "network: reads the published release redirect"]
fn live_release_discovery() {
    let result = fetch(
        &client(TIMEOUT, true),
        LATEST,
        install::DOWNLOAD_ROOT,
        "0.0.1",
        &InstallTarget::detect(),
    )
    .unwrap();
    assert!(
        matches!(
            result,
            CheckStatus::Available { .. } | CheckStatus::Current { .. }
        ),
        "{result:?}"
    );
}

#[test]
fn checksum_sidecars_require_a_full_sha256_digest() {
    let expected = digest(b"accept");
    assert_eq!(
        install::parse_checksum(format!("{expected}  Breeze.pkg\n").as_bytes()),
        Ok(expected.clone())
    );
    assert_eq!(
        install::parse_checksum(format!("  {expected}\n").as_bytes()),
        Ok(expected)
    );
    let too_long = "a".repeat(65);
    for invalid in [
        b"".as_slice(),
        b"\n",
        b"abc  Breeze.pkg\n",
        b"zzzz  Breeze.pkg\n",
        too_long.as_bytes(),
        b"not utf8 \xff",
    ] {
        assert_eq!(
            install::parse_checksum(invalid),
            Err(InstallError::ChecksumInvalid),
            "{invalid:?}"
        );
    }
}

#[test]
fn download_verifies_the_published_checksum() {
    let artifact = "Breeze installer payload";
    let plan = install::InstallPlan {
        asset: "Breeze-test.bin".into(),
        checksum: "Breeze-test.bin.sha256".into(),
    };
    let (base, server) = serve(vec![
        http_body(&format!(
            "{}  {}\n",
            digest(artifact.as_bytes()),
            plan.asset
        )),
        http_body(artifact),
    ]);
    let directory = tempfile::tempdir().unwrap();
    let progress = install::DownloadProgress::default();
    let path = install::download_into(
        &install::download_client(false),
        &base,
        "v0.2.0",
        &plan,
        directory.path(),
        &progress,
    )
    .unwrap();
    assert_eq!(fs::read(&path).unwrap(), artifact.as_bytes());
    assert_eq!(progress.percent(), Some(100));
    let requests = server.join().unwrap();
    assert!(
        requests[0]
            .to_lowercase()
            .starts_with("get /v0.2.0/breeze-test.bin.sha256 http/1.1"),
        "{}",
        requests[0]
    );
    assert!(
        requests[1]
            .to_lowercase()
            .starts_with("get /v0.2.0/breeze-test.bin http/1.1"),
        "{}",
        requests[1]
    );
}

#[test]
fn download_rejects_a_mismatched_or_missing_checksum() {
    let plan = install::InstallPlan {
        asset: "Breeze-test.bin".into(),
        checksum: "Breeze-test.bin.sha256".into(),
    };
    let directory = tempfile::tempdir().unwrap();
    let progress = install::DownloadProgress::default();
    let (base, server) = serve(vec![
        http_body(&format!("{}  {}\n", digest(b"other"), plan.asset)),
        http_body("Breeze installer payload"),
    ]);
    assert_eq!(
        install::download_into(
            &install::download_client(false),
            &base,
            "v0.2.0",
            &plan,
            directory.path(),
            &progress,
        ),
        Err(InstallError::ChecksumMismatch)
    );
    server.join().unwrap();
    assert!(!directory.path().join(&plan.asset).exists());

    let (base, server) = serve(vec![
        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".into(),
    ]);
    assert_eq!(
        install::download_into(
            &install::download_client(false),
            &base,
            "v0.2.0",
            &plan,
            directory.path(),
            &progress,
        ),
        Err(InstallError::ChecksumMissing)
    );
    server.join().unwrap();
}

#[test]
fn policies_bound_concurrency_and_ignore_disabled_automatic_results() {
    let mut state = Updates::default();
    assert!(!state.begin(true, 0));
    assert!(!state.begin(true, 2));
    assert!(state.begin(true, 1));
    assert!(!state.begin(false, 1));
    assert!(!state.begin(true, 1));
    state.finish(
        true,
        2,
        CheckStatus::Available {
            version: "1.0.0".into(),
            tag: "v1.0.0".into(),
            url: RELEASES.into(),
            install: None,
        },
    );
    assert_eq!(state.status, CheckStatus::Idle);
    assert!(!state.in_flight);
    assert!(!state.begin(true, 1));
    // Explicit manual checks remain available even when background checks are disabled.
    assert!(state.begin(false, 2));
    state.finish(false, 2, CheckStatus::Unpublished);
    assert_eq!(state.status, CheckStatus::Unpublished);
    assert!(state.begin(false, 0));
}

/// A missing application must fail on every platform. Windows has no bundle
/// launch (its update runs a setup package), so a bundle path there is refused
/// rather than handed to the macOS-only `open` command.
#[test]
fn relaunch_rejects_a_missing_application() {
    let action = RestartAction::Relaunch {
        path: PathBuf::from("/nonexistent/Breeze"),
        kind: install::LaunchKind::Executable,
    };
    assert!(matches!(
        action.launch(),
        Err(InstallError::LaunchFailed(_))
    ));
    let action = RestartAction::Relaunch {
        path: PathBuf::from("/nonexistent/Breeze.app"),
        kind: install::LaunchKind::Bundle,
    };
    assert!(matches!(
        action.launch(),
        Err(InstallError::LaunchFailed(_))
    ));
}

#[cfg(target_os = "macos")]
fn macos_app(parent: &Path, version: &str, binary: &str) -> PathBuf {
    let app = parent.join("Breeze.app");
    fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
    fs::create_dir_all(app.join("Contents/Resources")).unwrap();
    fs::write(app.join("Contents/MacOS/Breeze"), binary).unwrap();
    fs::write(
        app.join("Contents/Resources/VERSION"),
        format!("{version}\n"),
    )
    .unwrap();
    app
}

#[cfg(target_os = "macos")]
fn macos_image(root: &Path, payload: &Path, version: &str) -> PathBuf {
    let dmg = root.join(format!("Breeze-{version}-test.dmg"));
    // `hdiutil` is not safe to drive concurrently, and the updater holds the same
    // lock for its own attach, so building the fixture takes it too.
    super::install::with_image_locked(|| {
        let status = Command::new("hdiutil")
            .args([
                "create",
                "-quiet",
                "-volname",
                &format!("Breeze update {version}"),
                "-srcfolder",
            ])
            .arg(payload)
            .args(["-ov", "-format", "UDZO"])
            .arg(&dmg)
            .status()
            .unwrap();
        assert!(status.success(), "hdiutil create failed");
    });
    dmg
}

#[cfg(target_os = "macos")]
#[test]
fn macos_install_swaps_the_bundle_and_removes_the_old_one() {
    let root = tempfile::tempdir().unwrap();
    let applications = root.path().join("Applications");
    let installed = macos_app(&applications, "0.1.0", "old binary");
    let payload = root.path().join("payload");
    fs::create_dir_all(&payload).unwrap();
    macos_app(&payload, "0.2.0", "new binary");
    let dmg = macos_image(root.path(), &payload, "0.2.0");

    let completion =
        install::install(&InstallTarget::MacBundle(installed.clone()), &dmg, "0.2.0").unwrap();
    assert_eq!(
        completion,
        install::Completion::Installed {
            path: installed.clone(),
            launch: install::LaunchKind::Bundle,
        }
    );
    assert_eq!(
        fs::read_to_string(installed.join("Contents/Resources/VERSION"))
            .unwrap()
            .trim(),
        "0.2.0"
    );
    assert_eq!(
        fs::read_to_string(installed.join("Contents/MacOS/Breeze")).unwrap(),
        "new binary"
    );
    assert!(
        !dmg.exists(),
        "the verified image is discarded after install"
    );
    let leftovers: Vec<_> = fs::read_dir(&applications)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .filter(|name| name.to_string_lossy().starts_with(".Breeze-"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[cfg(target_os = "macos")]
#[test]
fn macos_install_rejects_a_bundle_with_the_wrong_version() {
    let root = tempfile::tempdir().unwrap();
    let applications = root.path().join("Applications");
    let installed = macos_app(&applications, "0.1.0", "old binary");
    let payload = root.path().join("payload");
    fs::create_dir_all(&payload).unwrap();
    macos_app(&payload, "0.3.0", "wrong binary");
    let dmg = macos_image(root.path(), &payload, "0.3.0");

    assert_eq!(
        install::install(&InstallTarget::MacBundle(installed.clone()), &dmg, "0.2.0"),
        Err(InstallError::VersionMismatch {
            found: "0.3.0".into(),
            expected: "0.2.0".into(),
        })
    );
    assert_eq!(
        fs::read_to_string(installed.join("Contents/Resources/VERSION"))
            .unwrap()
            .trim(),
        "0.1.0",
        "a rejected image leaves the installed bundle intact"
    );
    assert!(dmg.exists(), "the rejected image is kept for inspection");
}

#[gpui_kit::test]
fn update_controls_and_notifications_follow_language_and_state(cx: &mut TestAppContext) {
    let (handle, view) = ui_tests::app(cx);
    cx.simulate_window_resize(handle, size(px(900.), px(650.)));
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.screen = Screen::Home;
            view.home_page = HomePage::About;
            view.language = 1;
            view.refresh_language(window, cx);
            assert!(view.updates.begin(false, 0));
        });
        window.render_frame(cx);
        assert_eq!(
            window.find("check-updates").label(),
            Some("Checking for updates…")
        );
        let bounds = window.find("check-updates").bounds();
        assert!(
            bounds.right() <= px(900.) && bounds.bottom() <= px(650.),
            "check button: {bounds:?}"
        );
        window.click("check-updates", cx);
        assert!(view.read(cx).updates.in_flight);
        view.update(cx, |view, cx| {
            view.updates.finish(
                false,
                0,
                CheckStatus::Available {
                    version: "1.0.0".into(),
                    tag: "v1.0.0".into(),
                    url: format!("{RELEASES}/tag/v1.0.0"),
                    install: None,
                },
            );
            cx.notify();
        });
        window.render_frame(cx);
        assert_eq!(
            window.find("check-updates").label(),
            Some("Check for updates")
        );
        assert_eq!(
            window.find("open-update-release").label(),
            Some("View release")
        );
        view.update(cx, |view, cx| {
            view.home_page = HomePage::Library;
            view.language = i18n::LanguagePreference::TraditionalChinese as usize;
            view.refresh_language(window, cx);
        });
        window.render_frame(cx);
        assert_eq!(
            window.find("update-banner-release").label(),
            Some("查看發佈版本")
        );
        window.click("dismiss-update-banner", cx);
        assert!(view.read(cx).updates.banner_dismissed);
        view.update(cx, |view, cx| {
            view.language = i18n::LanguagePreference::SimplifiedChinese as usize;
            view.refresh_language(window, cx);
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn an_available_install_is_offered_in_the_banner_and_about(cx: &mut TestAppContext) {
    let (handle, view) = ui_tests::app(cx);
    cx.simulate_window_resize(handle, size(px(900.), px(650.)));
    let plan = InstallPlan {
        asset: "Breeze-1.0.0-x86_64-pc-windows-msvc.exe".into(),
        checksum: "Breeze-1.0.0-x86_64-pc-windows-msvc.exe.sha256".into(),
    };
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.show_home(cx);
            view.language = 1;
            view.refresh_language(window, cx);
            view.updates.status = CheckStatus::Available {
                version: "1.0.0".into(),
                tag: "v1.0.0".into(),
                url: format!("{RELEASES}/tag/v1.0.0"),
                install: Some(plan.clone()),
            };
            cx.notify();
        });
        window.render_frame(cx);
        assert!(window.find("update-banner").visible());
        assert_eq!(
            window.find("update-banner-install").label(),
            Some("Update now")
        );
        assert!(window.find("update-banner-release").visible());
        assert!(
            window.find("update-banner").bounds().top()
                < window.find("library-sidebar").bounds().top()
        );

        view.update(cx, |view, cx| {
            view.home_page = HomePage::About;
            cx.notify();
        });
        window.render_frame(cx);
        // About swaps the banner for the same entry beside the check control.
        assert!(window.try_find("update-banner").is_none());
        assert_eq!(window.find("install-update").label(), Some("Update now"));
    })
    .unwrap();
}

#[gpui_kit::test]
fn a_ready_install_offers_restart_without_reinstalling(cx: &mut TestAppContext) {
    let (handle, view) = ui_tests::app(cx);
    cx.simulate_window_resize(handle, size(px(900.), px(650.)));
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.language = 1;
            view.refresh_language(window, cx);
            view.updates.phase = Phase::Ready {
                version: "1.0.0".into(),
                action: RestartAction::RunInstaller {
                    path: PathBuf::from("/tmp/Breeze-1.0.0-setup.exe"),
                },
            };
            cx.notify();
        });
        window.render_frame(cx);
        assert!(window.find("update-banner-restart").visible());
        assert!(window.try_find("update-banner-install").is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn a_ready_install_prompts_for_a_restart_only_once(cx: &mut TestAppContext) {
    let (handle, view) = ui_tests::app(cx);
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.updates.phase = Phase::Ready {
                version: "1.0.0".into(),
                action: RestartAction::RunInstaller {
                    path: PathBuf::from("/tmp/Breeze-1.0.0-setup.exe"),
                },
            };
            view.prompt_restart(window, cx);
            assert!(view.updates.restart_prompted);
        });
        window.render_frame(cx);
        assert!(window.has_active_dialog(cx));
        // A second completion in the same session must not stack modals.
        view.update(cx, |view, cx| view.prompt_restart(window, cx));
        window.render_frame(cx);
        window.click("cancel", cx);
        window.render_frame(cx);
        assert!(!window.has_active_dialog(cx));
        // Dismissing keeps the banner entry for a later restart.
        assert!(!view.read(cx).updates.restart_pending());
        assert!(view.read(cx).updates.restart_action().is_some());
    })
    .unwrap();
}

#[gpui_kit::test]
fn about_status_wraps_centered_below_the_check_button(cx: &mut TestAppContext) {
    let (handle, view) = ui_tests::app(cx);
    cx.simulate_window_resize(handle, size(px(900.), px(650.)));
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.screen = Screen::Home;
            view.home_page = HomePage::About;
            view.language = 1;
            view.refresh_language(window, cx);
            view.updates.status = CheckStatus::Unpublished;
        });
        window.render_frame(cx);
        let status = window.find("update-status").bounds();
        let button = window.find("check-updates").bounds();
        assert!(status.size.width <= px(360.) + px(1.), "{status:?}");
        assert!((status.center().x - button.center().x).abs() < px(1.));
        assert!(status.top() >= button.bottom());
        // The unpublished notice is longer than one 12 px line, so it must wrap.
        assert!(status.size.height > px(20.), "status must wrap: {status:?}");
    })
    .unwrap();
}

#[gpui_kit::test]
fn about_stays_silent_until_a_check_has_a_result(cx: &mut TestAppContext) {
    let (handle, view) = ui_tests::app(cx);
    cx.simulate_window_resize(handle, size(px(900.), px(650.)));
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.screen = Screen::Home;
            view.home_page = HomePage::About;
            view.language = 1;
            view.refresh_language(window, cx);
            assert_eq!(view.updates.status, CheckStatus::Idle);
        });
        window.render_frame(cx);
        // An untouched check is not news, so no placeholder line is shown.
        assert!(window.try_find("update-status").is_none());
        view.update(cx, |view, cx| {
            assert!(view.updates.begin(false, 0));
            cx.notify();
        });
        window.render_frame(cx);
        // The button already labels the running check; the line stays hidden.
        assert_eq!(
            window.find("check-updates").label(),
            Some("Checking for updates…")
        );
        assert!(window.try_find("update-status").is_none());
        view.update(cx, |view, cx| {
            view.updates.finish(
                false,
                0,
                CheckStatus::Current {
                    latest: "9.9.9".into(),
                },
            );
            cx.notify();
        });
        window.render_frame(cx);
        assert!(window.find("update-status").visible());
    })
    .unwrap();
}
