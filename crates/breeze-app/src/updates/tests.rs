use super::*;
use gpui_kit::test::TestWindowExt;
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};

fn metadata(tag: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "tag_name": tag, "draft": false, "prerelease": false,
        "html_url": "https://untrusted.invalid/install"
    }))
    .unwrap()
}

fn fixture(response: String, delay: Duration) -> (String, thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/latest", listener.local_addr().unwrap());
    let worker = thread::spawn(move || {
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
        let (headers, body) = response.split_once("\r\n\r\n").unwrap();
        socket
            .write_all(format!("{headers}\r\n\r\n").as_bytes())
            .unwrap();
        thread::sleep(delay);
        // The total deadline must continue through body reads, not only headers.
        let _ = socket.write_all(body.as_bytes());
        String::from_utf8(request).unwrap()
    });
    (endpoint, worker)
}

fn test_client(timeout: Duration) -> ureq::Agent {
    client(timeout, false)
}

#[test]
fn semantic_versions_and_release_urls_are_checked() {
    for tag in ["v0.10.0", "0.10.0", "V1.0.0", "1.0.0+build.5"] {
        let CheckStatus::Available { url, .. } = parse_release(&metadata(tag), "0.9.0").unwrap()
        else {
            panic!("{tag}")
        };
        assert_eq!(url, format!("{RELEASES}/tag/{tag}"));
    }
    for tag in ["0.9.0", "v0.8.9", "0.9.0+different.build"] {
        assert!(matches!(
            parse_release(&metadata(tag), "0.9.0"),
            Ok(CheckStatus::Current { .. })
        ));
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
        assert_eq!(
            parse_release(&metadata(tag), "0.1.0"),
            Err(CheckError::InvalidMetadata)
        );
    }
    assert!(matches!(
        parse_release(&metadata("1.0.0"), "1.0.0-rc.1"),
        Ok(CheckStatus::Available { .. })
    ));
    assert_eq!(
        parse_release(
            br#"{"tag_name":"1.0.0","draft":true,"prerelease":false}"#,
            "0.1.0"
        ),
        Err(CheckError::InvalidMetadata)
    );
    assert_eq!(
        parse_release(
            br#"{"tag_name":"1.0.0","draft":false,"prerelease":true}"#,
            "0.1.0"
        ),
        Err(CheckError::InvalidMetadata)
    );
    assert_eq!(
        parse_release(b"not json", "0.1.0"),
        Err(CheckError::InvalidMetadata)
    );
    assert_eq!(
        parse_release(b"{}", "0.1.0"),
        Err(CheckError::InvalidMetadata)
    );
}

#[test]
fn real_http_transport_sends_headers_and_rejects_false_success() {
    let body = String::from_utf8(metadata("v0.2.0")).unwrap();
    let (endpoint, server) = fixture(
        format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        ),
        Duration::ZERO,
    );
    let result = fetch(&test_client(TIMEOUT), &endpoint, "0.1.0").unwrap();
    assert!(matches!(result, CheckStatus::Available { .. }));
    let request = server.join().unwrap().to_lowercase();
    assert!(request.starts_with("get /latest http/1.1"));
    assert!(request.contains("user-agent: breeze/0.1.0"));
    assert!(request.contains("accept: application/vnd.github+json"));
    assert!(request.contains("x-github-api-version: 2022-11-28"));
    assert!(!request.contains("authorization"));
    for (code, headers, expected) in [
        (404, "", Ok(CheckStatus::Unpublished)),
        (403, "", Err(CheckError::Http(403))),
        (
            403,
            "X-RateLimit-Remaining: 0\r\n",
            Err(CheckError::RateLimited),
        ),
        (429, "", Err(CheckError::RateLimited)),
        (500, "", Err(CheckError::Http(500))),
        (
            302,
            "Location: https://untrusted.invalid\r\n",
            Err(CheckError::Http(302)),
        ),
        (200, "", Err(CheckError::InvalidMetadata)),
    ] {
        let (endpoint, server) = fixture(
            format!("HTTP/1.1 {code} Status\r\n{headers}Content-Length: 0\r\n\r\n"),
            Duration::ZERO,
        );
        assert_eq!(fetch(&test_client(TIMEOUT), &endpoint, "0.1.0"), expected);
        server.join().unwrap();
    }
}

#[test]
fn transport_bounds_body_and_total_time() {
    let body = "x".repeat(MAX_METADATA_BYTES as usize + 1);
    let (endpoint, server) = fixture(
        format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        ),
        Duration::ZERO,
    );
    assert_eq!(
        fetch(&test_client(TIMEOUT), &endpoint, "0.1.0"),
        Err(CheckError::TooLarge)
    );
    server.join().unwrap();
    let (endpoint, server) = fixture(
        "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}".into(),
        Duration::from_millis(250),
    );
    assert_eq!(
        fetch(&test_client(Duration::from_millis(50)), &endpoint, "0.1.0"),
        Err(CheckError::Timeout)
    );
    server.join().unwrap();
    assert!(client(TIMEOUT, true).config().https_only());
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
            url: RELEASES.into(),
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

#[gpui_kit::test]
fn update_controls_and_notifications_follow_language_and_state(cx: &mut TestAppContext) {
    let (handle, view) = ui_tests::app(cx);
    cx.simulate_window_resize(handle, size(px(900.), px(650.)));
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.screen = Screen::Home;
            view.home_page = HomePage::Info;
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
                    url: format!("{RELEASES}/tag/v1.0.0"),
                },
            );
            cx.notify();
        });
        window.render_frame(cx);
        assert_eq!(window.find("check-updates").label(), Some("Check now"));
        assert_eq!(
            window.find("open-update-release").label(),
            Some("View release")
        );
        view.update(cx, |view, cx| {
            view.home_page = HomePage::Library;
            view.language = 2;
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
            view.language = 0;
            view.refresh_language(window, cx);
        });
    })
    .unwrap();
}
