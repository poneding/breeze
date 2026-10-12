# Reader continuity and lookahead

## Report and intended behavior

On October 1, 2026 the user reported visible loading after each scroll, page replacement when resizing the outline panel in Fit Width, and flashes after adjusting custom zoom and toggling the panel.

The current implementation only requests the exact visible tiles, destroys prior resolution tiles before replacement, derives their scale from rounded layout bounds, and resets continuous scroll position when available width changes. This follow-up addresses these behaviors together, within the existing renderer and byte budgets.

- Prefetch one viewport around the visible area, with a bounded 16 MiB share of the existing 56 MiB tile budget. Prioritize visible pixels, nearby page metadata, nearby pixels, then thumbnails. Prefetch at page boundaries without constructing offscreen UI or changing the reading position.
- Use the semantic zoom and layout calculation for raster identity. Fit modes reserve the resolution required with the outline collapsed when estimated visible pixels fit within a 44 MiB planning ceiling (leaving headroom inside the 56 MiB cache); larger cases prepare exact native density while retaining previous pixels; custom percentages and Actual Size do not depend on outline width. Paint cached pixels into the current page geometry.
- Retain previous-resolution pixels during a real scale change. Overlay arriving native-resolution tiles and release old tiles only when their visible area is covered. Prefetch and offscreen residency yield to visible work; CPU and GPU eviction remain paired.
- Preserve the scroll anchor through layout measurement changes, including unchanged page heights in custom zoom. Do not convert an outline-width change into a jump to the current page's top.

## Validation contract

Regression checks must exercise actual page canvases and scroll handles: custom scale after changing zoom, collapsed/expanded and resized outline panels, fit-width reuse, lookahead without another scroll event, adjacent-page preparation, no navigation from prefetch, previous pixels during pending scale replacement, and bounded transition/preload residency. Run workspace formatting, tests, Clippy and isolated application startup, then inspect native scroll and sidebar interactions. Distinguish native checks from component simulation and record any tool/platform limitation.

Final steady-state pixels remain at or above the display resolution. Temporarily retained pixels during scale changes are a presentation bridge, not a new low-resolution final rendering policy. Very fast jumps into unprepared regions and engine-heavy documents can still require work; do not claim zero latency on every PDF.

## Working-tree validation, October 1

Implemented the lookahead queue, semantic/reserved raster density, retained previous pixels with coverage-based eviction, replacement ordering, and continuous scroll anchoring. No renderer or document-write path changed. Optional prefetch remains within 16 MiB of the existing 56 MiB tile cache and does not take priority over missing visible pixels. Oversampling is refused when the estimated visible-pixel cost would consume transition headroom; those large-window cases retain previous pixels while preparing native-density replacements.

Six new regression tests pass, including a real asynchronous worker test that completes current-view and adjacent-page prefetch after a single frame, without any subsequent scroll or frame call. Component tests cover 167% custom zoom after a prior zoom, sidebar widths 168/208/280/360, Fit Width reuse, continuous scroll offset preservation, and a 2304 x 1217 logical-pixel large-window transition. Cache-transition tests cover scale ratios 0.5, 0.9, 1.01, 1.1, and 2.0, preserve coverage at sampled previously visible points after every arriving tile, and remain within the existing byte budget.

Formatting and workspace/all-target Clippy pass. A workspace run excluding the independently failing `library_empty_component_is_centered_in_both_views` passes 99 tests (three existing probes ignored, one test filtered). The unfiltered run fails that library test because `home-content` has no registered observation; concurrent library changes and their staged files are left untouched. This is not an unqualified full-suite pass.

The final `make build` and isolated `cargo run --locked -p breeze-app` succeed; the scratch process/workspace were stopped after startup verification. Computer Use reports that the Mac is locked. Native scrolling/sidebar/visual checks are unperformed; unlocking was requested. No UI completion or commit is claimed until that required check is possible. Generated review data, logs, and the review bundle are under `target/continuity/`. The upstream `block 0.1.6` future-incompatibility notice remains.
