# Detailed Remaining Backlog and Gates

This document turns the remaining product targets into implementation-sized work packages. Items below are not implemented merely because this plan exists. Execute ready work in dependency order and update `STATUS.md` with results.

## R1: Renderer fidelity and representative measurements

**Files:** `crates/breeze-pdf/src/lib.rs` (split loading/rendering/cache as needed), focused tests under `crates/breeze-pdf/tests/`, and fixture documentation under `assets/` only for redistributable inputs.

1. Reproduce the standard `Helvetica-Bold` substitution issue with a small versioned PDF; inspect selected font family/weight in the current renderer.
2. Exercise embedded/subset/CID/Type 3 fonts, CJK text, transparency, CropBox/Rotate/UserUnit, image-heavy scans, and mixed page sizes.
3. Record exact dependency graph, fixture hashes, Release timings, output size, first/warm conditions, RSS, and independent-renderer differences.
4. Propose a targeted upstream/API fix only after the defect is isolated. Do not modify registry sources, invent a fallback engine, or broaden unsupported geometry silently.

**Gate:** review any unresolved fidelity error, 100 ms ordinary-page raster p95 miss, or working-set overrun before production acceptance. The existing synthetic cache benchmark is not a substitute.

## R2: Adaptive rendering and reading layouts

**Dependency:** R1's reproducible geometry/quality baselines.

**Files:** split `breeze-pdf` worker requests/results; add a core viewport/layout model; update `reader.rs` and new viewport-specific UI tests.

1. Request page raster dimensions from actual viewport size/DPR with a bounded zoom bucket and revision-aware cache key.
2. Present the visible page before optional thumbnails; deprioritize/cancel obsolete work without accumulating a queue.
3. **Delivered in work package 08 (fixed raster):** implement visible-range page layout for continuous and two-page modes. Render only visible/nearby pages, keep per-page geometry, and preserve a stable scroll anchor during resize/zoom.
4. **Delivered in work package 08:** replace the 300-entry outline cap and four-thumbnail window with measured virtualized navigation, not eager widgets/textures for all pages.

**Acceptance:** mixed-size 1,000-page fixture, bounded CPU/GPU caches, stale-generation rejection, zoom/anchor tests, no every-page texture allocation, and native interaction/frame measurements.

## R3: Localization and platform opening

**Files:** introduce a small typed UI-string/catalog module in `breeze-app`, extend persistence for language selection, and add minimal platform integration only when used.

1. Inventory all visible strings, tooltips, errors, and dialogs. Route them through a Chinese/English catalog with format arguments and runtime switch tests.
2. Keep user document titles, note contents, and paths unchanged. Test long English labels, CJK/IME input, keyboard access, and native dialog text.
3. **Delivered in [work package 23](23-file-associations.md):** native macOS
   open-url events, command-line documents, and deliberate PDF viewer
   registration in macOS, Windows, and Linux packaging. Settings states that
   the default handler stays a system choice instead of showing a fake boolean.
4. Cold-open and already-running open are covered by tests and verified natively
   on macOS. Multiple files, missing files, and sandbox/permission errors, plus
   Windows and Linux registration, still need their own hosts.

**Gate:** no complete-localization or association claim before all native paths pass.

## R4: Update checking and release artifacts

**External decision required:** a real release repository/endpoint, signing policy/keys, distribution channels, supported CPU/OS matrix, and versioning policy. Do not invent these values or publish automatically.

1. Add a read-only update metadata check using a configured HTTPS source, timeouts, and explicit error states; honor manual/automatic/never settings.
2. Separate checking from download/install. Verify artifact authenticity before any installation workflow.
3. Add platform package resources, license inventory, macOS signing/notarization, Windows signing/installer, and Linux packaging according to the chosen channels.
4. Run the full native GPU/window/IME/accessibility suite and representative resource/fuzz tests before release.

**Gate:** no placeholder network requests, unsigned silent updater, fictitious "up to date" result, or publication without user authorization.

## R5: Persistence/recovery acceptance

**Files:** `breeze-storage`, `persistence.rs`, `annotations.rs`, and existing integration tests.

1. Add bounded, explicit reload/reassociate/retry flows for externally changed state and sidecars without losing unsaved edits.
2. Cover interrupted writes, failed directory sync, path aliases, missing/unwritable destinations, multiple writers, application shutdown, and moved documents.
3. Preserve annotation text/geometry and source identity; an export receipt must not acknowledge a sidecar save.
4. Add schema changes only alongside concrete migration requirements. Do not proactively build a generic migration framework.

**Acceptance:** each failure leaves the original document and last valid state intact, has an actionable UI message, and does not silently clear history or switch storage locations.

## R6: Editing pre-existing PDF annotations

**Dependency:** verified PDF text-string decoding and a display path that can isolate imported annotation appearances.

1. Import supported standard annotation types with stable source-object identities; keep unsupported appearances explicitly read-only.
2. Track edits/deletions as annotation-object changes, not page-content erasure or duplicate overlays.
3. Avoid showing both the original appearance and an edited copy. If the renderer cannot isolate the imported appearance, keep editing disabled rather than cover it up.
4. Preserve Unicode Contents, author metadata, replies/popups, shared Annots arrays, and unrelated annotation objects on export.

**Acceptance:** import/edit/export/reopen fixtures in independent viewers; no duplicated appearance, missing note contents, or removal of unrelated annotations. The currently implemented exporter preserves pre-existing annotations but does not import them into editable Breeze history.
