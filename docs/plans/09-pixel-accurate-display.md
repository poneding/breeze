# Pixel-accurate PDF display

## Requirement and status

The user requested clear PDF display with good performance on September 30, 2026. The confirmed requirement is to avoid sacrificing display resolution to obtain responsiveness. The first investigation below was a proposal and did not change application display. After the user reported unchanged results from `make run`, the follow-up implemented the device-resolution tile path described in the implementation record below. Production fidelity and performance guarantees remain subject to the stated corpus/platform gates.

The recommended path is **device-resolution rendering, genuine viewport tiles, retained page preparation, and bounded revision-aware caches**. Increasing a fixed whole-page DPI cannot meet this requirement across large pages and high zoom. Source image resolution and engine font/transparency fidelity remain separate limits; higher raster resolution cannot restore detail absent from a scan or fix incorrect glyph rendering.

## Display contract

- Derive raster dimensions from the actual page surface in logical pixels multiplied by the current window's device scale. Observe window/display/zoom changes; do not poll. For percentage zoom under Breeze's existing convention, the effective DPI is `96 * zoom / 100 * device_scale`. Fit modes use the measured page surface instead.
- Round dimensions up and validate returned dimensions. Never accept the renderer's automatic reduction of requested resolution as a completed sharp render. Do not use JPEG, artificial sharpening, or theme filters in the page path.
- Full-page rendering is appropriate only when the exact requested raster fits the existing allocation policy. A4 at 100% on a 2x display needs about 1588 x 2246 pixels / 13.6 MiB for one BGRA buffer; at 400% it needs roughly 6350 x 8982 / 217.6 MiB. These are geometry estimates, not RSS measurements.
- Above the full-page limit, allocate and rasterize only visible tiles plus a bounded adjacent margin. Begin by evaluating 512 x 512 physical-pixel tiles: approximately 1 MiB each before the edge gutter. Tile size is an experimental tuning parameter, not an accepted constant.
- Default to full-resolution results. An old raster stretched to a new zoom must not be presented as the final result. The strict presentation policy keeps the previous viewport at its original transform while a requested zoom is prepared, or marks newly exposed areas as loading. Commit the new transform, page pixels, selection, and annotation geometry together. A temporary blurry zoom preview would require a separate UX decision.
- UI response and sharp-content readiness are separate metrics. Controls can remain responsive while missing content is prepared; that does not prove that fresh content meets its latency target.

## Avoid unnecessary work

1. Reuse loaded document data, parsed page operators, font resolution, and decoded resources where safe, with separately accounted byte budgets. Existing text/search extraction must not be repeated for every resolution or tile.
2. Prioritize missing visible content. Prefetch a bounded adjacent strip only when visible work is complete; stop prefetch under memory pressure. Thumbnails remain a separate low-priority pipeline.
3. Coalesce viewport changes once per frame. A render key includes source fingerprint/content revision, page, raster scale, rotation/crop geometry, tile coordinates, and color policy. Chrome-only theme changes do not invalidate page content.
4. Keep a bounded queue and discard results that no longer match the document and requested viewport. Add cancellation checkpoints between page preparation and tile jobs. The current synchronous whole-page call cannot be preempted safely.
5. Reuse GPU textures while scrolling within cached regions. Keep annotations in their independent vector layer so pointer movement and sidecar saves do not rerasterize PDF content.
6. Account for CPU rasters, GPU textures, page preparation/resources, in-flight work, and references retained by old frames. Enforce admission before allocation. If a working set exceeds its budget, evict offscreen entries or report the limit; do not silently lower resolution or loop on failed cache insertion.

## Engine feasibility gate

`pdf_oxide 0.3.78` provides `render_page_region`, but inspection of `src/rendering/mod.rs` shows that it renders the whole page, decodes the resulting image, crops, and encodes it again. It is unsuitable for viewport tiling. Its implementation also passes raw RGBA render bytes to an encoded-image decoder when requested with `as_raw()`; the probe below checks this incompatibility independently of the UI.

The current `PageRenderer` clears page font/color/ICC caches on each page call. Calling it once per apparent tile would repeat page setup as well as whole-page rasterization. Clipping after rendering is not a performance fix.

The required engine work is a targeted Rust region-render API that accepts an output rectangle in device pixels and renders directly into a bounded tile surface using the full-page transform translated by the tile origin. It must preserve original page coordinates, CropBox/nonzero origins, rotation, UserUnit, clipping, patterns, transparency groups, soft masks, annotations, and color conversion. Small overlapping gutters must be cropped at composition to avoid seams; validate the required gutter size rather than assuming it solves every filter case. Retain necessary prepared page data with explicit lifetime and memory accounting.

An implementation must not crop a rewritten temporary PDF, modify source objects, switch engines, or change downloaded registry sources. Use a reviewable upstream fix or a deliberately pinned source patch only after the fixture tests establish its semantics. The initial proposal did not approve an engine fork, alternate engine, or cache-budget increase. The subsequent implementation uses the narrowly scoped local patch recorded below, preserving the selected Rust engine.

Until this gate passes, adaptive whole-page rendering can improve ordinary zoom within the existing budget, but cannot fulfill the complete high-zoom requirement. Do not mark the user request resolved by only raising DPI or rejecting every larger viewport.

## Reproducible feasibility probe

`crates/breeze-pdf/examples/resolution_probe.rs` renders the first bundled sample page at fixed requested resolutions. It accepts only 144, 192, and 384 DPI and uses a diagnostic 16-million-pixel ceiling; it does not change the application's four-million-pixel limit. Each process records parser/renderer initialization, a separate first render, and 20 subsequent uncached renders of the same page. It retains one document; `page` also retains one renderer, while region modes construct renderers internally. PNG-region timing includes its required codecs, so it is not an isolated raw-raster comparison.

```sh
cargo build --release --locked -p breeze-pdf --example resolution_probe
/usr/bin/time -l target/release/examples/resolution_probe 144 page
/usr/bin/time -l target/release/examples/resolution_probe 192 page
/usr/bin/time -l target/release/examples/resolution_probe 384 page
/usr/bin/time -l target/release/examples/resolution_probe 192 region-raw
/usr/bin/time -l target/release/examples/resolution_probe 192 region-png
/usr/bin/time -l target/release/examples/resolution_probe 384 region-png
```

Run sequentially without concurrent build/test workloads. On macOS, `time -l` reports process maximum resident-set size in bytes. The embedded sample avoids disk I/O; OS font caches are uncontrolled. Results exclude GPUI, upload, presentation, text selection extraction, and the user's document. Do not equate them with disk-cold opening, application RSS, or representative-corpus acceptance. The raw-region command is expected to fail on the pinned dependency and must not be counted as a successful benchmark.

### September 30 feasibility measurements

Environment: Apple M1 Pro, MacBookPro18,3, 8 logical CPUs, 16 GiB RAM, macOS 15.7.9 (24G830), arm64, Rust 1.97.1, Release. Dependency versions are those in the repository lockfile: PDF Oxide 0.3.78 with rendering/ICC and office_oxide 0.1.9. Input: bundled `field-notes.pdf`, page 0, 612 x 792 pt, SHA-256 `38ee1558450f88101995c5610c4814df599b275abde9ef939edf5a797da5cf4e`. Runs were sequential; preliminary overlapping runs were excluded. Each row has one process-first render followed by 20 warm uncached observations. No background build/test workload ran during the recorded measurements.

| Mode | DPI | Returned pixels | Initialization / first render (ms) | Warm p50 / p95 (ms) | Process peak RSS (MiB) |
|---|---:|---|---:|---:|---:|
| Raw full page | 144 | 1224 x 1584 | 19.44 / 22.19 | 14.07 / 15.43 | 48.77 |
| Raw full page | 192 | 1632 x 2112 | 11.38 / 24.68 | 21.11 / 21.75 | 39.17 |
| Raw full page | 384 | 3264 x 4224 | 11.43 / 85.94 | 77.21 / 80.08 | 117.25 |
| PNG region | 192 | 512 x 512 | 0.09 / 44.43 | 31.98 / 33.00 | 49.06 |
| PNG region | 384 | 512 x 512 | 0.10 / 129.43 | 116.03 / 124.78 | 132.80 |

Full-page raw buffers contain 7,755,264 / 13,787,136 / 55,148,544 bytes respectively. The region's nominal decoded output is only 1,048,576 bytes, while process peak RSS still grows with whole-page resolution. Region timing includes renderer initialization and the upstream PNG encode/decode/encode cycle. Initialization outside the first render covers document parsing and, for full-page mode, renderer construction. Peak RSS includes all probe-process allocations, not just the returned buffer; allocator/font-cache behavior makes it unsuitable for extrapolation from one run. It excludes the application and GPU.

The 192-DPI raw-region attempt failed after 34.97 ms with `Invalid PDF: render output decode: The image format could not be determined`. This is an API incompatibility in the pinned dependency, not evidence of a malformed sample.

Interpretation: device-resolution full-page output is plausible for ordinary sample reading, but a small requested region currently costs more than the entire raw page. These results reject the existing region helper as a tile implementation. They do not establish native frame timing, high-zoom acceptance, fidelity on the user's PDF, or a representative-corpus p95. The 384-DPI probe deliberately exceeds the application's current output budget for diagnosis only.

The investigation passed `cargo fmt --all -- --check`, `cargo test --workspace --locked` (67 passed, three existing probes ignored), and workspace/all-target Clippy with `-D warnings`. The example was built and run in Release; unsupported DPI input was rejected. `cargo run --locked -p breeze-app` built and launched with process-verified isolated data under `target/resolution-probe/startup-state`; the scratch process was then stopped. This is startup verification only. No UI/rendering behavior changed, and no native visual quality acceptance is claimed. The existing upstream `block 0.1.6` future-incompatibility notice remains.

## Implementation and acceptance sequence

1. Establish matched-resolution reference images and measurements for embedded fonts, CJK, vector linework, transparency, rotated/cropped pages, mixed-size documents, and scans. Keep the existing fidelity blockers visible.
2. Prove direct tile rendering and prepared-page reuse with full-page-versus-assembled-tile comparisons, edge-boundary fixtures, stable memory under repeated high-zoom panning, and cancelled work. Measure decoder and transparency intermediates as well as tile buffers.
3. Connect device-resolution requests and scale-aware caches to single, continuous, and spread layouts. Cover zoom, resize, moving between displays, navigation during rendering, fingerprints changing, cache admission failures, and stale completions in component tests.
4. Measure native end-to-end content readiness, frame p50/p95, cached feedback, idle CPU, and peak RSS. Preserve the existing 16.7 ms p95 frame target, sub-50 ms cached feedback target, and ordinary-reading RSS target of 350 MiB. The historical 144-DPI raster target is not automatically proof of performance at larger device resolutions.
5. Commit implementation phases only after their required checks and native inspection pass. Report misses with input hashes, hardware, profile, dimensions, sample count, warm/cold definitions, and memory scope before seeking changes to budgets or architecture.

No PDF display implementation can promise constant latency for arbitrarily complex inputs. Acceptance must bind clarity and performance to a declared corpus and device range, while unsupported or over-budget cases remain explicit.

## Implemented follow-up: device-resolution display

The application now uses metadata-only page requests and direct device-region raster jobs. Native page canvases calculate physical resolution from their actual bounds and display scale, coalesce visible requests, and paint the resulting BGRA tiles without PNG/JPEG encoding or upscaling a fixed page raster. Tile interiors are 508 pixels with two-pixel gutters, yielding at most 512 x 512 uploaded pixels. Scale changes retire the previous resolution; scrolling reuses current-scale tiles. Stale completions and changed document fingerprints cannot enter the visible cache. Source page content is read-only.

The dependency is pinned to a local copy of the same PDF Oxide 0.3.78 distribution. Only `src/rendering/page_renderer.rs` is patched: `render_device_region` allocates the requested region and translates the original page transform into that surface, sharing the existing operator/font/annotation/color implementation. It does not rewrite CropBox or crop an already-rendered full page. [Patch provenance](../../vendor/pdf_oxide/BREEZE-PATCH.md) records the upstream revision and archive checksum. The lockfile retains the existing dependency versions. Retained operator/display-list preparation remains unimplemented; existing document/font caches are reused.

UI cache accounting is split into 8 MiB of page metadata, 56 MiB of page pixels, and 8 MiB of thumbnails. GPU entries are explicitly removed on eviction, source changes, and obsolete zoom levels. Native testing caught a material bug in the first implementation: dropping CPU image references left GPU atlas entries resident. The next measurement exposed an atlas-packing problem: 516-pixel guttered images each occupied a 1024-pixel Metal texture. Both were corrected, and a component test verifies removal of an actually uploaded atlas entry after cache clearing. The 508-pixel interior has a regression assertion preventing this packing regression.

### Validation executed

- `make build` and `make run` passed in the shared checkout; the latter was started with a fresh isolated `BREEZE_DATA_DIR`, observed running, then its scratch process was interrupted. The separate native comparison window was retained.
- Workspace formatting and all-target Clippy with `-D warnings` passed. The integrated checkout passed **79 tests**, with the three pre-existing ignored probes unchanged. An isolated copy excluding unrelated theme edits also passed 76 tests; the final integrated run covers all 79 tests. The upstream `block 0.1.6` future-incompatibility notice remains.
- Renderer regressions cover exact requested tile dimensions at multiple scales, metadata without a page raster, no stale low-resolution cache hit, CropBox offsets, all four page rotations, and transparent overlapping content. Tile assembly is compared with the same engine's full-page reference with a small numeric rasterization tolerance; this does not assert byte identity for all documents.
- Component tests cover actual canvas requests at 1x/2x display scale, zoom changes, cancellation on Home, visible-area tile bounds, panning reuse, and GPU atlas release. Existing annotation/export/gesture tests pass with the metadata-only page path.
- Native macOS inspection used isolated data under `target/quality-fix/` and separate old/new review bundles. The same Chinese book page was visibly blurred in the old 144-DPI display and sharp in the new large-window display. Native scroll/zoom and the concurrently delivered reading-layout controls were checked. There were intermittent Computer Use `noWindowsAvailable`/user-state-change errors; they are not counted as successful interactions.
- Independent PDFKit reference images (the system PDF implementation used by Skim) were compared at matching 288-DPI dimensions for two user-owned Chinese documents. Text shape, positioning, and fine strokes were inspected; existing raster illustrations retain their original detail limits. Documents and generated images remain local, outside version control. This is not broad corpus or Windows/Linux acceptance.

### Measured resolution and memory scope

Environment: Apple M1 Pro, 8 logical CPUs, 16 GiB RAM, macOS 15.7.9 (24G830), arm64, Rust 1.97.1. Release probe command: `cargo build --release --locked -p breeze-pdf --example tile_probe`, then `target/release/examples/tile_probe /path/to/document.pdf ZERO_BASED_PAGE 288 target/comparison`. Runs below were sequential after builds finished. Input bytes are read before timing; OS caches are uncontrolled. First full render includes parser/renderer initialization; subsequent tile rows are uncached raster jobs using the loaded document/renderer. GPU upload and presentation are excluded. PNG writing happens outside each tile timing. Probe RSS includes its retained full-page/comparison buffers and is not application RSS.

| Local input | Page (zero-based) | Whole-page dimensions | Tile samples | Tile p50 / p95 | All tiles | Probe peak RSS |
|---|---:|---|---:|---|---|---|
| Chinese book A | 31 | 2188 x 2732 | 30 | 5.39 / 11.68 ms | 224.98 ms | 143.91 MiB |
| Chinese book B | 35 | 1996 x 2835 | 24 | 5.80 / 13.62 ms | 240.09 ms | 92.98 MiB |

The two input documents are private, so their SHA-256 values are withheld here; the measurements above are unchanged.

Native development-build measurements used a 4608 x 2434-pixel window at 2x device scale. `vmmap -summary` reported approximately 1.2 GiB physical footprint / 1.3 GiB peak in the first leaking version. Explicit release reduced the observed footprint to 483 MiB; atlas packing reduced it further, and retiring obsolete scales yielded **216.0 MiB current / 361.6 MiB peak** in the final interactive run. These runs are diagnostic observations, not identical scripted benchmarks. Physical footprint includes compressed/private/GPU/window-surface memory and is different from RSS; the proposed 350 MiB RSS gate is not declared passed by this comparison. A 10-second settled interval recorded zero new render jobs and about 0.5% process CPU before the final obsolete-scale eviction refinement.

The final implementation improves the tested documents substantially without reducing their requested raster resolution. First-open percentiles, sustained 60/120 Hz frame timing, decoder peaks, large mixed corpora, and cross-platform GPU behavior remain release acceptance work. Do not describe the remaining gates or exact equivalence to Skim as completed.
