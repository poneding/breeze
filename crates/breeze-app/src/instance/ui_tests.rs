use crate::{Breeze, Source, open_events::OpenRequests, ui_tests::app};
use core::prelude::v1::test;
use gpui_kit::test::TestWindowExt;
use gpui_kit::*;
use std::path::PathBuf;

#[gpui_kit::test]
fn forwarded_requests_reuse_the_window_and_existing_tabs(cx: &mut TestAppContext) {
    let (window_handle, view) = app(cx);
    let sample = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/samples/field-notes.pdf")
        .canonicalize()
        .unwrap();
    let requests = OpenRequests::new();
    cx.update(|cx| requests.attach(cx.to_async(), window_handle, view.downgrade()));
    cx.update_window(window_handle, |_, _, cx| {
        view.update(cx, |view: &mut Breeze, _| view.restoring = true);
    })
    .unwrap();
    requests.receive_paths(vec![sample.clone()]);
    cx.run_until_parked();
    cx.update_window(window_handle, |_, _, cx| {
        view.update(cx, |view, cx| {
            assert_eq!(view.pending_opens.as_slice(), std::slice::from_ref(&sample));
            view.restoring = false;
            view.open_requested(Vec::new(), cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    for paths in [vec![sample.clone()], Vec::new()] {
        requests.receive_paths(paths);
        cx.run_until_parked();
        cx.update_window(window_handle, |_, window, cx| {
            window.render_frame(cx);
            let reader = view.read(cx);
            assert_eq!(reader.books.len(), 2);
            assert_eq!(
                reader.books[reader.active].source,
                Source::File(sample.clone())
            );
            assert!(reader.storage_error.is_none());
            assert_eq!(cx.windows().len(), 1);
            assert!(window.find("document-tabs").visible());
        })
        .unwrap();
    }
}
