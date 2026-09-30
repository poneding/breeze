# 08 — Reading layouts and library refinement

Date: September 30, 2026. Status: implemented; automated checks and initial macOS visual review complete. Final scheduler refinements have automated coverage; native rechecking is deferred while the user is actively using the review window.

## Confirmed scope

- Increase chrome typography and control size slightly without changing the 34 px title bar, 38 px toolbar/navigation header, or 32 px status bar.
- Left-align library and outline labels, including the Button's internal label container.
- Replace seeded categories with user-created, named categories: create, rename, delete, assign, and filter. Deleting a category only unassigns documents.
- Make every thumbnail reachable in a scrolling, virtualized navigation list.
- Integrate each document tab into one rounded, clickable surface. Reserve space for an inset hover-only close button; elevate the active tab and separate inactive tabs.
- Implement independent per-document Single, Continuous, and Spread layouts. Spread groups pages 1–2, 3–4, etc.; an odd final page remains alone. Navigation advances by a spread in Spread mode.

## Execution

1. Adjust control sizes and internal alignment, preserving fixed bar dimensions. Refine title tabs and retain keyboard close/tab switching.
2. Add persistent stable category IDs and explicit category management. Migrate only the existing version-1 numeric assignments; fresh libraries contain no named categories.
3. Separate thumbnail requests from full-page rendering. Cache document dimensions once per open document. Keep one running and at most one pending worker request; the UI schedules visible pages before thumbnails and rejects stale document results.
4. Virtualize continuous pages and thumbnails using the pinned GPUI Kit list. Bound resident page pixels to 64 MiB and thumbnails to 8 MiB, with count bounds as well. Retain the existing 32 MiB worker result budget. No eager whole-document rasterization.
5. Persist layout per document; support jump, search result navigation, fit width/page, zoom, and spread navigation. Bind annotation gestures to the actual page surface and its geometry, not a shared current-page assumption.
6. Add category/migration, layout, scheduler, component, geometry, and fixed-height regression tests. Run fmt, locked workspace tests, Clippy, and a native visual review when the desktop is available.

## Acceptance and limits

- Thumbnail scrolling reaches the final page; continuous scrolling changes the page indicator; layout switches preserve the current document/page.
- No blank-page success on render errors. Visible pages show loading/error state independently.
- Source PDF editing remains gated by structural-editing fidelity requirements; this request does not authorize an overlay-based editor or engine fallback.
- Existing renderer fidelity and latency risks remain open. This increment improves scheduling, not font fidelity, adaptive raster resolution, or three-platform runtime acceptance. Measure any new performance failure and report before relaxing targets.

## Validation record

- Automated coverage includes category create/rename/delete and narrow legacy transition; layout persistence and spread boundaries; byte/count cache protection; separate last-page thumbnail requests; 120-page virtual-list reachability; correct second-page annotation gestures; native-component hover-only tab close; fixed bar heights and left-label content bounds; coalesced/stale visible ranges.
- The list scheduler accepts ranges from prepaint, not measurement-only layout. Unchanged ranges do not enqueue new effects. This avoids measurement-driven page flips and redundant idle scheduling.
- Native macOS review has exercised user category creation/assignment, Single/Spread/Continuous display, thumbnail scrolling to page 8, and continuous page-indicator changes. Final build checks and the complete verification summary are recorded in STATUS.

### Controlled worker performance, September 30, 2026

Command: `cargo test -p breeze-pdf --release --locked report_visible_request_costs -- --ignored --nocapture`.

Apple M1 Pro, 16 GiB RAM, macOS 15.7.9 (24G830), Rust 1.97.1, optimized Release profile. Project-authored eight-page embedded sample; 20 new-worker observations per request kind, each immediately followed by the same cached request. Pages cycle through the eight-page sample. “Cold” means a fresh worker/document/engine; OS/font caches were not cleared, and embedded input does not measure disk I/O. Other development work was running, so these are controlled sample observations, not an isolated lab result.

| Request | Raster | Cold p50 / p95 | Cached p50 / p95 | Last accounted result |
|---|---|---|---|---|
| Full page, glyphs and document metadata | 1224 × 1584 BGRA, 144 DPI | 16.92 / 54.19 ms | 0.72 / 1.09 ms | 7,767,474 bytes |
| Thumbnail, no glyph extraction | 204 × 264 BGRA, 24 DPI | 2.61 / 3.09 ms | 0.01 / 0.02 ms | 215,424 bytes |

Times measure worker request/response preparation, not GPU upload, frame presentation, or end-to-end application latency. Bytes account for returned raster/glyph/outline data, not process RSS, engine allocations, transient clones, or GPU residency. Existing real-document font fidelity and tail-latency risks remain open; no budget or engine change is approved by this measurement.
