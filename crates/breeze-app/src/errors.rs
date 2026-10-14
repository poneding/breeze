//! Rendering of pipeline failures in the selected interface language.
//!
//! `breeze-pdf` reports failures as [`PdfError`] variants and keeps pure
//! diagnostics as plain context. Structured failures are worded here, through
//! the catalog; diagnostics translate only when their text is itself a catalog
//! key, so an English detail can never masquerade as translated Chinese.

use crate::*;
use breeze_pdf::PdfError;

/// Message for an error the reader is about to show.
pub(super) fn error_text(error: &anyhow::Error) -> String {
    let structured = error.downcast_ref::<PdfError>();
    // The chain repeats a context layer with its English source text; that text
    // is the structured error itself and is already worded below.
    let repeated = structured.map(|pdf| pdf.to_string());
    let mut skip_repeated = repeated.is_some();
    let mut parts: Vec<String> = Vec::new();
    if let Some(pdf) = structured {
        parts.push(pdf_message(pdf));
    }
    for cause in error.chain() {
        let text = cause.to_string();
        if skip_repeated && Some(&text) == repeated.as_ref() {
            skip_repeated = false;
            continue;
        }
        let translated = i18n::text(&text);
        parts.push(if translated == text {
            text
        } else {
            translated.to_string()
        });
    }
    if parts.is_empty() {
        return format!("{error:#}");
    }
    parts.join(": ")
}

/// Worded failure for a structured PDF error.
pub(super) fn pdf_message(error: &PdfError) -> String {
    match error {
        PdfError::UnsupportedFormat => tr!("Only PDF documents are supported").into(),
        PdfError::OpenFailed { path } => tf!("Cannot open {}", path),
        PdfError::FileTooLarge { limit_mib } => {
            tf!("PDF files are limited to {} MiB", limit_mib)
        }
        PdfError::SourceChangedDuringOpen => {
            tr!("The document changed while it was opening. Try again.").into()
        }
        PdfError::Encrypted => tr!("Encrypted or password-protected PDFs are not supported").into(),
        PdfError::PageCountUnsupported { max } => {
            tf!("Documents must have between 1 and {} pages", max)
        }
        PdfError::NoPages => tr!("The document has no readable pages").into(),
        PdfError::PageUnsupported { page } => {
            tf!("Page {} cannot be displayed by Breeze", page + 1)
        }
        PdfError::PageUnreadable { page } => tf!("Page {} could not be read", page + 1),
        PdfError::PageRenderFailed { page } => tf!(
            "Page {} rendering failed; some content could not be displayed",
            page + 1
        ),
        PdfError::PageOutOfRange { page, total } => {
            tf!(
                "Page {} is outside this document's {} pages",
                page + 1,
                total
            )
        }
        PdfError::UnsupportedZoom => tr!("Unsupported page zoom").into(),
        PdfError::RenderBudget => tr!("This page exceeds the render memory budget").into(),
        PdfError::RegionOutOfBounds => {
            tr!("The requested page region is outside the document").into()
        }
        PdfError::ResolutionMismatch => {
            tr!("The renderer did not return the requested resolution").into()
        }
        PdfError::UnexpectedPixels => {
            tr!("The renderer returned an unexpected pixel buffer").into()
        }
        PdfError::SearchQuery => tr!("Enter a search term of 1–256 bytes").into(),
        PdfError::SearchCancelled => tr!("Search cancelled").into(),
        PdfError::Superseded => tr!("Superseded by a newer request").into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_failures_are_worded_by_the_catalog() {
        let case = |language: i18n::Language, error: PdfError| {
            i18n::set_language(language);
            error_text(&anyhow::Error::new(error))
        };
        assert_eq!(
            case(i18n::Language::SimplifiedChinese, PdfError::Encrypted),
            "不支持加密或带密码的 PDF"
        );
        assert_eq!(
            case(i18n::Language::English, PdfError::Encrypted),
            "Encrypted or password-protected PDFs are not supported"
        );
        assert_eq!(
            case(i18n::Language::TraditionalChinese, PdfError::RenderBudget),
            "頁面超出渲染記憶體預算"
        );
        // Parameters keep their slots in every language.
        assert_eq!(
            case(
                i18n::Language::SimplifiedChinese,
                PdfError::PageRenderFailed { page: 2 }
            ),
            "第 3 页渲染失败，部分内容无法显示"
        );
    }

    #[test]
    fn a_wrapped_failure_is_still_found_and_translated() {
        i18n::set_language(i18n::Language::SimplifiedChinese);
        // The adapter attaches diagnostics as context, so the structured error
        // may sit below another layer in the chain.
        let error = anyhow::anyhow!("engine detail").context(PdfError::PageUnreadable { page: 4 });
        assert_eq!(error_text(&error), "无法读取第 5 页: engine detail");
        // Unknown prose passes through instead of pretending to be translated.
        assert_eq!(
            error_text(&anyhow::anyhow!("MuPDF returned an empty pixmap")),
            "MuPDF returned an empty pixmap"
        );
    }
}
