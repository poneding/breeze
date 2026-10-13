# Work Package 05: Standard PDF Annotation Export

Status: implemented as a restricted development exporter after this plan was written. Controlled fixtures passed independent parsing/rendering and the native save-dialog workflow; broader corpus/release acceptance is still pending. This package does not modify source page content.

## User-visible contract

- Export the current annotation snapshot to a **new** PDF. Keep source PDF bytes and sidecar state unchanged.
- Preserve every existing page content stream and annotation. Do not flatten pages or annotations.
- Export shape/ink/markup annotations with explicit appearance streams so viewers do not need to invent their initial appearance.
- Export a text note as a standard PDF **sticky note** (`Text`), with Unicode contents and a note icon. Disclose that it is not the inline note box used by Breeze's canvas; no silent font substitution or loss of note text.
- Reject encrypted files, forms, digital signatures, unsupported geometry, stale source fingerprints, and any existing destination. This is an intentionally restricted initial exporter.
- Exporting does not mark pending sidecar edits as saved. Errors remain visible; no destination is published before validation succeeds.

## Concrete changes, in order

1. Add pinned `lopdf = 0.45.0` as a pure Rust writer dependency of `breeze-pdf`, not a replacement renderer. Retain existing PDF Oxide/Office Oxide pins.
2. Add `crates/breeze-pdf/src/export.rs` with a snapshot-based request and receipt. Validate input and destination before serialization.
3. Add `export/appearance.rs` to produce Form XObject normal appearances. Keep the implementation independent of GPUI and the PDF engine's DOM types.
4. Map Highlight/Underline/StrikeOut to QuadPoints; region/rectangle to Square; ellipse to Circle; line/arrow to Line/LE; pen/free wavy path to Ink; note to Text/Unicode Contents.
5. Reuse existing page references; append to a copied page Annots array rather than mutating a shared array. Keep original Contents and Resources untouched.
6. Serialize to a same-directory temporary file. Reopen with the writer and the independent existing parser; check page count, added annotations, appearance resources, unchanged source objects/page-content streams, and signatures of all newly serialized annotation/appearance objects. Recheck the source fingerprint, then publish without overwriting any existing path.
7. Add `crates/breeze-app/src/export.rs`: native save dialog, single in-flight job, immutable annotation/source snapshot, close protection, success/error receipt, and an explicit note-semantics explanation. Add an Action/Tooltip binding and a contextual export button.
8. Add unit/integration fixtures and an ignored visual-fixture test that writes only beneath `target/`. Validate with at least two independent viewer engines before treating export as generally available.

## Limits and cost controls

- Reuse the 256 MiB source limit and 5,000 annotation / 100,000 point-or-quad limits.
- Bound temporary output and verification allocations. Avoid rendering all document pages on the UI thread.
- Export is an explicit background operation, not part of the pen-input or autosave path.
- Keep exported appearances vector-based. No bitmap fallback and no bundled new font system for this package.

## Automated acceptance

- All eleven stored annotation kinds round-trip with the expected standard subtype; erasure has no export object.
- Existing page streams/resources, outline, images, and existing annotation entries survive export.
- Unicode note contents survive exact decoding; note appearance uses a disclosed icon rather than guessed font glyphs.
- Original/alias/existing destinations, changed source, invalid page/style/geometry, encrypted/form/signed documents, and excessive output fail without publishing a file.
- A source with no existing Annots and a source with an indirect/shared Annots array are both covered.
- Generated Appearance streams are nonempty, have valid BBox/resources, and render within their intended bounds.

## Manual acceptance and gate

Render generated examples with PDF Oxide and an independent renderer; inspect them in a second independent viewer where available. Check text-note contents and editable annotation metadata, not only screenshots. External validation tools are test-only and never application dependencies.

If the second-viewer check cannot be completed or a fidelity defect is found, retain a clearly labeled development-only export path and record the limitation. Do not describe that as broad PDF compatibility or a signed/archival-file preservation guarantee.

## Execution results and limits

- Implemented writer/appearance modules, immutable UI snapshots, shortcut/save dialog, in-flight close protection, receipts, and non-clobbering publication.
- The controlled fixture includes all eleven stored kinds. Poppler and macOS PDFKit displayed the exported shapes/markup/ink/note icon. Pypdf and PDFKit read back the exact Chinese/emoji note contents.
- In Poppler at 144 DPI, hiding exported annotations produced a page background pixel-identical to the source.
- Native macOS UI exported a project-owned annotation through the actual save dialog into an isolated directory; the output was independently reopened.
- PDF Oxide 0.3.78's high-level annotation API incorrectly decodes UTF-16 Contents as UTF-8. Valid UTF-16 output is retained; the reader defect is recorded, not worked around with non-standard output.
- Inherited MediaBox/CropBox/Rotate values are validated; older supported source headers are raised to 1.7 for the introduced appearance features.
- The exporter also rejects object counts above 250,000, object identifiers above 1,000,000, nesting at/above 64, and output above 512 MiB. These limits do not establish a total process-memory guarantee.
- Controlled interoperation does not certify arbitrary existing annotations, malformed PDFs, archival conformance, full-document performance, or imported-annotation editing.
