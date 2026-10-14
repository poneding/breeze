//! Failures whose wording belongs to the interface, not to the engine adapter.
//!
//! Every variant renders in English, which is the application catalog's source
//! language and key. The application matches the variant and substitutes its own
//! translation, so the reader never shows Chinese text in an English interface
//! (or the reverse) simply because the message came from the PDF layer.
//!
//! Messages that are pure diagnostics — an engine parse failure, an unexpected
//! MuPDF return — stay as `anyhow` context and are not translated.

use std::{error::Error, fmt, path::Path};

#[derive(Clone, Debug)]
pub enum PdfError {
    /// The selected file is not a PDF.
    UnsupportedFormat,
    /// The file could not be opened at all.
    OpenFailed { path: String },
    /// The PDF is larger than the reader accepts.
    FileTooLarge { limit_mib: u64 },
    /// The file changed while it was being opened.
    SourceChangedDuringOpen,
    /// Encrypted or password-protected documents are unsupported.
    Encrypted,
    /// The document has more pages than the reader accepts.
    PageCountUnsupported { max: usize },
    /// The document has no pages.
    NoPages,
    /// One page uses geometry the reader cannot represent.
    PageUnsupported { page: usize },
    /// One page could not be read from the document.
    PageUnreadable { page: usize },
    /// One page could not be rendered completely.
    PageRenderFailed { page: usize },
    /// A request asked for a page outside the document.
    PageOutOfRange { page: usize, total: usize },
    /// The requested scale is outside the supported range.
    UnsupportedZoom,
    /// The requested raster would exceed the render budget.
    RenderBudget,
    /// The requested view region does not intersect the page.
    RegionOutOfBounds,
    /// The engine did not return the requested resolution.
    ResolutionMismatch,
    /// The engine returned a pixel buffer of an unexpected shape.
    UnexpectedPixels,
    /// A search query must be 1–256 bytes.
    SearchQuery,
    /// A search was cancelled before it finished.
    SearchCancelled,
    /// A newer request replaced this one before it ran.
    Superseded,
}

impl PdfError {
    /// English catalog key. Parameterized keys keep their ordered `{}` slots.
    pub fn key(&self) -> &'static str {
        match self {
            Self::UnsupportedFormat => "Only PDF documents are supported",
            Self::OpenFailed { .. } => "Cannot open {}",
            Self::FileTooLarge { .. } => "PDF files are limited to {} MiB",
            Self::SourceChangedDuringOpen => {
                "The document changed while it was opening. Try again."
            }
            Self::Encrypted => "Encrypted or password-protected PDFs are not supported",
            Self::PageCountUnsupported { .. } => "Documents must have between 1 and {} pages",
            Self::NoPages => "The document has no readable pages",
            Self::PageUnsupported { .. } => "Page {} cannot be displayed by Breeze",
            Self::PageUnreadable { .. } => "Page {} could not be read",
            Self::PageRenderFailed { .. } => {
                "Page {} rendering failed; some content could not be displayed"
            }
            Self::PageOutOfRange { .. } => "Page {} is outside this document's {} pages",
            Self::UnsupportedZoom => "Unsupported page zoom",
            Self::RenderBudget => "This page exceeds the render memory budget",
            Self::RegionOutOfBounds => "The requested page region is outside the document",
            Self::ResolutionMismatch => "The renderer did not return the requested resolution",
            Self::UnexpectedPixels => "The renderer returned an unexpected pixel buffer",
            Self::SearchQuery => "Enter a search term of 1–256 bytes",
            Self::SearchCancelled => "Search cancelled",
            Self::Superseded => "Superseded by a newer request",
        }
    }

    /// Parameters for the `{}` slots of [`Self::key`].
    pub fn parameters(&self) -> Vec<String> {
        match self {
            Self::OpenFailed { path } => vec![path.clone()],
            Self::FileTooLarge { limit_mib } => vec![limit_mib.to_string()],
            Self::PageCountUnsupported { max } => vec![max.to_string()],
            Self::PageUnsupported { page }
            | Self::PageUnreadable { page }
            | Self::PageRenderFailed { page } => vec![(page + 1).to_string()],
            Self::PageOutOfRange { page, total } => {
                vec![(page + 1).to_string(), total.to_string()]
            }
            _ => Vec::new(),
        }
    }

    pub fn open_failed(path: &Path) -> Self {
        Self::OpenFailed {
            path: path.display().to_string(),
        }
    }
}

impl fmt::Display for PdfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut text = self.key().to_string();
        for value in self.parameters() {
            text = text.replacen("{}", &value, 1);
        }
        f.write_str(&text)
    }
}

impl Error for PdfError {}
