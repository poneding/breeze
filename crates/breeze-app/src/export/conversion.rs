//! Snapshot conversion and conflict-checked publication to a new file.

use crate::*;
use anyhow::{Context as _, Result, ensure};
use std::fs;

const MAX_OUTPUT_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ExportFormat {
    Pdf,
    Markdown,
    Word,
}

impl ExportFormat {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Pdf => tr!("PDF document (.pdf)"),
            Self::Markdown => tr!("Markdown document (.md)"),
            Self::Word => tr!("Word document (.docx)"),
        }
    }
    pub(super) fn extension(self) -> &'static str {
        match self {
            Self::Pdf => "pdf",
            Self::Markdown => "md",
            Self::Word => "docx",
        }
    }
}

pub(super) enum DocumentSnapshot {
    Pdf { fingerprint: String },
    Markdown { text: String, fingerprint: String },
}

pub(super) struct ConversionRequest {
    pub source: Source,
    pub snapshot: DocumentSnapshot,
    pub format: ExportFormat,
    pub destination: PathBuf,
}

pub(super) struct Receipt {
    pub destination: PathBuf,
    pub bytes: u64,
    pub pdf_conversion: Option<ExportFormat>,
    pub empty_pages: usize,
    pub simplified: usize,
}

fn verify_source(source: &Source, expected: &str) -> Result<()> {
    let current = match source {
        Source::File(path) => breeze_storage::fingerprint(path)?,
        Source::Sample => breeze_storage::bytes_fingerprint(include_bytes!(
            "../../../../assets/samples/field-notes.pdf"
        )),
    };
    ensure!(current == expected, "Source changed; export refused");
    Ok(())
}

/// All parsing, codecs, PDF objects and filesystem work run outside the UI.
pub(super) fn convert(request: ConversionRequest) -> Result<Receipt> {
    ensure!(
        request
            .destination
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case(request.format.extension())),
        "The export filename must end in .{}",
        request.format.extension()
    );
    ensure!(
        !request.destination.exists(),
        "A file already exists at {}; choose a different name",
        request.destination.display()
    );
    let parent = request
        .destination
        .parent()
        .context("The destination has no directory")?;
    ensure!(parent.is_dir(), "The destination directory is missing");
    let fingerprint = match &request.snapshot {
        DocumentSnapshot::Pdf { fingerprint } | DocumentSnapshot::Markdown { fingerprint, .. } => {
            fingerprint
        }
    };
    verify_source(&request.source, fingerprint)?;
    let temporary =
        tempfile::NamedTempFile::new_in(parent).context("Cannot create temporary export")?;
    let mut assets = None;
    let mut receipt = Receipt {
        destination: request.destination.clone(),
        bytes: 0,
        pdf_conversion: None,
        empty_pages: 0,
        simplified: 0,
    };
    match &request.snapshot {
        DocumentSnapshot::Pdf { fingerprint } => {
            let text = breeze_pdf::text_export::extract(&request.source, fingerprint)?;
            receipt.pdf_conversion = Some(request.format);
            receipt.empty_pages = text.empty_pages;
            match request.format {
                ExportFormat::Markdown => {
                    let directory = tempfile::Builder::new()
                        .prefix("breeze-export-assets-")
                        .tempdir_in(parent)?;
                    let link = directory
                        .path()
                        .file_name()
                        .context("Invalid export assets directory")?
                        .to_string_lossy();
                    text.write_markdown(temporary.path(), directory.path(), &link)?;
                    if text.images > 0 {
                        assets = Some(directory);
                    }
                }
                ExportFormat::Word => text.write_word(temporary.path())?,
                ExportFormat::Pdf => anyhow::bail!("Unsupported document conversion"),
            }
        }
        DocumentSnapshot::Markdown { text, .. } => {
            let base = match &request.source {
                Source::File(path) => path,
                Source::Sample => anyhow::bail!("Unsupported document conversion"),
            };
            let document = breeze_reflow::export::prepare_markdown(text, base)?;
            receipt.simplified = document.simplified;
            match request.format {
                ExportFormat::Pdf => document.write_pdf(temporary.path())?,
                ExportFormat::Word => document.write_word(temporary.path())?,
                ExportFormat::Markdown => anyhow::bail!("Unsupported document conversion"),
            }
        }
    }
    receipt.bytes = fs::metadata(temporary.path())?.len();
    ensure!(
        receipt.bytes > 0 && receipt.bytes <= MAX_OUTPUT_BYTES,
        "Export exceeds the 64 MiB output limit"
    );
    temporary.as_file().sync_all()?;
    verify_source(&request.source, fingerprint)?;
    temporary
        .persist_noclobber(&request.destination)
        .context("Cannot publish export; existing files are preserved")?;
    if let Some(assets) = assets {
        let _ = assets.keep();
    }
    Ok(receipt)
}

#[cfg(test)]
mod tests;
