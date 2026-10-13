use super::*;
use gpui_kit::test::{TestAppContextExt, TestWindowExt};

fn damaged_image_pdf() -> Vec<u8> {
    let mut bytes = b"%PDF-1.7\n".to_vec();
    let mut offsets = vec![0];
    for (index, object) in [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R 6 0 R] /Count 2 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Resources << /XObject << /Broken 5 0 R >> >> /Contents 4 0 R >>",
        "<< /Length 10 >>\nstream\n/Broken Do\nendstream",
        "<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /BitsPerComponent 8 /ColorSpace /DeviceRGB /Filter /DCTDecode /Length 8 >>\nstream\nbad JPEG\nendstream",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Resources << >> >>",
    ].into_iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", index + 1).as_bytes());
    }
    let xref = bytes.len();
    bytes.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len()).as_bytes());
    for offset in offsets.iter().skip(1) {
        bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            offsets.len()
        )
        .as_bytes(),
    );
    bytes
}

/// The stock engine logs a failed image draw and returns a successful raster with
/// that content missing, so the reader cannot tell an incomplete page from a
/// complete one. This test states the requirement it does not currently meet: it
/// passed against the retired `vendor/pdf_oxide` copy, whose rendering-integrity
/// change made those draws fail. Re-enable it when that change reaches an
/// upstream release.
#[ignore = "requires the retired engine patch that propagates draw failures"]
#[gpui_kit::test]
async fn damaged_image_shows_a_real_page_error_and_navigation_recovers(cx: &mut TestAppContext) {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("broken.pdf");
    let before = damaged_image_pdf();
    std::fs::write(&source, &before).unwrap();
    let (handle, view) = crate::ui_tests::app(cx);
    view.update(cx, |view, cx| {
        view.books[0].source = Source::File(source.clone());
        view.books[0].state.layout = ReadingLayout::Single;
        view.load(cx);
    });
    cx.wait_for(handle, std::time::Duration::from_secs(10), |_, cx| {
        !view.read(cx).loading
    })
    .await;
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .unwrap();
    cx.wait_for(handle, std::time::Duration::from_secs(10), |_, cx| {
        !view.read(cx).raster.errors.is_empty()
    })
    .await;
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let error = window.find(("page-render-error", 0usize));
        assert!(error.visible());
        let page = window.find("page-surface").bounds();
        assert!(error.bounds().right() <= page.right());
        assert!(
            view.read(cx)
                .raster
                .errors
                .values()
                .any(|message| message.contains("第 1 页渲染失败"))
        );
        view.update(cx, |view, cx| view.next_page(1, cx));
    })
    .unwrap();
    cx.wait_for(handle, std::time::Duration::from_secs(10), |_, cx| {
        !view.read(cx).loading
    })
    .await;
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .unwrap();
    cx.wait_for(handle, std::time::Duration::from_secs(10), |_, cx| {
        view.read(cx).raster.visible_ready()
    })
    .await;
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(view.read(cx).books[0].state.navigation.page(), 1);
        assert!(window.try_find(("page-render-error", 0usize)).is_none());
        assert!(!view.read(cx).raster.cache.is_empty());
    })
    .unwrap();
    assert_eq!(std::fs::read(source).unwrap(), before);
}
