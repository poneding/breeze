# Product Scope

## Confirmed requirements

- Breeze is a local, cross-format reading workspace. PDF comes first; EPUB and Markdown may be added later.
- Use Rust for application logic and UI, with the `mupdf-rs` Rust API over MuPDF's C/C++ implementation for PDF processing. PDFium, Poppler, and WebView-based document rendering are not product engines.
- Distribute Breeze and the corresponding source under AGPL-3.0-only, including required MuPDF notices and build/installation scripts.
- Support macOS, Windows, and Linux, with native validation on each platform before release.
- Default annotation persistence is a sidecar file, with explicit export to a new PDF.
- PDF page content is read-only. Breeze does not expose original text or image editing, and annotations are stored separately from the source document.
- Defer OCR, password/encrypted documents, forms, digital signatures, and DRM.
- Report inadequate performance or fidelity before adopting compromises. Do not silently add fallback engines or reduce quality targets.

## Product targets

These are requirements, not a list of completed prototype features.

| Area | Target |
|---|---|
| Home | Recent documents, favorites, and categories using references to local files |
| Reading | Document tabs, opening PDFs, page navigation, page-number entry, and zoom/fit modes |
| Navigation | Collapsible embedded outline and thumbnail panel, loaded on demand |
| Annotations | Text highlight, area highlight, underline, strikeout, pen, eraser, text note, circle, rectangle, line, wavy line, and arrow |
| Settings | Default-file association, language, theme, update policy, opening-window behavior, session restoration, and reading-position restoration |

Single/two-page arrangement and continuous scrolling are independent reading settings, with continuous single-page as the default. Layout and zoom choices are remembered. Text selection is a prerequisite for semantic text markup; consult delivery status for the implemented selection and search subset.

## Scope boundaries

- The eraser operates on editable annotation ink, not original page content. Whole-stroke erasure is the initial proposal; partial-stroke erasure needs a separate cost/experience decision.
- Text markup follows glyph quadrilaterals. A freehand wavy line is a path, not a text range.
- Do not add cloud sync, telemetry, AI features, a plugin system, mobile clients, or a browser version without a new requirement.

## Prototype baseline

The compact UI direction is accepted. The source PDF remains read-only. The local reader now persists library/settings and reading state, supports search and glyph-bounds selection, and draws/saves independent sidecar annotations. Restricted annotation export is implemented. PDF page content is never edited; see [delivery status](../plans/STATUS.md).

A document may belong to multiple user-created categories. Removing a category removes only that membership; a document is Unassigned when it has no category memberships.

## Decisions still required

- Confirm minimum OS versions and supported CPU architectures before release commitments.
- Decide the initial scope of existing-annotation editing.
- On a source fingerprint mismatch, stop automatic sidecar application and require explicit reassociation. Do not build speculative automatic migration machinery.
