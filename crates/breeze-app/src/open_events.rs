//! Documents the operating system asks Breeze to open.
//!
//! Windows and Linux hand a document to a new process as a command-line
//! argument. macOS delivers `application:openURLs:` to the running application
//! instead, and that event can arrive before the reader window exists. The
//! platform callback carries no application context, so requests are queued
//! here and delivered to the window on the foreground executor.

use crate::Breeze;
use gpui_kit::*;
use std::{cell::RefCell, ffi::OsString, path::PathBuf, rc::Rc};

/// Attempts allowed when a delivery cannot borrow the application yet. The
/// foreground executor retries after the current main-thread turn, which is
/// normally enough; the cap keeps a closed window from looping forever.
const DELIVERY_ATTEMPTS: u8 = 4;

/// Requests kept while no window can receive them. The library itself stops at
/// 4096 entries, so this only bounds memory when a malformed event floods the
/// queue; the documents beyond the cap are dropped instead of silently growing.
const MAX_PENDING: usize = 256;

/// Documents named by command-line arguments.
///
/// macOS passes `-psn_0_12345` when Finder launches a bundle and a user can add
/// options of their own, so anything that looks like a flag is an option rather
/// than a document.
pub(super) fn argument_paths(arguments: impl IntoIterator<Item = OsString>) -> Vec<PathBuf> {
    arguments
        .into_iter()
        .filter(|argument| !argument.to_string_lossy().starts_with('-'))
        .map(PathBuf::from)
        .collect()
}

/// Documents named by open-url events. Only `file:` URLs are documents Breeze
/// can read; other schemes belong to other applications and are ignored rather
/// than guessed at.
pub(super) fn paths_from_urls(urls: &[String]) -> Vec<PathBuf> {
    urls.iter().filter_map(|url| file_url_path(url)).collect()
}

fn file_url_path(url: &str) -> Option<PathBuf> {
    let rest = url.strip_prefix("file://")?;
    // macOS writes `file:///Users/…`; a share may name the local host.
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    // Only absolute paths are documents; `file://server/share/…` is a network
    // location whose handling belongs to the file manager.
    if !rest.starts_with('/') {
        return None;
    }
    let path = rest.split(['?', '#']).next().unwrap_or(rest);
    let decoded = percent_decode(path.as_bytes());
    Some(PathBuf::from(
        String::from_utf8_lossy(&decoded).into_owned(),
    ))
}

fn percent_decode(bytes: &[u8]) -> Vec<u8> {
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && let (Some(high), Some(low)) = (
                bytes.get(index + 1).copied().and_then(hex_digit),
                bytes.get(index + 2).copied().and_then(hex_digit),
            )
        {
            decoded.push(high * 16 + low);
            index += 3;
            continue;
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    decoded
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Queued document requests and the window that will open them.
#[derive(Clone, Default)]
pub(super) struct OpenRequests(Rc<RefCell<RequestState>>);

#[derive(Default)]
struct RequestState {
    app: Option<AsyncApp>,
    window: Option<AnyWindowHandle>,
    /// The reader view is owned by the window's `Root`, so requests update the
    /// entity directly instead of casting the root view.
    view: Option<WeakEntity<Breeze>>,
    pending: Vec<PathBuf>,
    scheduled: bool,
    attempts: u8,
}

impl OpenRequests {
    pub(super) fn new() -> Self {
        Self::default()
    }

    /// Remember where later platform events are delivered.
    pub(super) fn attach(&self, app: AsyncApp, window: AnyWindowHandle, view: WeakEntity<Breeze>) {
        let mut state = self.0.borrow_mut();
        state.app = Some(app);
        state.window = Some(window);
        state.view = Some(view);
    }

    /// Documents requested before the window existed. The caller opens them
    /// after the saved library has been restored.
    pub(super) fn take_pending(&self) -> Vec<PathBuf> {
        std::mem::take(&mut self.0.borrow_mut().pending)
    }

    /// Queue the documents an open-url event named and deliver them.
    pub(super) fn receive_urls(&self, urls: &[String]) {
        let paths = paths_from_urls(urls);
        if paths.is_empty() {
            return;
        }
        {
            let mut state = self.0.borrow_mut();
            let room = MAX_PENDING.saturating_sub(state.pending.len());
            state.pending.extend(paths.into_iter().take(room));
            state.attempts = DELIVERY_ATTEMPTS;
        }
        self.schedule();
    }

    /// Deliver queued requests on the foreground executor, where the
    /// application is not inside another callback.
    fn schedule(&self) {
        let app = {
            let mut state = self.0.borrow_mut();
            if state.scheduled || state.pending.is_empty() {
                return;
            }
            let Some(app) = state.app.clone() else {
                return;
            };
            state.scheduled = true;
            app
        };
        let requests = self.clone();
        app.foreground_executor()
            .spawn(async move {
                requests.0.borrow_mut().scheduled = false;
                requests.deliver();
            })
            .detach();
    }

    fn deliver(&self) {
        let (app, window, view, paths) = {
            let mut state = self.0.borrow_mut();
            let (Some(app), Some(window), Some(view)) =
                (state.app.clone(), state.window, state.view.clone())
            else {
                return;
            };
            if state.pending.is_empty() {
                return;
            }
            (app, window, view, std::mem::take(&mut state.pending))
        };
        let queued = paths.clone();
        let delivered = app.update(|cx| {
            window.update(cx, move |_, window, cx| {
                let Some(reader) = view.upgrade() else {
                    return false;
                };
                reader.update(cx, |reader, cx| reader.open_paths(queued, cx));
                window.activate_window();
                cx.activate(true);
                true
            })
        });
        if matches!(delivered, Ok(true)) {
            return;
        }
        // The window is gone or the application is busy in another callback.
        let mut state = self.0.borrow_mut();
        let mut pending = paths;
        pending.append(&mut state.pending);
        state.pending = pending;
        if state.attempts == 0 {
            eprintln!(
                "Breeze could not open {} requested by the system",
                state.pending.len()
            );
            state.pending.clear();
            return;
        }
        state.attempts -= 1;
        drop(state);
        self.schedule();
    }
}

#[cfg(test)]
mod tests {
    use super::{argument_paths, paths_from_urls};
    use std::{ffi::OsString, path::PathBuf};

    #[test]
    fn flags_are_not_documents() {
        let paths = argument_paths(
            ["-psn_0_4098", "/tmp/report.pdf", "--flag"]
                .into_iter()
                .map(OsString::from),
        );
        assert_eq!(paths, [PathBuf::from("/tmp/report.pdf")]);
    }

    #[test]
    fn file_urls_are_decoded() {
        let paths = paths_from_urls(&[
            "file:///tmp/Breeze%20notes.pdf".to_string(),
            "file://localhost/tmp/second.pdf".to_string(),
            "file:///tmp/%E4%B8%AD%E6%96%87.pdf".to_string(),
        ]);
        assert_eq!(
            paths,
            [
                PathBuf::from("/tmp/Breeze notes.pdf"),
                PathBuf::from("/tmp/second.pdf"),
                PathBuf::from("/tmp/中文.pdf"),
            ]
        );
    }

    #[test]
    fn other_urls_are_ignored() {
        assert!(
            paths_from_urls(&[
                "https://example.com/document.pdf".to_string(),
                "file://server/share/report.pdf".to_string(),
                "breeze://open".to_string(),
            ])
            .is_empty()
        );
    }

    #[test]
    fn malformed_escapes_stay_literal() {
        assert_eq!(
            paths_from_urls(&["file:///tmp/100%25%2.pdf".to_string()]),
            [PathBuf::from("/tmp/100%%2.pdf")]
        );
    }
}
