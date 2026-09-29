# Product Scope

## Confirmed requirements

- Breeze is a local, cross-format reading workspace. PDF comes first; EPUB and Markdown may be added later.
- Use Rust for application logic, UI, and document processing. System API bindings are allowed; C/C++ PDF engines and WebView-based document rendering are not.
- Support macOS, Windows, and Linux, with native validation on each platform before release.
- Default annotation persistence is a sidecar file, with explicit export to a new PDF.
- Existing text and image edits must change the actual source content or selected drawing instance. Covering content, erasing a region and appending replacement text, or rasterizing a page is not an acceptable substitute.
- Defer OCR, password/encrypted documents, form editing, digital signatures, and DRM.
- Report inadequate performance or fidelity before adopting compromises. Do not silently add fallback engines or reduce quality targets.

## Product targets

These are requirements, not a list of completed prototype features.

| Area | Target |
|---|---|
| Home | Recent documents, favorites, and categories using references to local files |
| Reading | Document tabs, opening PDFs, page navigation, page-number entry, and zoom/fit modes |
| Navigation | Collapsible embedded outline and thumbnail panel, loaded on demand |
| Annotations | Text highlight, area highlight, underline, strikeout, pen, eraser, text note, circle, rectangle, line, wavy line, and arrow |
| Existing text editing | Font, foreground/background color, opacity, size, bold, italic, and alignment within a defined text run/block |
| Existing image editing | Scale, rotate, and delete one selected drawing instance without changing other shared-resource uses |
| Insertion | New text and image objects, separate from replacement of existing content |
| Settings | Default-file association, language, theme, update policy, opening-window behavior, session restoration, and reading-position restoration |

Continuous and two-page layouts remain proposed reading enhancements. Text selection is a prerequisite for semantic text markup; search is a proposed companion capability rather than a completed feature.

## Scope boundaries

- The eraser operates on editable annotation ink, not original page content. Whole-stroke erasure is the initial proposal; partial-stroke erasure needs a separate cost/experience decision.
- Text markup follows glyph quadrilaterals. A freehand wavy line is a path, not a text range.
- Editing a text block does not imply Word-style page-wide reflow. Block boundaries and alignment semantics must be explicit.
- Missing glyphs must not silently become wrong characters. An explicitly selected, embeddable replacement font may be proposed with a preview.
- Do not add cloud sync, telemetry, AI features, a plugin system, mobile clients, or a browser version without a new requirement.

## Prototype baseline

The compact UI direction is accepted. The prototype is read-only and implements single-page rendering, session-only library state, outlines, nearby thumbnails, and navigation. Annotation/editing controls remain previews; see [UI design](04-design.md).

The current category control assigns one category per entry. Whether the product should instead support multiple tags remains undecided; do not describe the current code as a multi-tag implementation.

## Decisions still required

- Confirm minimum OS versions and supported CPU architectures before release commitments.
- Define text-run/block editing, alignment, and reflow boundaries through targeted validation.
- Decide category semantics and the initial scope of existing-annotation editing.
- On a source fingerprint mismatch, stop automatic sidecar application and require explicit reassociation. Do not build speculative automatic migration machinery.
