# Stage 3: Structural Editing and PDF Export

## Preconditions

Stage 2 sidecars are not portable PDF annotations until exported. The current `pdf_oxide` DOM editing API is not approved for precise original-content editing. Historical exploratory results are not a reusable production editing engine.

## Export work package

The restricted implementation and executed checks are detailed in [work package 05](05-annotation-export.md). This does not advance the structural-editing gate below.

- Write standard annotation subtypes and valid appearance streams into a new PDF using a pure Rust object writer.
- Preserve existing page content and annotations; do not flatten by default.
- Resolve page-object references, rotation/CropBox transforms, fonts for notes, and alpha consistently.
- Reopen output in an independent parser and at least two viewers; compare geometry and editable annotation semantics.
- Refuse to overwrite the source or a changed destination. Do not label an exported file successful before validation completes.

## Structural-editing work package

1. Build source-operator identity and resource-usage mapping for text and images, including graphics/text state.
2. Define a supported subset based on reliable character encoding and exact source identity. Return explicit unsupported reasons elsewhere.
3. Modify the selected operator/instance and isolate shared resources using copy-on-write.
4. Add font selection, encoding/subsetting, color/opacity, bounded text-block alignment, and image transforms only after each operation has fixtures.
5. Integrate hit testing, selection handles, properties, insertion, command history, and Save As into the UI.

## Mandatory rejection tests

- No white rectangles, full-page images, region redaction plus reinsertion, or text-only page reconstruction as replacement editing.
- No modification of a second shared image/Form usage when only one was selected.
- No unexplained changes outside the target region, missing text extraction, or broken font encoding after save/reopen.

## Gate

Provide a representative CJK/Latin/subset/font/transparency/shared-resource corpus and measured per-operation costs before enabling each category. Stop and report failures rather than silently broadening an unsafe subset. This stage is not considered complete merely because controls are present.
