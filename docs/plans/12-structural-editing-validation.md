# Work Package 12: Validate Precise PDF Content Editing

Status: **proposed after the October 3, 2026 research**. No code in this work package has been implemented or accepted. This plan does not enable original-content editing or change the existing engine decision.

Evidence and alternatives: [PDF editing research](../specs/07-pdf-editing-research.md). Authority and invariants: [product scope](../specs/00-product.md), [editing contract](../specs/03-pdf-editing.md), and [Stage 3](03-editing-export.md).

## Outcome

Determine whether the pinned reader plus `lopdf 0.45.0` can support a useful, exactly identified subset of text/image edits, with native selection, correct reopened output, and an explicit refusal path. Produce reproducible evidence before committing to broad editor coverage or a schedule.

The first milestone is a validated source-addressing and mutation prototype, not a toolbar of unsupported controls. No new crate, renderer replacement, cloud service, OCR, or high-level redaction/reinsertion path is proposed.

## Phase A: Source identity and no-op round trips

1. Inspect the reader's content interpreter for hooks that retain stream/operator identity, resource scope, glyph clusters, and Form invocation paths. Compare its identifiers with the objects loaded by `lopdf` from the same immutable source bytes. Do not join glyphs using text/rectangle similarity.
2. Prototype a checked source-span adapter for decoded content. Preserve original stream boundaries and bytes outside edited spans; maintain interpreter state across `/Contents` arrays. Bound recursion, operations, decompressed bytes, and pending page work.
3. Return opaque targets tied to a document revision, plus per-operation capabilities/rejection reasons. Keep PDF engine types out of `breeze-app` and `breeze-core`.
4. Validate no-op read/write/reopen and source-address stability on the declared corpus. Inventory retained source structures, including annotations and marked content. Record the current unsupported fraction explicitly.

**Gate:** every supported selected target maps to a unique drawing invocation and encoded span; stale/ambiguous targets fail closed. No-op output preserves active semantics and independently rendered appearance. If achieving provenance requires a broad parser/renderer fork, report scope and options before broadening the design.

## Phase B: One image usage

Support an ordinary top-level image XObject usage first: scale, rotate, and delete that usage. Include shared image data across pages and repeated use on the same page. Change only the selected call/local transform; preserve later graphics state. Reject unsupported clipping/transparency contexts until fixtures establish their behavior.

Then validate copy-on-write for shared page streams and nested Forms. Clone only the necessary path and resource dictionaries; remap only the selected `Do` when the resource name is used more than once. Do not change every invocation by replacing one shared dictionary entry.

**Gate:** other uses remain semantically and visually unchanged, and undo/redo/reopen reproduce the expected selected-instance behavior. A top-level success does not accept nested Forms.

## Phase C: Existing-font text runs

Start with horizontal, uncomplicated text runs whose Unicode/code/glyph mapping and widths are verified. Replace the actual source operand; preserve unrelated operations and continuation matrices/spacing. Test both `Tj` and `TJ`, different replacement widths, multiple operators, and repeated strings. Do not restrict the research corpus to ASCII even if the first implementation subset is smaller.

Explicitly reject unresolved font encodings, absent glyphs, Type 3 text, vertical writing, text clipping, ambiguous ligatures, and unhandled `/ActualText` or structure associations. Tests must prove that the rejection is visible and leaves the document untouched.

**Gate:** replacement text copies/searches correctly after reopening; the selected original occurrence is absent from active content; unselected matching text remains; following text and graphics retain their intended positions/state. No secure-erasure claim follows from these checks.

## Phase D: CJK fonts and selected blocks

Treat each expansion as a separately verified change: an explicitly chosen replacement font, coherent embedding/subsetting and Unicode mapping, real bold/italic faces, isolated color/opacity, then selected-line/block alignment and wrapping. Evaluate Rust shaping and font components already present in the dependency graph before adding direct dependencies.

Font reuse requires actual character/cluster coverage. A subset lacking a newly typed Chinese character must produce a supported font-choice flow or an explicit refusal. Test punctuation, mixed Latin/CJK, multi-codepoint clusters, and font embedding restrictions. Define overflow handling without automatic silent shrink-to-fit or page-wide reflow.

**Gate:** independent extraction/rendering, correct font resources/metrics, and predictable layout within the selected block. Do not advertise general Chinese editing based only on one synthetic font.

## Phase E: Native interaction and safe saves

1. Add an Edit mode with supported object bounds, contextual properties, native text entry and IME, image handles, focus-safe shortcuts, and concise Chinese failure explanations. Preserve 34/38/32 px chrome heights and existing Kit controls.
2. Use coalesced actual draft rendering with content revisions; reject stale preview/save responses. Keep annotation history separate. Invalidate affected caches/pages, not the full document per gesture.
3. Save an immutable revision to a new destination using a temporary file, an allowed-object-change manifest, validation of all untouched objects, reopening, final fingerprint checks, and non-clobbering publication.
4. Specify old-sidecar/new-copy association explicitly. Do not silently apply annotations to moved/replaced content or confuse content-save receipts with annotation acknowledgements.

**Gate:** native visual/interaction checks and real save/reopen tests, including IME, undo/redo, concurrent navigation, close while dirty, cancellation, disk/write failure, source changes, and repeated edits. Production release still requires native macOS/Windows/Linux validation.

## Corpus and verification record

Use versioned, redistributable fixtures with provenance and hashes. Put generated comparisons and reports under `target/`; never commit private documents or screenshots.

| Fixture group | Essential failure it should expose |
|---|---|
| Known Latin + mixed CJK text | Wrong source encoding, missing subset glyphs, unexpected substitution |
| Repeated text and shared resources | Editing a different occurrence or multiple drawing instances |
| Multi-stream contents and `TJ` | Lost stream origins, kerning, continuation/line-matrix drift |
| Nested/repeated Forms | Wrong resource scope or incomplete copy-on-write |
| Rotation, CropBox, UserUnit, clipping | Incorrect hit testing/transforms; unsafe unsupported cases |
| Overlap, transparency, patterns | Geometric erasure/reinsertion changing unrelated drawing |
| Ligatures, vertical text, Type 3, `/ActualText` | An unsafe capability claim instead of a precise refusal |
| Scans, outlined text, deferred document types | Fictitious editability or unintended scope expansion |
| Repeated saves and source conflicts | Stale targets, lost operations, damaged original files |

For each supported mutation, compare untouched object signatures and operator/source spans, then render before/after with independent external validators. Use exact outside-region comparisons for deterministic fixtures; document any necessary tolerance and inspect all unexplained changes. Independent validators may use PDFKit, PDFium, MuPDF, Poppler, or PDFBox outside the application; this does not authorize bundling them as a fallback engine. Include independent text extraction and annotation inspection, not screenshots alone.

Report counts by capability and corpus group: supported, explicitly rejected, failed. All declared-supported fixtures must pass; a high success percentage must not conceal a wrong-object mutation. Real-document coverage targets require a representative corpus and are not invented here.

Measure Release builds with hardware/OS/dependency versions, page/device dimensions, sample count, cold/warm conditions, p50/p95, and peak-memory scope. Separate source indexing, draft update, rendering/upload/presentation, and save/reopen costs. The historical reader/ASCII probes are not fresh measurements for this plan.

Implementation phases require `cargo fmt --all -- --check`, `cargo test --workspace --locked`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, and isolated `cargo run --locked -p breeze-app`; UI phases also require native visual inspection. Record actual outcomes in `STATUS.md` and commit each completed verified phase separately. Stop at a failed gate and report evidence/options; do not silently switch engines or weaken preservation requirements.
