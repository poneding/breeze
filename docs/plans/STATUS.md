# Delivery Status

## Code review remediation — October 3, 2026

[Work package 13](13-review-remediation.md) tracks the authorized remediation sequence. Phase 0 registers the missing library-container test observation: formatting, workspace tests (103 passed, three existing ignored probes), Clippy and isolated macOS startup passed. No visual behavior changed.

Phase 1 fixes transactional annotation Save As and blocks unloaded histories. Failed/cancelled writes preserve the previous association; successful writes acknowledge only their captured snapshot. Five permanent regressions pass. Formatting, the complete workspace suite (108 passed, three existing ignored probes), Clippy and isolated startup passed. Native macOS cancellation and successful Save As were exercised; the new association and exact annotation data were read back, and the source PDF hash was unchanged. Conflict recovery, rendering diagnostics and the remaining product/release gates are pending.

The accepted compact UI has progressed into a local reader with durable state, sidecar annotations, restricted standard-PDF annotation export, and opening-window policies. It is **not** a completed production PDF content editor.

Library presentation refinements are tracked item by item in [work package 10](10-library-presentation.md). Follow-up Popover, row, grid, and empty-state changes are recorded in [work package 11](11-library-interactions.md).

## Current implementation

| Area | Delivered behavior |
|---|---|
| Library | All Documents first; whole-row opening, aligned list/cover-grid views, empty new libraries, persisted multi-category memberships and eight tag colors, inline hover actions, favorites/filtering, and removal without deleting source files |
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

### Makefile command layer

The root Makefile exposes formatting/check/lint/test/verify, debug build/run, optimized release build, local packaging, and clean targets. All Cargo resolution/build/test commands use the lockfile; `make run` defaults to isolated data under `target/manual-state/`. Release/package targets never tag, push, or publish.

`make verify`, `make build`, and `make package` passed locally; the tooling suite now has 13 passing tests, including Make dry-run contracts and Windows executable naming. The then-current working-tree Rust suite passed 65 tests with three ignored. An injected failing Cargo command was propagated by Make. `make run` started the real executable with a unique data directory under `target/release-validation/`; the scratch process was then interrupted and stopped. This is startup verification, not visual or cross-platform runtime acceptance. `clean` and the mutating `fmt` command were checked by dry run only to avoid deleting local outputs or changing unrelated work.

## September 30 reading/library refinement

[Work package 08](08-reading-layout-and-library.md) records the requested density, left alignment, user-category management, unified rounded tabs, and real reading layouts. Title/toolbar/status heights remain 34/38/32 px. Root/Kit base typography is now 13/15 px, and Small controls replace XSmall. Tab close remains hover-only with Primary+W as its keyboard alternative. Primary+Shift+L cycles layouts.

Document title tabs now use one Kit Button surface with direct icon/text content, no inner text-button background or outer shadow, 8 px left padding, and no extra right padding around Close. Their Small height matches Home. Formatting, workspace tests (67 passed, three ignored), Clippy, isolated application startup, and native macOS inspection passed; component assertions cover equal height/alignment, right-edge close placement, and closing without activation.

Native macOS review verified category creation and assignment, two actual side-by-side pages, thumbnail scrolling through the final sample page, continuous scrolling across page boundaries, restored layout/page state, and the synchronized page input. The first category smoke test inadvertently used the default data directory because interactive terminal entry lost the environment variable's initial character; the review-created category was removed under the state lock, without touching real documents. Subsequent review uses direct command execution with a verified isolated `BREEZE_DATA_DIR` under `target/`.

A new Release worker probe measured full-page cold p50/p95 16.92/54.19 ms and thumbnail cold p50/p95 2.61/3.09 ms on the project sample (20 observations each). Cached p95 was 1.09 ms / 0.02 ms respectively. See work package 08 for hardware, cache conditions, dimensions, and explicit timing/memory scope. These are not end-to-end or corpus acceptance results.

The user began actively interacting with the review instance during validation. Its process and newly created category were preserved. The latest scheduler-only refinements and allocation guard were checked automatically and rebuilt, but the in-use review process was not forcibly restarted; a final native recheck of those refinements and an isolated idle-CPU/frame-time measurement remain pending. Do not count the running older process as acceptance of the latest binary.


## Status layout and zoom controls

The status bar now exposes independent one/two-page and continuous choices in a native Kit menu, separated by a divider. New documents default to continuous single-page reading. All four combinations render real pages, including virtualized continuous spreads with a lone odd final page. Fit Page / Fit Width / Actual Size share a second native menu, with an icon and current mode or percentage on the trigger.

Primary+wheel zooms over the document without scrolling it; Primary+0 / + / - use the same zoom mutations (Primary is Command on macOS and Control on Windows/Linux). Existing equals-key zoom-in remains available. Percentage zoom stays within 25–200%, and fit-to-percentage steps begin at the displayed scale. Per-document settings and the defaults for new documents persist in state version 4, independently of position restoration; explicit legacy layout/zoom values are preserved.

The full working checkout passes formatting, workspace tests (78 passed, three existing tests ignored), and Clippy with warnings denied. Component tests cover menu selection, independent layout choices, continuous spreads through an odd final page, platform-primary keyboard/wheel behavior, ordinary scrolling, JSON restoration, and new-document defaults. Native macOS startup used a process-verified isolated data directory. Visual inspection confirmed both menus, their icons, divider/checkmarks, current status labels, and preserved compact chrome; the state file contains the selected layout and percentage. The review instance remains open after user interaction. Restart behavior is covered by component restoration tests; a manual native restart and Windows/Linux runtime checks were not performed.

The isolated commit, excluding pre-existing theme and raster changes, also passes its workspace suite (69 tests, three existing tests ignored).

## September 30 PDF display quality fix

The reader now requests metadata separately and renders visible device-resolution tiles with the pinned PDF Oxide engine. It no longer enlarges a fixed 144-DPI page raster. Scale-aware caching, stale-result checks, explicit GPU atlas eviction, and 508-pixel tile interiors with two-pixel gutters preserve sharpness while bounding resident page data. The 64 MiB UI page budget is split into 8 MiB metadata and 56 MiB pixel tiles; thumbnails retain 8 MiB. A local upstream-source patch adds direct region allocation without modifying source documents.

[Work package 09](09-pixel-accurate-display.md) records native old/new comparison on the user's Chinese documents, independent PDFKit checks, Release tile p50/p95 measurements, and the GPU memory faults found and fixed during validation. The integrated checkout passes 79 tests (three existing ignored probes), formatting, Clippy, and local build/startup checks. Native large-window text is visibly sharper. Broad fidelity, frame-time, peak-memory/RSS, and Windows/Linux release gates remain open; the final observed macOS physical footprint was 216.0 MiB with a 361.6 MiB peak, not a universal memory guarantee.


## Separate zoom mode and percentage

The single-page icon is now `file`. Zoom mode is independent from its numeric status: Fit Page / Fit Width / Actual Size / Custom Ratio remain in one native menu, followed by a separate minus / calculated percentage / plus group. Fit percentages recompute with the viewport; changing the percentage switches to Custom Ratio, including custom 100%. Discrete zoom commands move to the next 25% boundary in their direction (167 → 175 or 150), and smooth trackpad input retains fine-grained zoom. State version 5 distinguishes Actual Size from custom 100% and migrates legacy 100% values to Actual Size.

The complete working checkout passes formatting, workspace tests (81 passed, three existing probes ignored), and Clippy with warnings denied. The isolated commit on top of the PDF display fix also passes formatting, 78 tests, and Clippy. Tests cover the four modes, separate control geometry, fit-scale recalculation, 167% directional grid steps, mode switching on percentage changes, custom 100% restoration, and legacy 100% migration. Native macOS inspection with isolated data confirmed the file icon, four-option menu, separate percentage, keyboard grid steps, and Fit Width at 136% switching to Custom Ratio at 150% after zoom-in; state JSON persisted the chosen mode/percentage. The review window remains available. Windows/Linux native validation remains pending.

## October 9 adaptive navigation sidebar

The Outline tab now appears only when the loaded document has outline entries. Documents without an outline show Thumbnails automatically, including through the Outline shortcut, without changing the saved navigation-view preference. Cached document outlines remain available while an uncached page loads, so page navigation does not temporarily remove the tab or its content.

Thumbnails use virtualized grid rows with compact cards capped at 144 logical pixels wide: one column at 168/208 px, two at 280 px, and three at 360 px. Cards preserve the page aspect ratio, page number, and current-page border. Incomplete final rows retain their column alignment. Visible rows map to complete page ranges; page navigation maps back to the correct row. Resizing retains the top visible page and fractional row offset, with integral row heights preventing layout-rounding drift. Scheduling covers the entire visible range through the existing serial worker and 8 MiB thumbnail cache.

Validation:

- Formatting, workspace tests (111 passed, three existing fixture/performance probes ignored), Clippy with warnings denied, and the debug build pass in the complete working checkout. The isolated phase on the committed baseline also passes formatting, 98 workspace tests (the same three probes ignored), and Clippy.
- Real component regressions cover outline availability, tab selection and shortcuts, retained outlines during page loading, automatic thumbnail preparation without an outline, 168/208/280/360 px grid geometry, resizing around page 61, page-index navigation, and a 121-page document's incomplete final row. The asynchronous worker test confirms that every visible grid column loads within the thumbnail byte budget.
- `cargo run --locked -p breeze-app` launched on macOS with process-verified isolated state under `target/sidebar-grid-review/`. Native window screenshots confirmed the conditional tabs, centered headers, two- and three-column grids, complete portrait/landscape thumbnails, page labels, and current-page borders. Narrow-column bounds and scrolling to the final thumbnail are covered by component tests; the attempted narrow native screenshot captured restoration rather than the finished reader and is not counted as visual validation.
- Orca native interaction automation was unavailable (`zsh:1: command not found: orca`); native visual inspection used system window captures. Windows/Linux native checks and the existing production fidelity/performance gates remain open. The upstream `block 0.1.6` future-incompatibility notice remains.

Generated fixtures/screenshots/state are excluded from version control. Review windows that received interaction were preserved; their isolated data remains under `target/sidebar-grid-review/`.
