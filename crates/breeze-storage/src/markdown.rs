//! Markdown source loading, publication and bounded recovery drafts.
//!
//! The PDF sidecar machinery is deliberately not reused: a Markdown draft is a
//! *replacement* for a text file the user owns, so the rules are different. The
//! source is read once and hashed from those same bytes, publication is
//! conflict-checked against the destination the draft was started from, and a
//! crash recovery record keeps whatever was not yet published.
use crate::{atomic_write, bytes_fingerprint};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
};

/// One document's source length cap, shared with the reader and the editor.
pub const MAX_SOURCE_BYTES: usize = 16 * 1024 * 1024;
/// Recovery records retained for one application-data directory.
pub const MAX_RECORDS: usize = 64;
/// Total bytes the recovery directory may hold before older clean records go.
pub const RECOVERY_BUDGET: u64 = 256 * 1024 * 1024;
const RECORD_VERSION: u32 = 1;

/// Source bytes together with the fingerprint of those exact bytes.
///
/// Reading and hashing in one step matters: hashing a second read could
/// fingerprint a file that changed in between and silently accept a draft
/// against content the user never saw.
pub struct LoadedSource {
    pub bytes: Vec<u8>,
    pub fingerprint: String,
}

pub fn load(path: &Path) -> Result<LoadedSource> {
    let mut file = File::open(path).with_context(|| format!("Cannot open {}", path.display()))?;
    ensure!(
        file.metadata()?.is_file(),
        "The document is not a regular file"
    );
    let mut bytes = Vec::new();
    file.by_ref()
        .take(MAX_SOURCE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= MAX_SOURCE_BYTES,
        "Markdown exceeds the {} MiB input limit",
        MAX_SOURCE_BYTES / 1024 / 1024
    );
    std::str::from_utf8(&bytes).context("Markdown is not valid UTF-8")?;
    let fingerprint = bytes_fingerprint(&bytes);
    Ok(LoadedSource { bytes, fingerprint })
}

/// What the destination looked like when the draft adopted it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// Nothing exists at the path yet; publication must not clobber.
    New,
    /// The path held this content when the draft adopted it.
    Existing { fingerprint: String },
}

/// The fingerprint of the bytes that were actually published.
pub fn publish(path: &Path, bytes: &[u8], target: &Target) -> Result<String> {
    let parent = path.parent().context("The destination has no directory")?;
    ensure!(parent.is_dir(), "The destination directory is missing");
    match target {
        Target::New => {
            ensure!(
                !path.exists(),
                "A file already exists at {}; choose a different name",
                path.display()
            );
            atomic_write(path, bytes, false)?;
        }
        Target::Existing { fingerprint } => {
            let metadata = fs::symlink_metadata(path)
                .with_context(|| format!("Cannot inspect {}", path.display()))?;
            ensure!(
                metadata.file_type().is_file(),
                "Refusing to replace a path that is not a regular file"
            );
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                ensure!(
                    metadata.nlink() == 1,
                    "Refusing to replace a file with more than one hard link"
                );
            }
            // The lock is advisory: another editor cannot be excluded, so the
            // fingerprint is rechecked immediately before publication and the
            // receipt reports only what was written.
            let lock_path = path.with_extension("md.breeze.lock");
            let lock = OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .open(&lock_path)
                .with_context(|| format!("Cannot lock {}", path.display()))?;
            lock.try_lock().map_err(|_| {
                anyhow::anyhow!("Another Breeze save is already writing this document")
            })?;
            let current = load(path)?.fingerprint;
            ensure!(
                current == *fingerprint,
                "The document changed on disk since this draft started; save to a new file"
            );
            atomic_write(path, bytes, true)?;
        }
    }
    Ok(bytes_fingerprint(bytes))
}

/// An unsaved draft kept across a crash or a forced quit.
///
/// The record is a draft, never a save: restoring it must not acknowledge a
/// file, and it is deleted only after a successful publication.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Draft {
    pub version: u32,
    /// Absolute path of the document this draft belongs to.
    pub document: String,
    /// Where an explicit save would go, once the draft has a chosen target.
    pub target: Option<String>,
    /// Fingerprint of the source when the draft began.
    pub baseline: String,
    /// Fingerprint of the last published revision, if any.
    pub published: Option<String>,
    /// Whether the draft still differs from what was published.
    pub dirty: bool,
    pub selection: (usize, usize),
    /// Seconds since the Unix epoch, for the restore notice and pruning.
    pub saved_at: u64,
    /// The draft source itself; Markdown is valid UTF-8 by construction.
    pub source: String,
}

impl Draft {
    pub fn is_valid(&self) -> bool {
        self.version == RECORD_VERSION
            && !self.document.is_empty()
            && self.document.len() <= 4096
            && self.source.len() <= MAX_SOURCE_BYTES
            && self.selection.0 <= self.source.len()
            && self.selection.1 <= self.source.len()
    }
}

/// The recovery directory inside an application-data directory.
pub fn recovery_directory(root: &Path) -> PathBuf {
    root.join("markdown-recovery")
}

/// One record per document, named by a hash of its path so the name is stable
/// across runs and independent of the filesystem's name limits.
pub fn recovery_path(root: &Path, document: &Path) -> PathBuf {
    let key = bytes_fingerprint(document.to_string_lossy().as_bytes());
    recovery_directory(root).join(format!("{}.json", &key[..32]))
}

pub fn write_recovery(root: &Path, draft: &Draft) -> Result<()> {
    ensure!(
        draft.is_valid(),
        "Refusing to write an invalid recovery record"
    );
    let directory = recovery_directory(root);
    fs::create_dir_all(&directory)?;
    let path = recovery_path(root, Path::new(&draft.document));
    let bytes = serde_json::to_vec(draft)?;
    ensure!(
        bytes.len() as u64 <= MAX_SOURCE_BYTES as u64 * 2,
        "The recovery record exceeds its size limit"
    );
    atomic_write(&path, &bytes, true)
}

pub fn read_recovery(root: &Path, document: &Path) -> Result<Option<Draft>> {
    let path = recovery_path(root, document);
    let Ok(bytes) = fs::read(&path) else {
        return Ok(None);
    };
    // A damaged record is reported rather than silently discarded.
    let draft: Draft = serde_json::from_slice(&bytes)
        .with_context(|| format!("Recovery record {} is invalid", path.display()))?;
    ensure!(draft.is_valid(), "Recovery record has an unsupported shape");
    Ok(Some(draft))
}

pub fn clear_recovery(root: &Path, document: &Path) -> Result<()> {
    let path = recovery_path(root, document);
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

/// Keep the recovery directory inside its budget.
///
/// Clean records go first, oldest first; the record being written right now is
/// never a candidate, and a draft that is still dirty is only removed once
/// every clean record is gone. Returns the bytes still held.
pub fn prune_recovery(root: &Path, protect: &Path) -> Result<u64> {
    let directory = recovery_directory(root);
    let protected = recovery_path(root, protect);
    let mut records: Vec<(std::time::SystemTime, u64, PathBuf, bool)> = Vec::new();
    let Ok(entries) = fs::read_dir(&directory) else {
        return Ok(0);
    };
    let mut total = 0u64;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }
        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        let size = metadata.len();
        total += size;
        let dirty = fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Draft>(&bytes).ok())
            .map(|draft| draft.dirty)
            .unwrap_or(false);
        records.push((
            metadata.modified().unwrap_or(std::time::UNIX_EPOCH),
            size,
            path,
            dirty,
        ));
    }
    if total <= RECOVERY_BUDGET && records.len() <= MAX_RECORDS {
        return Ok(total);
    }
    // Clean records first, then the oldest dirty ones.
    records.sort_by_key(|(modified, _, _, dirty)| (*dirty, *modified));
    while (total > RECOVERY_BUDGET || records.len() > MAX_RECORDS) && !records.is_empty() {
        let (_, size, path, _) = records.remove(0);
        if path == protected {
            // Put the protected record back so pruning cannot delete the draft
            // that is being written.
            records.push((std::time::SystemTime::now(), size, path, true));
            if records.iter().all(|(_, _, path, _)| path == &protected) {
                break;
            }
            continue;
        }
        if fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(size);
        }
    }
    Ok(total)
}
