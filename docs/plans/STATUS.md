# Delivery Status

## Code review remediation — October 3, 2026

[Work package 13](13-review-remediation.md) tracks the authorized remediation sequence. Phase 0 registers the missing library-container test observation: formatting, workspace tests (103 passed, three existing ignored probes), Clippy and isolated macOS startup passed. No visual behavior changed.

Phase 1 fixes transactional annotation Save As and blocks unloaded histories. Failed/cancelled writes preserve the previous association; successful writes acknowledge only their captured snapshot. Five permanent regressions pass. Formatting, the complete workspace suite (108 passed, three existing ignored probes), Clippy and isolated startup passed. Native macOS cancellation and successful Save As were exercised; the new association and exact annotation data were read back, and the source PDF hash was unchanged.


The accepted compact UI has progressed into a local reader with durable state, sidecar annotations, restricted standard-PDF annotation export, and opening-window policies. It is **not** a completed production PDF content editor.

Library presentation refinements are tracked item by item in [work package 10](10-library-presentation.md). Follow-up Popover, row, grid, and empty-state changes are recorded in [work package 11](11-library-interactions.md).

Phase 2 implements annotation recovery and gesture ownership. Transient I/O failures can be retried, source and sidecar conflicts remain distinct, validated annotations from a stale-fingerprint sidecar are retained but hidden, and recovery copies preserve that original fingerprint. Explicit reassociation requires confirmation, checks page bounds and rechecks the source fingerprint during publication; explicit discard restores only the in-memory saved snapshot. Close attempts route users to the affected document's annotation panel. Mode and navigation changes clear active gestures. Regression tests cover these conflict, persistence, stale-dialog, close and gesture paths. Formatting, the workspace suite (116 app tests, 11 core, 19 PDF, 3 PDF integration and 7 storage tests passed; three existing PDF probes ignored), Clippy with warnings denied, and isolated app startup pass. The upstream `block 0.1.6` future-incompatibility notice remains. A native macOS capture confirmed the retained annotation, hidden stale coordinates, conflict message and recovery actions with the PDF page still rendered. Reassociation entered its confirmation flow; component tests cover confirmation/cancellation and file-picker behavior. Windows/Linux runtime checks are pending. See [work package 13](13-review-remediation.md). Rendering diagnostics and later product/release gates remain pending.

Phase 3 rendering-integrity changes are implemented and awaiting native acceptance. Audited failed image/inline-image/ImageMask, Form, mask and shading operations no longer produce successful incomplete rasters. Full-page, thumbnail and device-tile requests carry page-numbered errors; the UI exposes a bounded page error and persistent status message, and navigation to good pages recovers. Reading preflight explicitly rejects non-default or invalid leaf-page UserUnit; default units keep the original annotation-coordinate model and align with export's supported boundary. Both the working checkout and isolated candidate pass formatting, 172 workspace tests (three existing probes ignored) and all-target Clippy; isolated startup also passes. Independent Poppler checks confirm unchanged source/export backgrounds and CropBox/rotation dimensions. Native error-panel inspection is blocked by a locked macOS session (`CGSSessionScreenIsLocked=true`), so this phase remains uncommitted; startup and component tests do not replace native acceptance. See [work package 13](13-review-remediation.md) for evidence and remaining fidelity/platform gates.

## Home navigation, About, and Shortcuts — October 12, 2026

[Work package 18](18-home-navigation.md) consolidates global pages into Home.
`Screen::Settings` is gone; `HomePage` selects the library, About, Shortcuts, or
Settings page inside the Home content area. The sidebar keeps library filters and
categories scrollable and adds About (`info`), Shortcuts (`keyboard`), and Settings
(`settings`) icon buttons along its bottom edge. The titlebar Settings button and
Primary+, select the Settings page; the titlebar Home button and Primary+Shift+H
select the library page.

About shows the bundled logo, application name, installed version, update check and
localized status, author, and repository address. Shortcuts lists real bindings
resolved from `Kbd::binding_for_action` / `Kbd::global_binding_for_action` and omits
actions without a registered binding; custom shortcut editing remains unavailable.
Settings was reduced to title-plus-control rows: the appearance/data-directory/GPUI
Kit subtitles, the inline shortcut note, and the inline version line were removed.

A follow-up pass stacked the sidebar entries full-width in the order Shortcuts
(Primary+K), About (Primary+I), and Settings (Primary+,), with shortcut tooltips
resolved from the real bindings. The three global pages share a bounded, centered
column that leaves clearance before the scroll clip; About is centered on both axes
and shows `Pone Ding<poneding@gmail.com>`. Settings dropped the "Never" update
policy, and state version 8 keeps older snapshots valid by mapping the retired value
to Manual.

Validation: `cargo fmt --all -- --check`, the workspace suite (126 app tests plus
core, PDF, storage and packaging tests; three existing PDF probes ignored), and
`cargo clippy --workspace --all-targets --locked -- -D warnings` pass. Native macOS
inspection before the screen locked confirmed the library and the first-pass About,
Shortcuts, and Settings pages with sidebar selection and titlebar Settings
activation. Visual inspection of the refined stacked layout was **not** performed:
the macOS session locked at 23:49 on October 12, and captures after that point
returned a stale frame. The layout claims rest on component tests until the screen
is unlocked. The upstream `block 0.1.6` future-incompatibility notice remains.
Windows/Linux runtime checks are pending.

## Current implementation

| Area | Delivered behavior |
|---|---|
| Library | All Documents first; whole-row opening, aligned list/cover-grid views, empty new libraries, persisted multi-category memberships and eight tag colors, inline hover actions, favorites/filtering, and removal without deleting source files |
| Preferences | Persistent Follow System / GPUI Kit built-in theme selection, sidebar width, independent file/position restoration, and opening-window policy |
| State integrity | Background/coalesced JSON, process locking, bounded inputs, atomic replacement, external-revision checks, and normal-close acknowledgement |
| Reading | Single/Continuous/Spread layouts; per-document view rotation in either direction; immersive fullscreen that keeps the document only; virtualized full outline/thumbnail lists; visible-range scheduling; 32 MiB worker, 64 MiB UI-page, and 8 MiB UI-thumbnail budgets; page navigation and zoom |
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
- [07 — Remaining work](07-remaining-work.md) defines concrete renderer, layout, localization, platform/release, recovery, and imported-annotation packages. It is a backlog, not a completion claim.

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
- The annotation writer does not resolve these reader defects or establish general document interoperability.

## Performance scope

The prior reader-only Release probe on the bundled eight-page sample measured first preparation around 112.62 ms and cached-page round-trip p95 around 0.77 ms, excluding GPU presentation. These remain historical sample observations, not current release budgets.

One controlled export fixture run took about 14.72 ms in the development profile, excluding rendering and UI. This is not an export p95 or large-file memory result. Export performs explicit background parsing, hashing, serialization, and revalidation; representative costs still need measurement under the remaining-work plan.

## Remaining work

- Renderer fidelity/corpus coverage, adaptive DPI/tiles, and representative-corpus layout/scroll performance acceptance. Fixed-raster continuous/spread layouts and virtualized navigation are implemented.
- Importing/editing pre-existing PDF annotations without duplicate appearances.
- Native localization acceptance outside macOS, OS file associations/open events, signed update/install delivery and production-ready installers/packages.
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

A follow-up removes all inter-tab separators and insets an 18.5 px square Close button with independent hover/pressed colors. The requested selected-tab emphasis was subsequently withdrawn: the tab retains its original background with no added border. The tab uses the same group-hover and pressed fill across its entire surface. Workspace tests (67 passed, three ignored), formatting, Clippy, and isolated startup pass. Native interaction/visual validation remains pending: the computer-use API exposed only window controls and rejected interaction with `noWindowsAvailable`, including after rebinding a separate review bundle. This follow-up remains uncommitted until the required native check can be completed.

Native macOS review verified category creation and assignment, two actual side-by-side pages, thumbnail scrolling through the final sample page, continuous scrolling across page boundaries, restored layout/page state, and the synchronized page input. The first category smoke test inadvertently used the default data directory because interactive terminal entry lost the environment variable's initial character; the review-created category was removed under the state lock, without touching real documents. Subsequent review uses direct command execution with a verified isolated `BREEZE_DATA_DIR` under `target/`.

A new Release worker probe measured full-page cold p50/p95 16.92/54.19 ms and thumbnail cold p50/p95 2.61/3.09 ms on the project sample (20 observations each). Cached p95 was 1.09 ms / 0.02 ms respectively. See work package 08 for hardware, cache conditions, dimensions, and explicit timing/memory scope. These are not end-to-end or corpus acceptance results.

The user began actively interacting with the review instance during validation. Its process and newly created category were preserved. The latest scheduler-only refinements and allocation guard were checked automatically and rebuilt, but the in-use review process was not forcibly restarted; a final native recheck of those refinements and an isolated idle-CPU/frame-time measurement remain pending. Do not count the running older process as acceptance of the latest binary.

## Built-in theme settings refinement

The settings screen now has one native searchable Kit Select for Theme instead of segmented mode buttons and separate light/dark palette rows. The initial implementation mistakenly exposed only Kit's two automatically registered defaults. This has been corrected by embedding all 21 official gallery JSON files from the same source commit as Kit 0.7.0 and explicitly registering their 36 themes. The dropdown contains **39 choices**, grouped by System / Light / Dark: Follow System, 12 light themes, and 26 dark themes. Each appearance group starts with its default, followed by case-insensitive name order. The complete names and group membership match the user-provided list.

[Theme provenance](../../assets/themes/README.md) records the pinned upstream source and license. Original author metadata and JSON bytes are preserved; every file was verified against the upstream Git blob ID. No runtime network access, external theme directory, or theme-directory watcher is required. Kit's selected configuration supplies both component and application-chrome colors without Breeze palette overrides; compact typography/radii and 34/38/32 px chrome heights are retained. PDF colors and page jobs are unchanged.

The titlebar command and Primary+Shift+T cycle the same choices. Follow System continues to use window appearance events; named themes retain their declared mode. State version 3 persists the theme name. Versions 1–2 preserve System or migrate Light/Dark to the matching built-in theme, dropping the retired palette fields from new snapshots without changing library/annotation references.

Validation for this change:

The shared working tree carries unrelated uncommitted follow-up work, so this phase was verified in an isolated tree containing only the theme changes. That tree passes `cargo fmt --all -- --check`, `cargo test --workspace --locked` (115 passed; three existing fixture/performance tests ignored), and `cargo clippy --workspace --all-targets --locked -- -D warnings`. The upstream `block 0.1.6` future-incompatibility notice remains.
- Tests assert the complete ordered 38-theme list and appearance groups, exact color/gradient-token application for every theme, default-theme resolution, and legacy-state migration. Real component tests cover Aurora Light dropdown confirmation, Tokyo Night search/confirmation/cancellation with correct grouped indices, titlebar/shortcut synchronization, Catppuccin Mocha restoration, unchanged saved document state, and preserved chrome dimensions.
- With the complete gallery registered, `cargo run --locked -p breeze-app` built and launched successfully on macOS from the isolated tree, with a process-verified `BREEZE_DATA_DIR` under `target/theme-commit-review`. The scratch process and terminal pane were stopped after the startup check.
- Native visual/interaction inspection remains **unverified**: the configured Computer Use entry point failed with `/bin/bash: orca: command not found`. Startup and component tests are not a substitute for visual inspection or native OS appearance-event testing; Windows/Linux native checks also remain pending.


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

## October 1 reader-continuity follow-up — native verification pending

The working tree now prefetches a bounded region around the viewport and adjacent pages, retains previous pixels during scale replacement, uses stable semantic zoom identities, and preserves continuous scroll position when the outline width changes. Fit modes reuse density reserved for a collapsed outline where the budget permits; larger cases keep the previous picture during native-density replacement. See [work package 11](11-reader-continuity.md).

Six new continuity regressions pass, including asynchronous prefetch completion with no additional frame/scroll event. Formatting and Clippy pass. The workspace run excluding one independently failing library observation test passes 99 tests; it is not a full-suite pass. Native verification is blocked by the locked Mac, so this follow-up remains uncommitted and must not be marked visually accepted yet.

## Outline navigation refinement (October 3, 2026)

- Leaf entries omit the chevron; rows are 24 logical pixels high with 4 pixels of top inset.
- Exactly one resolved outline entry is active: the closest destination at or before the current page, preferring deeper entries and then the first peer on ties. Pages between headings retain the current section.
- Outline destinations currently contain page numbers only; matching the reader position to separate headings within one page remains unavailable.
- Regression coverage includes same-page parents/peers, unresolved destinations, gaps between headings, out-of-order destinations, and backward navigation. Formatting and Clippy passed; the app suite passed 60 tests but failed the unrelated `library_empty_component_is_centered_in_both_views` test (missing `home-content` observation), so this phase is not committed. An isolated native macOS run confirmed the bundled sample leaf has no chevron and has the top inset; multiple-heading visual coverage remains pending.

### Parent outline folding follow-up

- Parent chevrons now toggle their descendant rows without changing pages. All titles only navigate and never change branch folding. Unresolved titles are disabled while their parent chevrons remain usable. Leaf titles have no chevron.
- Expanded parents use a downward chevron, collapsed parents use a rightward chevron. Nested fold choices survive collapsing/reopening an ancestor and page navigation; changing the loaded source or fingerprint resets these transient choices.
- The virtual list uses visible outline indices while preserving original entry identities. When the active entry is hidden, only its visible collapsed ancestor is highlighted. Title buttons fill the remaining row width.
- Logic and real-component click regressions pass, covering nested folds, unresolved parents, same-page active ancestors, full-width title targets, and leaf navigation. Formatting and Clippy pass. The app suite passes 62 tests and still fails the pre-existing `library_empty_component_is_centered_in_both_views` observation test; other workspace crate tests pass. This follow-up remains uncommitted under the repository verification rule.
- An isolated native macOS run with a three-level synthetic outline confirmed downward chevrons, leaf omission, branch collapse, and promotion of the single highlight to the collapsed parent. Native automation intermittently returned `noWindowsAvailable` or delayed actions; reopening and the final row-width refinement are covered by component tests, with native rechecking pending.

- Title/arrow separation was subsequently verified in a native macOS run: clicking a parent title navigated from page 3 to page 1 while preserving both expanded and collapsed states; only its chevron changed child visibility. The full-width title presentation was also visually checked. The updated component regression passes; the full app suite remains at 62 passes and the same unrelated library observation failure.


## October 3 titlebar tab styling

Document tabs now use the theme accent hover color, with the same 50% opacity in dark mode as ordinary Kit ghost buttons, instead of the list-active fill. The selected tab keeps the theme background and uses `shadow_xs`, matching the settled segmented Tab shadow; no accent border is added. Hover/pressed fills remain separate from the selected background so the selected tab can also respond to hover. Existing separator removal and inset Close-button behavior are retained.

Formatting, the tab-close component test, Clippy, and the debug build pass. The initial workspace run failed in the existing `library_empty_component_is_centered_in_both_views` test because `home-content` was not registered for test lookup; unrelated staged library changes were preserved. An isolated native review opened the project sample and exposed an overly heavy first shadow, which was reduced to `shadow_xs`. The Mac then locked, blocking the final visual recheck. These changes remain uncommitted pending verification.

On October 8, the selected-tab background was corrected to apply the complete `colors.background` token directly. Kit's Custom button variant had mixed the supplied base color with transparency, so passing the theme color through that variant did not preserve its value. Direct background styling preserves the theme's color/gradient while retaining the existing hover, close-button behavior, and `shadow_xs`. Formatting, workspace tests (108 passed, three ignored), Clippy, and isolated native startup pass. Visual inspection is still pending because the configured native GUI tool is unavailable (`orca: command not found`); this follow-up remains uncommitted.

On October 9, the selected-tab shadow was increased from `shadow_xs` to `shadow_sm` at the user's request. This uses Kit's stronger two-layer shadow without changing the exact background token, hover behavior, or control geometry. Formatting, the complete workspace suite (116 passed, three existing probes ignored), Clippy, and isolated native startup pass. The native review used two project-sample copies under `target/tab-shadow-strength/`; the captured window remained on its initial frame and the macOS session reported itself locked. Final visual inspection is therefore unverified and this shadow adjustment remains uncommitted.

## October 9 adaptive navigation sidebar

The Outline tab now appears only when the loaded document has outline entries. Documents without an outline show Thumbnails automatically, including through the Outline shortcut, without changing the saved navigation-view preference. Cached document outlines remain available while an uncached page loads, so page navigation does not temporarily remove the tab or its content.

Thumbnails use virtualized grid rows with compact cards capped at 144 logical pixels wide: one column at 168/208 px, two at 280 px, and three at 360 px. Cards preserve the page aspect ratio, page number, and current-page border. Incomplete final rows retain their column alignment. Visible rows map to complete page ranges; page navigation maps back to the correct row. Resizing retains the top visible page and fractional row offset, with integral row heights preventing layout-rounding drift. Scheduling covers the entire visible range through the existing serial worker and 8 MiB thumbnail cache.

Validation:

- Formatting, workspace tests (111 passed, three existing fixture/performance probes ignored), Clippy with warnings denied, and the debug build pass in the complete working checkout. The isolated phase on the committed baseline also passes formatting, 98 workspace tests (the same three probes ignored), and Clippy.
- Real component regressions cover outline availability, tab selection and shortcuts, retained outlines during page loading, automatic thumbnail preparation without an outline, 168/208/280/360 px grid geometry, resizing around page 61, page-index navigation, and a 121-page document's incomplete final row. The asynchronous worker test confirms that every visible grid column loads within the thumbnail byte budget.
- `cargo run --locked -p breeze-app` launched on macOS with process-verified isolated state under `target/sidebar-grid-review/`. Native window screenshots confirmed the conditional tabs, centered headers, two- and three-column grids, complete portrait/landscape thumbnails, page labels, and current-page borders. Narrow-column bounds and scrolling to the final thumbnail are covered by component tests; the attempted narrow native screenshot captured restoration rather than the finished reader and is not counted as visual validation.
- Orca native interaction automation was unavailable (`zsh:1: command not found: orca`); native visual inspection used system window captures. Windows/Linux native checks and the existing production fidelity/performance gates remain open. The upstream `block 0.1.6` future-incompatibility notice remains.

Generated fixtures/screenshots/state are excluded from version control. Review windows that received interaction were preserved; their isolated data remains under `target/sidebar-grid-review/`.

## October 9 outline controls and title search

Navigation tabs are left-aligned. Outline view shows title search on the right when entries exist, plus an `ellipsis-vertical` dropdown only for a nested outline. Its Expand All / Collapse All items use `list-chevrons-up-down` / `list-chevrons-down-up` and change folding without changing pages. Flat outlines omit the entire disclosure-icon slot. The required existing branch-folding and active-section implementation is included in this phase; unrelated theme, rendering-continuity, and library changes remain outside it.

Title search uses a compact Kit Input below the 38 px header. Matching is case-insensitive and retains ancestors, including titles below previously collapsed branches. Filtered results keep their original destination identities and use temporary folding choices. Closing search or pressing Escape restores the prior folds and scroll position; clearing the query restores the original folds. Input editing keys retain their normal behavior, and missing matches have an explicit empty state. The search state belongs to the loaded document and resets with its source/fingerprint.

The navigation minimum is now 192 logical pixels, with the default still 208 and the maximum still 360. The native 168 px review exposed an edge-clipped More button despite the headless geometry check; 192 px provides space for native font metrics. Older saved widths in 168–191 px remain valid and are raised when restored; future snapshots save the clamped width.

Validation: the complete working checkout passes formatting, 116 workspace tests (three existing fixture/performance probes ignored), Clippy with warnings denied, and the debug build. The isolated outline phase passes formatting, 106 workspace tests (the same three ignored), and Clippy. Regressions exercise the real dropdown through keyboard selection, filtering and destination clicks, Unicode titles, folded matches and restoration, scroll restoration after no matches, Escape, absence of controls in Thumbnails/empty outlines, the flat-title inset, minimum-width geometry, and legacy-width restoration.

Native macOS startup used `cargo run --locked -p breeze-app` with process-verified isolated state under `target/outline-controls-review/`. System window captures confirmed the final 192 px header, left-aligned tabs, both right-side icons, the nested outline, and the flat outline without a disclosure slot or More button. Orca interaction automation remains unavailable (`zsh:1: command not found: orca`), so dropdown/search interaction was verified with real component tests, not automated native input. Windows/Linux native acceptance and the existing rendering/release gates remain open. Screenshots, fixtures, and local state are excluded from version control; the review window that received interaction was preserved.

## October 9 search-row visual consistency

Outline title search now uses the same shared row styling as document search: a fixed 34 px height, Kit horizontal padding and control gap, centered Small controls, and the theme's bottom border. The existing navigation-header border supplies the top edge. Removed the asymmetric bottom-only inset and the extra in-field clear button; the row contains the input and its separate close button. Filtering, Escape, and fold/scroll restoration behavior is unchanged.

Formatting, the complete workspace suite (116 passed, three existing probes ignored), and Clippy with warnings denied pass. The isolated phase passes 106 tests (the same three ignored) and Clippy. Native macOS inspection confirmed the expanded row at the 192 px sidebar minimum, including its bottom divider and balanced spacing. Because native control automation did not reliably open the input, an isolated preview opened the existing search UI on its first loaded frame; that temporary state setup was removed after the capture. This verifies native layout, while the existing component tests verify opening, filtering, and closing. Review data and screenshots are under `target/search-bar-review/` and excluded from version control.

## October 9 navigation-divider consistency

The Kit splitter now owns the single 1 px boundary between navigation and content; the navigation panel no longer paints an adjacent border. Its drag indicator is 4 px wide and explicitly centered over that line. The appearance override retains Kit's hover/press/drag lengths, opacity transitions, pointer hit area, and resizing behavior. The divider uses the same palette border color as the toolbar and navigation header.

Validation: the complete working checkout passes formatting, 116 workspace tests (three existing probes ignored), and Clippy with warnings denied. The isolated phase passes 106 tests (the same three ignored) and Clippy. Existing component tests exercise pointer resizing, width bounds, and collapse/restoration. Native macOS startup uses `cargo run --locked -p breeze-app` with process-verified isolated state under `target/sidebar-divider-review/`. System window captures at 2× confirm matching two-physical-pixel horizontal/vertical borders and an eight-physical-pixel hover indicator extending three physical pixels equally past each side of the divider. Orca is unavailable (`command not found`); macOS accessibility and CoreGraphics supplied the native hover interaction. Native Windows/Linux checks remain unperformed. Generated review files are excluded from version control; the review window is retained after interaction.

## October 9 document icon colors

Document tabs and library cover placeholders now share a format-specific `file-text` foreground: PDF orange-red, EPUB green, and Markdown blue. Each hue has light/dark variants and remains independent of the application's accent and the tab's active/hover state. Extension matching is case-insensitive; the bundled sample uses PDF coloring and unknown/missing extensions retain the theme's muted foreground. Original page pixels, real covers, warning icons, and tab labels/backgrounds keep their existing presentation. EPUB and Markdown remain presentation mappings only; their reading backends are unavailable. Exact colors are recorded in [the icon specification](../specs/06-icons.md#document-identity-colors).

Validation: the complete working checkout passes formatting, 116 workspace tests (three existing probes ignored), and Clippy with warnings denied. The isolated phase passes formatting, 106 workspace tests (the same three ignored), and Clippy. Existing real component tests cover tabs, close controls, library covers, and theme changes. Native macOS startup used `cargo run --locked -p breeze-app` with verified isolated state under `target/document-icon-review/`. Native screenshots confirm the colors in light/dark themes, selected/unselected PDF tabs, and hover states; manually prepared library entries exercise the planned format mappings and neutral fallback without enabling their backends. Orca was unavailable (`zsh:1: command not found: orca`), so macOS window capture and CoreGraphics supplied native inspection. Windows/Linux native checks remain unperformed. Generated fixtures, screenshots, and review state are excluded from version control.


## October 9 Settings icon

The titlebar Settings command now uses the bundled Lucide `settings` gear (`IconName::Settings`), and the icon map records that selection. Formatting, workspace tests (116 passed, three existing probes ignored), Clippy, and isolated native startup passed. A capture of the native macOS window confirmed the gear in the existing titlebar button. Review state and screenshots are under `target/settings-icon-review/` and excluded from version control.

## October 9 unavailable cover icons

Both library views now use the shared, format-colored `file-text` icon whenever a cover is pending or unavailable. Cover failures retain their explanation in the document tooltip, and successfully loaded covers remain thumbnails.

Formatting, workspace tests (116 passed, three existing probes ignored), and Clippy with warnings denied pass. Native `cargo run --locked -p breeze-app` used isolated state under `target/cover-fallback-review/`. macOS window captures confirmed the light list and dark grid with orange-red PDF, green EPUB, blue Markdown, neutral unknown-file icons, and an unchanged real sample cover. Planned-format entries were prepared only in the review state; reading support is unchanged. Native Windows/Linux checks remain unperformed. Generated fixtures, state, and captures are excluded from version control.


## October 9 leading document-tab spacing

The horizontal document-tab scroller now reserves 4 logical pixels inside its left edge so the first selected tab's shadow fits within the scroll clip. Tab-to-tab spacing, tab content padding, and titlebar height are unchanged. Formatting, the complete workspace suite (116 passed, three existing probes ignored), Clippy, and isolated native startup pass. A native macOS capture of two tabs confirms the first active tab's left shadow is fully visible. Review sample copies, state, and screenshots remain under `target/tab-leading-space/` and are excluded from version control.

## October 9 filled favorites

Library and reader Favorite buttons now use an outline bookmark for ordinary documents and a filled Lucide derivative for favorites. Favorite state no longer adds a persistent button background. Hover/press feedback, Primary+D, persistence, and independent library secondary clicks remain intact; accessible labels reflect Favorite/Remove Favorite.

Formatting, workspace tests (116 passed, three existing probes ignored), and Clippy with warnings denied pass. Native macOS inspection of the isolated `cargo run --locked -p breeze-app` window confirmed the solid reader bookmark and adjacent solid/outline library states without persistent button fills. Review data and captures are under `target/reader-tools-review/`. Windows/Linux native checks remain unperformed. The remaining sidebar/tool-settings work is tracked in [work package 14](14-reader-tools-and-details.md).

## October 10 document sidebar and tool settings

The reader toolbar now ends with Search, Favorite, and a document-sidebar toggle in every mode. The optional 256 px sidebar starts closed, with Information and Annotations tabs in a 38 px header. Information uses the loaded document's name, format, pages, current-page dimensions, categories, and source/sidecar locations. The virtualized annotation list covers the current document across pages, supports type and current-page filters, navigates to an item's page, and deletes the checked item through the existing undo/redo and sidecar-save workflow. Save, Save As, export, and error reporting remain reachable; annotation errors also appear in the reader status.

Tool configuration moved out of the permanent sidebar into interactive Kit hover cards (350 ms open / 300 ms close). Pointer travel into the card retains it; keyboard activation opens a focusable Kit popover with Escape dismissal. Hovering alone does not select a tool. Annotation styles are independent per tool for the current session. The palette shares eight fixed, saturated colors with category tags. Highlight/area tools default to 40% amber; strokes/notes default to full opacity, and the displayed percentage is the actual saved opacity. Existing annotations and original PDF page content are unchanged.

Validation: formatting, the full workspace suite (122 passed; three existing probes ignored), and Clippy with warnings denied pass. The isolated commit candidate passes 112 workspace tests (the same three ignored) and Clippy. Six new logic/component tests cover real hover-card pointer travel, eight colors and saved stroke geometry, independent tool settings, note input, keyboard activation/Escape, sidebar mode independence and narrow geometry, filter indices/order, current-page filtering, deletion/undo, navigation, and toolbar search focus. A test caught the filter menu's closing transition intercepting an immediate synthetic row click; the regression now waits for that transition before testing the row.

Isolated native macOS runs used `cargo run --locked -p breeze-app` and review state under `target/reader-tools-review/`. Light-theme captures confirm the toolbar order, annotation sidebar/filter, and interactive tool-card layout; dark-theme capture confirms the toolbar and unchanged page colors. Native activation/accessibility was intermittent (`Could not activate review window`, `target unavailable`), so dark sidebar/card interactions and the final small swatch/row sizing refinements are covered by component checks rather than a complete native recheck. Windows/Linux native checks remain unperformed. Review captures, sample copies, sidecars, and scratch builds are excluded from version control.

## Library Empty state (October 10, 2026)

The fourth item in [library interaction refinement](11-library-interactions.md) is complete. Empty libraries and filtered results use the native Kit Empty component with a small icon and “暂无文档”; table headings and the persistence footer are omitted for empty results. The committed baseline plus this change passed formatting, 116 workspace tests (three existing probes ignored), Clippy, startup, and native macOS empty-layout inspection. Other in-progress reader/settings changes remain outside this validation; Windows/Linux runtime acceptance remains pending.

## October 10 native annotation controls

Text annotations use Lucide `type` in the toolbar and annotation list. Line width uses a Kit NumberInput with decimal input, 0.25 pt steps, and a 0.25–100 pt range. Opacity uses a Kit Slider across 0–100% and displays the current percentage. Controls retain each tool's independent values and reject stale events after changing tools; invalid line-width input does not change the applied style.

Formatting, workspace tests (123 passed, three existing probes ignored), and Clippy with warnings denied pass. Component checks cover typed/stepped widths, slider pointer updates, persisted gesture values, invalid numeric values, and stale tool events. An isolated native macOS run confirmed the Type icon, decimal NumberInput, and continuous opacity Slider in the tool hover card. Screenshots and state are excluded under `target/annotation-edit-review/`; the review window that received user interaction is preserved. Windows/Linux native checks remain unperformed. Text placement and page selection are implemented in the second phase of [work package 15](15-direct-annotation-editing.md).

## October 10 direct sidecar annotation editing

Text annotations now start at the clicked page position with an empty native input. Enter commits one sidecar item and Escape cancels; IME marked composition never commits partial text. Existing sidecar items can be selected on the page, moved within page bounds, edited, styled, or deleted. Selection controls use the annotation color, width, and opacity; text overlays remain page-local and preserve their foreground color and opacity. Numeric, slider, text, and completed-move edits each form one undoable history transaction. Source identity, sidecar fingerprint, transient history revision, item index, and expected value reject stale edits. Existing PDF content remains read-only, and sidecar saves preserve the source PDF bytes.

Direct editing tests cover text creation, Enter/Escape behavior, IME composition, text revision, deletion, movement, style transactions, stale selections, shape hit testing, CropBox/rotation transforms, spread/page-local placement, invalid source/fingerprint handling, and sidecar save integrity. The isolated locked candidate passes `cargo fmt --all -- --check`, workspace tests (126 passed, three existing PDF probes ignored), `cargo clippy --workspace --all-targets --locked -- -D warnings`, and `cargo build --locked -p breeze-app`. Native captures under `target/annotation-edit-review/` show the inline text editor, selected annotation controls, Type icon, NumberInput, and Slider. Native automation could not reliably activate the final input window; real component tests cover the complete interaction. Windows/Linux native checks and the existing production fidelity/release gates remain open.

## October 11 right document sidebar layout

The document sidebar now sits beside the complete reader content below the title bar, matching the left navigation panel's vertical placement. Its header aligns with the 38 px reader toolbar. Information and Annotations are separate icon buttons; the reader toolbar's sidebar button remains selected while the panel is open, and the active content button is selected when search is not taking over the panel.

Document text search now opens the right sidebar automatically. Its input, confirm button, and close button occupy the right sidebar toolbar in place of Information and Annotations; results remain below that toolbar. The left Outline/Thumbnails navigation remains unchanged while search is active. Opening search records whether the right sidebar was already open: closing with X, Search, or Escape restores the prior open/closed state. The sidebar toggle cancels active search before closing the panel.

Formatting, the complete workspace suite (96 passed, three existing PDF probes ignored), and Clippy with warnings denied pass. Focused component tests cover uniform icon spacing, top alignment, narrow-window bounds, icon-button switching, toolbar search controls, automatic right-sidebar search, search-result placement, Escape dismissal, and restoration from both sidebar states. Native macOS capture under `target/right-sidebar-review/` confirms the aligned right panel and evenly spaced Information/Annotations buttons; search-toolbar interaction remains covered by component tests. Windows/Linux native checks and the existing production fidelity/release gates remain open.
## October 11 interface languages

The language preview is now a native Kit Select offering English, Simplified Chinese,
and Traditional Chinese. Application-owned chrome, menus, tooltips, input placeholders,
annotation tools and new notices use a compiled catalog. Theme groups and existing
inputs refresh immediately; filenames, user category names, document content and
official theme names are preserved. The three choices persist using the existing
numeric preference mapping (0/1 retained, 2 added for Traditional Chinese). Kit's own
locale follows the selection. Platform dialogs and underlying library/OS diagnostic
details can retain their platform or dependency language; previously emitted notices
retain the language in which they were produced.

Formatting, workspace tests (143 passed, three existing PDF probes ignored), all-target
Clippy with warnings denied, build and isolated `cargo run --locked -p breeze-app`
startup pass. The isolated commit candidate passes 131 workspace tests (the same
three probes ignored), formatting and Clippy. New tests cover catalog/parameter completeness, actual dropdown keyboard
selection, live chrome/input refresh, user-content preservation and persisted restore.
Native macOS interaction with the isolated review bundle verified all three dropdown
choices and complete settings layouts in English and both Chinese variants. The
review data directory is `target/language-update-review/native/`. Windows/Linux native
localization checks remain unperformed. Release checks are the second phase of
[work package 17](17-language-and-updates.md).

## October 11 stable-release checks

Settings now checks the latest stable GitHub Release for `poneding/breeze`, the future
repository supplied by the user. Manual remains the default. Automatic checks once
after startup restoration or when first enabled in the session; Never prevents
background checks. A manual check remains available in every mode. Policy persists;
results are session-only. A newer version shows a dismissible notification outside
Settings and a button to open its release page. No download, installation, remote
creation or publication occurs.

The background Rust HTTP client enforces HTTPS, a 15-second total deadline (including
body reads), a 1 MiB metadata limit, no redirects, and one in-flight request. SemVer
comparison handles numeric ordering, optional v/V tags, prereleases and build metadata.
Only a validated stable tag can produce a link, constructed under the fixed GitHub
repository. Missing releases/repositories, invalid metadata, network errors, rate
limits and HTTP failures cannot produce an up-to-date result. An automatic result
is discarded if background checks were disabled while its request was running.

Validation: formatting, 148 workspace tests (three existing PDF probes ignored),
all-target Clippy with warnings denied, and the debug build pass. Five new tests
exercise version ordering and link validation, real loopback HTTP headers/statuses,
response-size and body-read timeouts, automatic/manual/never policy and in-flight
bounds, and real Kit loading/result/banner interactions across languages. Native macOS startup used `cargo run --locked -p breeze-app` with isolated state.
The review bundle restored Traditional Chinese across restart, performed a real
HTTPS check, showed GitHub's actual rate-limit failure (HTTP 403 with remaining
quota zero), re-enabled Check Now, and relocalized that result when switched to
English. Both update layouts were inspected without clipping. The English reader was also
inspected with an isolated bundled-sample copy, including navigation, toolbar and
status labels. The future repository
has no user-published release yet; successful/newer-release and missing-release
responses are verified with local HTTP fixtures, not a claimed live publication.
Windows/Linux native checks remain unperformed.

The isolated release-check commit candidate passes 136 workspace tests (the same
three existing probes ignored), formatting and all-target Clippy. The control test
uses an actual simulated 900×650 viewport and verifies the check button remains
inside it. Only the four HTTP-client support packages and the required Rustls log
feature were added to the lockfile; unrelated locked resolutions were retained.

## October 12 left navigation button state

The left navigation toolbar button now derives its selected appearance from the
existing sidebar open state, matching the right document-sidebar toggle. Clicking
the toggle, using its shortcut, opening Outline/Thumbnails, and restoring saved
sidebar state all use that same state.

Formatting, the full working-checkout workspace suite (142 passed; three existing
PDF probes ignored), and all-target Clippy with warnings denied pass. Existing
component tests cover shortcut toggling and collapse/width restoration. An isolated
`cargo run --locked -p breeze-app` launched on macOS with state under
`target/navigation-toggle-review/native/`. The initial native visual check was blocked
by a locked Mac. A subsequent isolated review under `target/search-close-review/native/`
confirmed the selected appearance while expanded, its removal after clicking to
collapse, and restoration after Command+B reopened navigation. Windows/Linux native
checks remain unperformed.

## October 12 simplified document search toolbar

The right-sidebar search toolbar now contains only the expanding search input and
its Close icon button. The separate Search confirmation button is removed; Enter
continues to submit the query through the existing input event handler.

Formatting, the full working-checkout workspace suite (142 passed; three existing
PDF probes ignored), and all-target Clippy with warnings denied pass. Isolated
`cargo run --locked -p breeze-app` startup and native macOS review confirmed the
input filling the toolbar beside Close, Enter returning document matches, and
Close restoring the previously closed right sidebar. Review state is under
`target/search-close-review/native/`; the review window is retained after user
interaction. Windows/Linux native checks remain unperformed.


## October 12 titlebar tab compression and hover Close

Titlebar content and its document scroller now have zero flex bases so Kit's enclosing bar cannot request the combined intrinsic width of every tab. Home and the grouped Open/theme/Settings controls retain their widths, and Kit's Windows/Linux window-control sibling keeps its own space. Document tabs share the remaining width equally up to 228 px, compress to 40 px, then scroll only within their bounded region. The 4 px leading shadow inset is retained.

Close is now an absolute 18.5 px square overlay with no reserved text slot. On hover, a 12 px transparent-to-surface gradient precedes a solid area around Close, hiding text without changing its layout. The fade uses the same composited hover color as the tab, including dark-theme opacity. Independent Close feedback, click propagation, keyboard closing, the exact theme active background, and the stronger active shadow are retained. This phase includes the previously pending title-tab surface and Close refinements as their required base.

The complete working checkout passes formatting, 143 workspace tests (three existing PDF probes ignored), and Clippy with warnings denied. The isolated titlebar phase on the committed dependency versions also passes formatting, 137 workspace tests (the same three ignored), and Clippy. Component regressions cover 3/16/60 tabs at 298–1000 px, equal compression, bounded overflow, reachable global buttons, and stable full-width titles overlaid by Close. Intermediate widths model the 102 px consumed by all three Kit window buttons; this is geometry coverage, not native Windows/Linux validation. Isolated macOS startup and captures of 16 long titles confirmed compression at 1340 px and the 900 px native minimum, separate global controls, light/dark hover fades, active-tab fading, and independent Close hover. Native Windows/Linux checks remain unperformed. Orca is unavailable; native inspection used the existing macOS window-capture and CoreGraphics review helper. Review copies/state/screenshots are under `target/titlebar-tab-density/` and excluded from version control.


## October 12 add-document control after tabs

A 1 × 14 px theme-border divider and a Small Lucide Plus button now follow the document-tab scroller. Plus stays visible outside the scroll clip and opens the existing document picker through the Open action and its shortcut tooltip. Its idle foreground is the theme's muted foreground; Kit's ordinary ghost-button hover restores the normal icon foreground and background. The former Open button is removed from the right-side group, which now contains only theme and Settings. The tab region is capped at its preferred total width so Plus follows the last tab even when only one document is open; compression and bounded overflow are retained. The control remains available with no open tabs.

Formatting, the working-checkout suite at verification time (143 passed, three existing PDF probes ignored), and all-target Clippy with warnings denied passed. Subsequent unrelated in-progress annotation changes temporarily prevented a new working-checkout startup, so the isolated phase on committed dependencies was also checked: 137 workspace tests (the same three ignored) and Clippy pass. Real-component coverage checks one, three, 16, and 60 tabs plus an empty strip, divider/Plus position, action-group bounds, removal of the old Open target, and Plus opening the native path prompt. Isolated native macOS startup and captures confirmed Plus following a single tab, staying visible after 16 compressed tabs, muted idle versus normal hover styling, and no Open icon in the right-side group. Native Windows/Linux checks remain unperformed. Review files are under `target/titlebar-add-document/` and excluded from version control.


## October 12 view rotation and immersive fullscreen

Rotate Left and Rotate Right sit immediately left of the layout menu in the status bar, and Fullscreen sits at its right edge. Rotation is view-only and per document: each click advances one clockwise or counterclockwise quarter turn, the result persists with the document, and the source PDF is never rewritten. A rotated page keeps its own `/Rotate`; the reader renders raster tiles and navigation thumbnails in the rotated view space, so a quarter turn exchanges the displayed width and height. Annotation geometry, text selection, and search destinations continue to use unrotated PDF user space and are mapped through the combined rotation, so existing highlights, notes, and gestures stay aligned.

Fullscreen requests native window fullscreen and renders only the document: the title bar, navigation panel, document sidebar, reader toolbar, and status bar are all removed and the page area uses the entire window, refitting fit modes to the larger area. F11 toggles the mode in both directions; Escape leaves it unless a text annotation is being edited. Per-document layout, zoom, and rotation are preserved, so a turned document stays turned in fullscreen.

`breeze-core` owns the `Rotation` value, the `ViewGeometry` mapping, and the per-document `ReaderState.rotation` field; `breeze-pdf` maps a view tile onto its axis-aligned source region and rotates rendered pixels off the UI thread, with rotation included in the tile identity and the worker cache key. Persistence is state version 7; versions 1–6 load unrotated because the field defaults. New coverage: rotation composition and point mapping, tile-region bounds and out-of-page rejection, pixel rotation of full pages and tiles, 90° tile reassembly against an independently rotated page, and component tests for the status controls, the width/height exchange, per-document restoration, immersive chrome removal, and both exit keys.

Verification status: an isolated tree containing the committed dependencies plus this feature passed formatting, the full workspace suite (119 app, 14 core, 23 PDF, 4 PDF integration, and 7 storage tests; three existing PDF probes ignored) and all-target Clippy with warnings denied. This tree deliberately excluded two uncommitted neighboring phases in the shared working directory, so the working-checkout run and native macOS visual inspection of the immersive mode are still pending; do not treat this phase as visually accepted yet.


## October 12 content-sized tabs and continuous hover shadows

Document-tab preferred widths now use Kit's Small text size and the active UI font to shape each filename, plus the actual icon/gap/padding widths. Short Latin and Unicode names remain compact rather than stretching to 228 px. Longer titles keep that maximum; tabs shrink from their preferred widths down to 40 px and retain bounded overflow. The Close overlay still reserves no layout space. The scroller cap is the sum of those preferred widths, keeping the divider and Plus beside the last tab.

Inactive tabs now use the same `shadow_sm` as active tabs while their hover group is active, including over Close. The tab scroller retains its 4 px leading inset and gains a 6 px trailing inset so the final active/hover shadow stays within the clip and clear of the trailing divider.

The isolated phase on the current committed base passes formatting, 168 workspace tests (three existing PDF probes ignored), and all-target Clippy with warnings denied. Regressions cover short Latin/Chinese names beside a maximum-width title, shrinking in a narrow viewport, stable hover/Close geometry, the divider's shadow clearance, 1/3/16/60 tabs, and reachable titlebar buttons. A native macOS run with isolated state confirmed short versus long widths, the last active tab's uncut shadow, inactive hover and Close-hover shadows, and matching dark-theme behavior. Review files remain under `target/titlebar-tab-intrinsic/` and are excluded from version control. The main working checkout's additional in-progress rendering tests prevented a complete combined check and were preserved; they are outside this commit. Windows/Linux native checks remain unperformed.


## October 12 balanced document-tab padding

Document tabs now use equal 0.5 rem side padding (7.5 logical pixels at the current 15 px rem), including when a short filename sets the preferred width. Width measurement includes both sides; the 40–228 px bounds, hover shadows, divider clearance, and Close fade remain in place. Close is inset by that same padding without reserving a layout slot.

Formatting, the working-checkout workspace suite (173 passed, three existing PDF probes ignored), and all-target Clippy with warnings denied pass. Existing component regressions now check equal content insets on short Latin/Chinese and maximum-width titles, plus Close alignment. Isolated-data `cargo run --locked -p breeze-app` startup and a native macOS capture confirmed the balanced short-title spacing. Review data/screenshots are under `target/tab-balanced-padding/` and excluded from version control; native Windows/Linux checks remain unperformed.


## October 12 independent title and Close insets

The title retains a 0.5 rem right inset matching the left icon inset, while the hover Close overlay again sits 2 logical pixels from the tab's right edge. The title's extra padding is applied inside its row so the existing content clip still contains Close. Preferred tab widths, the gradient fade, and hover shadows are unchanged.

Formatting, the workspace suite (173 passed, three existing PDF probes ignored), and all-target Clippy with warnings denied pass. Component regressions check the title inset separately from Close's 2 px alignment. Isolated-data native macOS startup and captures confirmed both the normal short-title spacing and the restored Close-hover position. Review files remain under `target/tab-close-inset/` and are excluded from version control; native Windows/Linux checks remain unperformed.
