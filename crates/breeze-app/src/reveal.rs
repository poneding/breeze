//! Hand a library document to the platform's file manager.
//!
//! Reveal is read-only shell integration: the launcher selects the document in
//! Finder, File Explorer or the desktop file manager and never opens or edits
//! the file itself.

use std::{io, path::Path, process::Command};

/// Catalog key for the reveal action.
///
/// File managers are platform terms, so the catalog carries one entry per
/// platform and the UI renders the one that matches the running system.
pub(super) fn label() -> &'static str {
    if cfg!(target_os = "macos") {
        "Reveal in Finder"
    } else if cfg!(target_os = "windows") {
        "Show in File Explorer"
    } else {
        "Show in File Manager"
    }
}

/// Why the document cannot be shown in a file manager.
#[derive(Debug)]
pub(super) enum Error {
    /// The saved path is gone, so there is nothing to select.
    Missing,
    /// The platform launcher could not be started.
    Launch(io::Error),
}

/// Show `path` in the platform's file manager.
pub(super) fn reveal(path: &Path) -> Result<(), Error> {
    if !path.is_file() {
        return Err(Error::Missing);
    }
    let mut child = command(path).spawn().map_err(Error::Launch)?;
    // The launcher is reaped so repeated reveals leave no zombie behind.
    let status = child.wait().map_err(Error::Launch)?;
    // Windows Explorer reports success as exit code 1, so only the other
    // launchers treat a non-zero status as a failure worth reporting.
    #[cfg(not(target_os = "windows"))]
    if !status.success() {
        return Err(Error::Launch(io::Error::other(format!(
            "file manager exited with {status}"
        ))));
    }
    #[cfg(target_os = "windows")]
    let _ = status;
    Ok(())
}

#[cfg(target_os = "macos")]
fn command(path: &Path) -> Command {
    // `-R` selects the document in Finder instead of opening it.
    let mut command = Command::new("open");
    command.arg("-R").arg(path);
    command
}

#[cfg(target_os = "windows")]
fn command(path: &Path) -> Command {
    let mut command = Command::new("explorer");
    command.arg(format!("/select,{}", path.display()));
    command
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn command(path: &Path) -> Command {
    // The freedesktop file-manager interface needs a D-Bus client and is not
    // available on every desktop, so the containing folder opens through
    // xdg-open instead of selecting the file.
    let directory = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut command = Command::new("xdg-open");
    command.arg(directory);
    command
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{Language, text_in};

    /// The catalog scan cannot see a call whose key comes from `label()`, so the
    /// platform keys are checked directly.
    #[test]
    fn every_platform_action_is_catalogued() {
        for key in [
            "Reveal in Finder",
            "Show in File Explorer",
            "Show in File Manager",
        ] {
            for language in [Language::SimplifiedChinese, Language::TraditionalChinese] {
                assert_ne!(text_in(key, language), key, "{key}: {language:?}");
            }
        }
    }

    #[test]
    fn missing_documents_are_not_launched() {
        let error = reveal(Path::new("missing-breeze-reveal-fixture.pdf")).unwrap_err();
        assert!(matches!(error, Error::Missing));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn finder_selects_the_document() {
        let command = command(Path::new("/tmp/example.pdf"));
        assert_eq!(command.get_program(), "open");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            ["-R", "/tmp/example.pdf"]
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn explorer_selects_the_document() {
        let command = command(Path::new(r"C:\tmp\example.pdf"));
        assert_eq!(command.get_program(), "explorer");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [r"/select,C:\tmp\example.pdf"]
        );
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    #[test]
    fn xdg_open_opens_the_containing_folder() {
        let command = command(Path::new("/tmp/example.pdf"));
        assert_eq!(command.get_program(), "xdg-open");
        assert_eq!(command.get_args().collect::<Vec<_>>(), ["/tmp"]);
    }
}
