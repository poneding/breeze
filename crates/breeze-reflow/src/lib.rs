//! Bounded, read-only EPUB and Markdown adapters. Engine objects never cross workers.
pub mod epub;
pub mod markdown;
mod worker;
pub use worker::EpubWorker;

use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path};

pub(crate) fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let file = File::open(path).with_context(|| format!("Cannot open {}", path.display()))?;
    ensure!(
        file.metadata()?.is_file(),
        "The document is not a regular file"
    );
    ensure!(
        file.metadata()?.len() <= limit as u64,
        "Document exceeds the {} MiB input limit",
        limit / 1024 / 1024
    );
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= limit, "Document grew beyond its input limit");
    Ok(bytes)
}

pub(crate) fn fingerprint(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
