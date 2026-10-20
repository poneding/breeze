//! Embed the application icon in the Windows executable.
//!
//! GPUI registers its window class with `LoadImageW(module, MAKEINTRESOURCE(1),
//! IMAGE_ICON, …)`, and Windows uses that class icon for the taskbar, Alt-Tab and
//! the window's own chrome. Explorer also reads the executable's icon for the
//! file and for shortcuts, and the installers reference `breeze-app.exe,0` for
//! document associations. The installers carry an icon for their own UI, but
//! without this resource the installed application itself shows the generic
//! program icon.
//!
//! The resource is only needed on Windows; every other build returns before
//! touching the resource compiler.

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    // The script lives at the crate root so its relative icon path resolves from
    // the working directory that both rc.exe and llvm-rc are given.
    println!("cargo:rerun-if-changed=breeze.rc");
    println!("cargo:rerun-if-changed=../../assets/brand/breeze.ico");
    // `manifest_required` also rejects a missing resource compiler, so a Windows
    // build fails instead of silently shipping an icon-less executable.
    embed_resource::compile("breeze.rc", embed_resource::NONE)
        .manifest_required()
        .expect("compile the Windows icon resource");
}
