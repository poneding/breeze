# Delivery Status

The accepted compact UI has progressed into a local reader with durable state, sidecar annotations, restricted standard-PDF annotation export, and opening-window policies. It is **not** a completed production PDF content editor.

## Current implementation

| Area | Delivered behavior |
|---|---|
| Library | Persistent entries, recent ordering, name filtering, favorites, user-created/renamable/deletable categories with explicit assignment, and removal without deleting source files |
| Preferences | Persistent appearance/palettes, sidebar width, independent file/position restoration, and opening-window policy |
| State integrity | Background/coalesced JSON, process locking, bounded inputs, atomic replacement, external-revision checks, and normal-close acknowledgement |
| Reading | Single/Continuous/Spread layouts; virtualized full outline/thumbnail lists; visible-range scheduling; 32 MiB worker, 64 MiB UI-page, and 8 MiB UI-thumbnail budgets; page navigation and zoom |
| Text/search | Rectangular horizontal-glyph selection/copy; cancellable background search with up to 300 results and page destinations |
| Annotations | Text/area highlight, underline, strikeout, ink, ink-only eraser, text note, ellipse, rectangle, line, free wavy line, and arrow |
| History/sidecars | Bounded undo/redo, per-page removal, source SHA-256, 500 ms debounced autosave, Save/Save As, and conflict refusal |
| PDF export | New-file-only standard annotations with vector normal appearances; old content/resources/annotations checked before publication |
| Export UX | Immutable snapshot, native save dialog, Primary+Shift+E, one in-flight job, close protection, receipt, and disclosed sticky-note semantics |
| Window policies | Default keeps geometry; Maximized maximizes once; Fit Document sizes once using remaining display space without changing zoom |
| Page-task lifetime | Foreground page tasks are owned and cancelled on replacement, Home, and close rather than left detached |

## Plans executed in this follow-up

- [05 — Annotation export](05-annotation-export.md) was written before implementation, then implemented and validated for the stated subset.
- [06 — Opening-window policies](06-window-policy.md) was written before implementation, then implemented and tested. Native QA found an origin-related sizing issue, which was corrected with a regression test.
- [07 — Remaining work](07-remaining-work.md) defines concrete renderer, layout, structural-editing, localization, platform/release, recovery, and imported-annotation packages. It is a backlog, not a completion claim.

## Export restrictions

- Export creates a new PDF and never overwrites the source, aliases, or an existing destination. It does not acknowledge a sidecar save.
- Notes export as standard sticky-note icons with the full Unicode contents in the popup, not inline FreeText boxes with guessed fonts.
- Existing PDF annotations are preserved but are not imported into editable Breeze history.
- Encrypted, signed, form/widget, unsupported page-geometry/UserUnit, excessive object-count/ID/nesting, and oversized-output cases are rejected.
- Original page streams/resources and existing annotation entries are checked, as are newly serialized annotation/appearance objects. Older supported PDF headers are raised to 1.7 for the introduced transparency/appearance features.
- This is not a sanitizer, a signature-preserving workflow, or an archival-conformance guarantee.

## Executed validation

- **64 automated tests passed**. Three fixture/performance tests are ignored by the normal suite; the export fixture generator was run explicitly for independent verification.
- Formatting, Clippy with `-D warnings`, and local macOS development packaging passed. The upstream `block 0.1.6` future-incompatibility notice remains.
- Export tests cover every stored annotation kind, exact Unicode bytes, source-content preservation, indirect/shared Annots arrays, rotation/cropping/inheritance, old headers, invalid geometry/style/page indices, signed/form/encrypted inputs, sparse IDs, aliases, and non-clobbering behavior.
- UI tests verify immutable export snapshots, in-flight close protection, failure cleanup, unchanged sidecar acknowledgement, window sizing, and unchanged page zoom.
- Poppler and macOS PDFKit rendered an eleven-kind controlled export. Both were visually inspected. Pypdf and PDFKit read back the exact Chinese/emoji note text.
- Poppler's first-page rendering with exported annotations hidden was pixel-identical to the source first-page rendering. This is a controlled one-page visual check, not a whole-corpus claim.
- A native macOS instance with isolated data exercised the export shortcut, actual save dialog, successful receipt, and a one-annotation output independently reopened afterward.
- Native inspection also confirmed that a saved ink annotation is visible after restoration, closing the prior overlay-positioning visual check for the controlled sample.

### Native window-policy caveat

The initial Fit Document check resized the window, but using full display dimensions while preserving the old origin could leave the footer off screen. The calculation now uses the remaining space at the actual origin, including secondary-display coordinates. A maximized-to-fit transition waits for a native bounds event; fullscreen mode is preserved.

The final native recheck after this correction was blocked by the locked Mac. Post-fix window-position/maximize-transition checks and Windows/Linux native acceptance remain pending. Do not mark those passed based on unit tests alone.

## Known engine risks

- Standard-font substitution/fidelity and the earlier real-paper raster p95 miss remain open.
- PDF Oxide 0.3.78's high-level annotation Contents API decodes UTF-16 strings as UTF-8. Export retains valid standard encoding, independently verified externally. Future imported-note UI needs a tested decoder/upstream correction.
- The new writer does not resolve these reader defects or validate general PDF editing.

## Performance scope

The prior reader-only Release probe on the bundled eight-page sample measured first preparation around 112.62 ms and cached-page round-trip p95 around 0.77 ms, excluding GPU presentation. These remain historical sample observations, not current release budgets.

One controlled export fixture run took about 14.72 ms in the development profile, excluding rendering and UI. This is not an export p95 or large-file memory result. Export performs explicit background parsing, hashing, serialization, and revalidation; representative costs still need measurement under the remaining-work plan.

## Remaining work

- Renderer fidelity/corpus coverage, adaptive DPI/tiles, and representative-corpus layout/scroll performance acceptance. Fixed-raster continuous/spread layouts and virtualized navigation are implemented.
- True original text/image editing, insertion into PDF content, source-object hit testing, and rich text-block formatting.
- Importing/editing pre-existing PDF annotations without duplicate appearances.
- Complete localization, OS file associations/open events, configured update sources, signing, and production-ready installers/packages.
- Complete conflict recovery/reassociation UX and native three-platform/resource/fuzz acceptance.

The remaining-work plan lists prerequisites and explicit gates; release automation does not waive them.

## SemVer release automation

The tag-push workflow validates SemVer (optional lowercase `v`) and exact workspace-version equality, checks/builds macOS arm64/x64, Windows x64, and Linux x64, and publishes archives with SHA-256 checksums only after the full matrix succeeds. Prerelease tags are marked as such; upload failures leave a resumable draft, and published releases are not overwritten. Packaging preserves the maintained notices and lockfile. See [release operations](../releases.md).

Local validation passed: eight release-tool tests (including all four archive layouts), Actionlint 1.7.12, Rust formatting/check/tests (64 passed, three ignored), Clippy, and an optimized macOS arm64 build. The real macOS ZIP was extracted and checked for its checksum, valid plist, executable permissions, architecture, and system-only dynamic-library paths. The existing upstream `block 0.1.6` future-incompatibility warning remains.

No GitHub remote is configured in this checkout, so no tag was pushed and no hosted workflow/release publication has been executed. Other-platform builds/runtime, signing/notarization, and the full dependency-license audit remain unverified or pending. Local phase commits are now required by AGENTS.md; unrelated in-progress changes remain untouched.

## September 30 reading/library refinement

[Work package 08](08-reading-layout-and-library.md) records the requested density, left alignment, user-category management, unified rounded tabs, and real reading layouts. Title/toolbar/status heights remain 34/38/32 px. Root/Kit base typography is now 13/15 px, and Small controls replace XSmall. Tab close remains hover-only with Primary+W as its keyboard alternative. Primary+Shift+L cycles layouts.

Native macOS review verified category creation and assignment, two actual side-by-side pages, thumbnail scrolling through the final sample page, continuous scrolling across page boundaries, restored layout/page state, and the synchronized page input. The first category smoke test inadvertently used the default data directory because interactive terminal entry lost the environment variable's initial character; the review-created category was removed under the state lock, without touching real documents. Subsequent review uses direct command execution with a verified isolated `BREEZE_DATA_DIR` under `target/`.

A new Release worker probe measured full-page cold p50/p95 16.92/54.19 ms and thumbnail cold p50/p95 2.61/3.09 ms on the project sample (20 observations each). Cached p95 was 1.09 ms / 0.02 ms respectively. See work package 08 for hardware, cache conditions, dimensions, and explicit timing/memory scope. These are not end-to-end or corpus acceptance results.

The user began actively interacting with the review instance during validation. Its process and newly created category were preserved. The latest scheduler-only refinements and allocation guard were checked automatically and rebuilt, but the in-use review process was not forcibly restarted; a final native recheck of those refinements and an isolated idle-CPU/frame-time measurement remain pending. Do not count the running older process as acceptance of the latest binary.
