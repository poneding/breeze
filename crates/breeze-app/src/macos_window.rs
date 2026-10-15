//! macOS-specific window behavior that GPUI does not expose through its options.
use gpui_kit::Window;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSWindowCollectionBehavior};

/// AppKit does not expose the GPUI window to the application until after its
/// first frame, so retry a few frames instead of silently leaving fullscreen
/// participation enabled.
pub(super) fn install(window: &mut Window, attempts: u8) {
    window.on_next_frame(move |window, _| {
        if !use_zoom_for_green_button() && attempts > 1 {
            install(window, attempts - 1);
        }
    });
}

/// Keeps AppKit's green traffic light as a zoom/maximize control. Full-screen
/// participation makes that button create a separate Space; the reader owns its
/// immersive mode and does not want the OS to change the application layout.
pub(super) fn use_zoom_for_green_button() -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    let application = NSApplication::sharedApplication(mtm);
    let Some(window) = application
        .mainWindow()
        .or_else(|| application.keyWindow())
        .or_else(|| application.windows().firstObject())
    else {
        return false;
    };

    let behavior = window.collectionBehavior();
    let behavior = (behavior
        & !(NSWindowCollectionBehavior::FullScreenPrimary
            | NSWindowCollectionBehavior::FullScreenAuxiliary))
        | NSWindowCollectionBehavior::FullScreenNone;
    window.setCollectionBehavior(behavior);
    true
}
