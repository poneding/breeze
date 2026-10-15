//! Document capabilities and durable locations, independent of rendering engines.
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentFormat {
    Pdf,
    Epub,
    Markdown,
}

impl DocumentFormat {
    pub fn from_path(path: &Path) -> Option<Self> {
        match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "pdf" => Some(Self::Pdf),
            "epub" => Some(Self::Epub),
            "md" | "markdown" => Some(Self::Markdown),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Pdf => "PDF",
            Self::Epub => "EPUB",
            Self::Markdown => "Markdown",
        }
    }

    pub fn capabilities(self) -> DocumentCapabilities {
        let pdf = self == Self::Pdf;
        DocumentCapabilities {
            thumbnails: pdf,
            fixed_pages: pdf,
            annotations: pdf,
            rotate: pdf,
            text_size: !pdf,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DocumentCapabilities {
    pub thumbnails: bool,
    pub fixed_pages: bool,
    pub annotations: bool,
    pub rotate: bool,
    pub text_size: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EpubLocation {
    /// A package-relative spine href, optionally including a publication anchor.
    pub href: String,
    /// Position within the chapter. Pagination changes do not become PDF locations.
    pub progression: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ReflowState {
    pub epub: EpubLocation,
    pub markdown_offset: usize,
    pub scroll_offset: f32,
    pub progress: f32,
    pub font_size: u8,
    /// Markdown only: read with the full window width instead of the readable
    /// maximum. EPUB ignores it; a reflowed page is laid out at the viewport.
    pub full_width: bool,
}

impl Default for ReflowState {
    fn default() -> Self {
        Self {
            epub: EpubLocation::default(),
            markdown_offset: 0,
            scroll_offset: 0.,
            progress: 0.,
            font_size: 18,
            full_width: false,
        }
    }
}

impl ReflowState {
    pub fn is_valid(&self) -> bool {
        (12..=32).contains(&self.font_size)
            && self.epub.href.len() <= 4096
            && self.epub.progression.is_finite()
            && (0.0..=1.0).contains(&self.epub.progression)
            && self.progress.is_finite()
            && (0.0..=1.0).contains(&self.progress)
            && self.scroll_offset.is_finite()
            && (0.0..=1_000_000.).contains(&self.scroll_offset)
            && self.markdown_offset <= 16 * 1024 * 1024
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_do_not_inherit_pdf_operations() {
        for extension in ["epub", "EPUB", "md", "markdown"] {
            let format =
                DocumentFormat::from_path(Path::new(&format!("book.{extension}"))).unwrap();
            assert!(!format.capabilities().fixed_pages);
            assert!(!format.capabilities().thumbnails);
            assert!(!format.capabilities().annotations);
            assert!(format.capabilities().text_size);
        }
        assert!(DocumentFormat::from_path(Path::new("unknown.txt")).is_none());
        assert!(DocumentFormat::Pdf.capabilities().annotations);
    }

    #[test]
    fn invalid_restored_locations_are_rejected() {
        let mut state = ReflowState::default();
        assert!(state.is_valid());
        state.epub.progression = f32::NAN;
        assert!(!state.is_valid());
        state.epub.progression = 0.;
        state.font_size = 255;
        assert!(!state.is_valid());
    }
}
