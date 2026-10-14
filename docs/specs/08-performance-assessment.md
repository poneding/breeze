# Reader performance assessment: lessons from SumatraPDF

Date: October 14, 2026. Status: research and local measurements. The subsequent engine decision and migration status are tracked in [work package 22](../plans/22-mupdf-migration.md).

## Recommendation

Start with **prefetch admission and scheduling fairness**, then measure and reduce document/resource ownership costs. Pursue **true region rendering in the published Rust engine** as the structural remedy for memory growth at large display sizes and zoom levels. Retained page preparation is a separate improvement for repeated rendering.

SumatraPDF provides useful examples of doing less work, retaining useful results, and abandoning obsolete work. The user subsequently selected MuPDF through `mupdf-rs`; this assessment is conceptual guidance and does not authorize copying SumatraPDF's GPL implementation.

## Scope and evidence

- Breeze source baseline: `84dd42c0d80f02e54bc4bbd13cf7a19c6d8ac2e5`, clean at the start of research.
- PDF Oxide: crates.io `0.3.78`, upstream commit `ad49c4cb3638dc950a29350ef882161372fde473`. The archive SHA-256 matches `Cargo.lock`: `95b1ecc1a60a314fbd1aea5fe8f700ec32af1efa70fef5c6879162444da3437d`. The inspected document, renderer, and rendering-module sources match that archive byte for byte.
- SumatraPDF release reference: `3.6.1rel`, commit `16c59fde8b824ab54c56f23aef910a6fdd874ad0`.
- SumatraPDF development reference: `master` at `bf941f965c6d94f0af4f56792099373337d93c65`. Development-only behavior is identified below; it must not be attributed to the user's unspecified installed version.
- User observation: a roughly 18 MB, 343-page PDF, read in a maximized window, uses approximately 350 MB in Breeze versus 120 MB in SumatraPDF; page turns visibly wait for content. The two executable versions, OS/memory-counter equivalence, and exact zoom/DPI were not established.
- Local reference input: the user's 343-page book, 18,648,370 bytes (17.78 MiB), SHA-256 `00660ce85cecc47817ab84fa4cae241e73d94f77ace38376e0fd459acb335f4e`. It was read locally without uploading or changing it. It is not a redistributable repository fixture.

Current code overrides older descriptions of the region renderer in `02-architecture.md` and the October 13 page-turn entry in `STATUS.md`. [Work package 20](../plans/20-upstreaming-engine-changes.md) records its retirement: the application now renders whole pages. The original 350 MiB ordinary-reading target is a proposed gate, not an established current guarantee.

## What SumatraPDF actually does

| Mechanism | Verified source behavior | What Breeze should borrow |
|---|---|---|
| Render spatial regions | Release `RenderCache::GetTileRes`, `Paint`, and `EngineMupdf::RenderPage` choose tiles, skip tiles outside the paint area, and allocate a pixmap for the requested bounding box. Tile size varies with screen, scale, and clipping support. | Allocate pixels for the visible region at native density. Avoid a full-size page bitmap followed by cropping. Use regions large enough to amortize page preparation. |
| Bound and refresh the work queue | Release queue is limited to eight requests; matching requests are deduplicated/promoted, stale resolution/visibility requests are removed, and the newest queued work is taken first. `AbortCookie` propagates cancellation into the renderer. | Keep latest visible demand ahead of speculative work. Cancellation must stop expensive work where supported, not merely discard its result. Keep fairness for visible thumbnails. |
| Reuse displayed pixels during replacement | Release painting can use a bitmap at another resolution while the requested one is being prepared. Visibility and replacement guide eviction. | Preserve the existing Breeze transition behavior, but protect only the necessary replacement image and account for its memory. |
| Predict a small amount of useful work | Development source chains at most four predictive page requests, one after another; the chain stops when its origin leaves view and skips high tile subdivisions. | Predict within an admitted byte/time budget, stop on direction/demand changes, and never let missing speculative work prevent visible work from completing. The number four is not a suggested Breeze setting. |
| Keep thumbnail work local to the view | Development `PageThumbnailsCtrl::StartRendering` queues visible pages before nearby pages, renders within a surrounding screen range, evicts outside a wider range, and runs one background thumbnail batch. | Prioritize on-screen thumbnail feedback, bound a small lookahead, and cancel obsolete batches. Its optional cloned engine is a tradeoff, not a default recommendation for Breeze. |
| Reuse library previews across launches | Release `FileThumbnails.cpp` loads/saves PNG previews in an application cache and checks source modification time. This is the file/Home thumbnail path, not evidence of persistent thumbnails for every page. | Add bounded disk caching for Home covers first; consider recently visited page thumbnails separately. Use Breeze's revision/fingerprint rules rather than copying a path-only cache identity. |
| Treat large source files differently | Release `FzOpenOrReadFile` reads files smaller than 32 MiB into memory and keeps larger normal files open for on-demand access. | Avoid mandatory complete-file duplication; ultimately support a file-backed reader. The user's 17.78 MiB file is below Sumatra's threshold, so file streaming alone does not explain this particular comparison. |
| Reuse prepared page drawing | Development `GetOrBuildPageDisplayList` caches a View display list; `RenderPage` replays it for the requested rectangle. The inspected release View path directly calls `pdf_run_page_with_usage` instead. | Evaluate a byte-bounded prepared-page/display-list cache in the Rust engine. Do not claim retained display lists explain the older release's performance. |

There are also reasons not to copy constants or implementation wholesale. Both inspected Sumatra cache headers cap bitmap **count** at 128 and explicitly say byte-based accounting would be better. The release uses one render thread; development has a lazy worker pool, yet its current display-list replay holds a render lock because concurrent image decoding caused failures. More threads are not automatically faster or smaller. Cancellation is cooperative: even development comments note that a large image decode can delay it.

## Findings in Breeze

### 1. Whole-page pixels and temporary copies amplify memory

[`PdfWorker::request_page`](../../crates/breeze-pdf/src/lib.rs) builds a whole-page raster at `ceil(scale * 72)` DPI. `MAX_PAGE_PIXELS` is 268,435,456: up to 1 GiB for one four-byte pixel buffer, before engine intermediates. This is a pathological-input output limit, not a process budget.

[`RasterView::insert`](../../crates/breeze-app/src/raster.rs) starts eviction after rendering and conversion. It preserves visible and transition images, and inserts the new image even when nothing eligible remains to evict. Therefore the 96 MiB constant is not a hard cache or RSS ceiling. Even a predicted page can be admitted beyond it.

PDF Oxide's raw output uses `pixmap.data().to_vec()`, so the drawing surface and returned buffer coexist during handoff. Breeze's nonzero view rotation allocates another pixel buffer. CPU `RenderImage` storage, GPU uploads, transition images, fonts, source bytes, decoded objects, and image/compositing scratch allocations all sit outside a single raster's byte count. On unified-memory hardware, RSS and GPU-reported bytes must not simply be added as if they were disjoint.

`page_raster_scale` still estimates oversampling cost from the viewport area, despite the current allocation being the entire page. At fit width, reserving the collapsed-sidebar density can render more pixels than currently needed. Use the whole-page extent and actual rounded DPI for admission while this backend remains whole-page.

### 2. Prefetch can repeatedly evict and regenerate itself

`RasterView::commit` admits prediction by page identity without a byte estimate. `next_key(true)` selects the first missing prediction. `insert` can evict another prediction to make room. Completion immediately calls `pump_visible` again.

A deterministic case follows directly from those rules:

```text
Budget: 96 MiB. Visible page: 40 MiB. Two predicted pages: 40 MiB each.
Render A -> keep visible + A (80 MiB).
Render B -> evict A; keep visible + B (80 MiB).
Render A -> evict B; keep visible + A (80 MiB).
Repeat while demand stays unchanged.
```

This was reproduced in a small model of the admission/eviction rules, not in a native GUI trace. The user's exact reading state has not been shown to trigger it. It is nevertheless a concrete scheduler risk: there is no stable admitted prediction set and no backoff for a prediction that cannot remain resident. Small placeholder-pixel tests do not exercise this condition.

### 3. Visible thumbnails are scheduled after speculative work

[`Breeze::pump_visible`](../../crates/breeze-app/src/reading.rs) currently chooses:

```text
visible metadata (including glyph extraction)
-> visible page raster
-> predicted-page metadata
-> predicted-page raster
-> visible navigation thumbnails
```

The UI permits one outstanding request across these paths. The worker itself has one running and one replaceable pending job; a running render is not interrupted. Thus a thumbnail that takes only milliseconds can wait behind multiple much more expensive page requests. The prefetch cycle above can indefinitely postpone thumbnail work while its conditions hold.

Change scheduling policy before adding threads: visible page pixels first, then timely visible-thumbnail and interaction work, then admitted predictions. Use bounded fairness so neither selection metadata nor visible thumbnails can starve. Discarding stale responses is already implemented; cooperative engine cancellation is still missing.

### 4. Source and resource memory is significant even for small rasters

[`read_source`](../../crates/breeze-pdf/src/lib.rs) reads the complete file. PDF Oxide's `PdfDocument::from_bytes` clones it into `source_bytes` while retaining the original in a reader. The user's file therefore accounts for about **35.57 MiB in two source-byte buffers alone**, excluding allocator spare capacity and parsed objects.

[`search`](../../crates/breeze-pdf/src/lib.rs) constructs a separate document. During concurrent reading/search, source-byte payload can reach approximately four times file size, about 71.14 MiB for this input, before both documents' caches. Changing `from_bytes` to `PdfDocument::open` does not fix this: the inspected `open` reads the file and calls `from_bytes` too.

Document internals have a mixture of byte limits, entry limits, and unbounded-by-byte maps. System font data/faces have process-wide caches. These are candidates for allocation profiling, not proof that every cache leaks. The file's source copies explain only part of the reported difference.

### 5. Opening and covers perform avoidable prerequisite work

Loading a new source computes its fingerprint, enumerates geometry for every page, loads the outline, and extracts characters for the requested metadata page before the reader requests pixels. All geometry must currently succeed before even a cover-only request returns. The single loaded engine document is replaced on source switches, including successive Home covers.

Separate geometry needed for the first display from selection/search preparation; consider incremental layout metadata with stable scroll geometry. Cover requests should not require the reader's complete outline and glyph work. Preserve required unsupported-feature reporting and source validation before applying annotations. A persistent cover cache avoids much of this work on later launches without retaining an engine per library book.

### 6. Already implemented mechanisms should be preserved

Breeze already has virtualized lists, a bounded pending job slot, stale-result rejection, low-DPI thumbnail requests without glyph extraction, raw BGRA handoff, separate annotation overlays, and explicit GPU image eviction. Worker thumbnail/metadata results have a 32 MiB cache; the UI has separate 8 MiB metadata, 8 MiB thumbnail, and 16 MiB cover budgets. These limits are not collectively a process limit.

New `PageRenderer` instances do **not** rescan all system fonts each time: this engine already shares a process-wide font database and font-face data. Reusing a renderer alone should not be sold as eliminating repeated font discovery. Content operators are still parsed and executed for each raster; prepared-page reuse is a different, larger optimization. Some image decoding already uses the target device footprint, so codec work should be profiled before proposing generic downsampling changes.

## Local Release measurements

Environment: Apple M1 Pro, 16 GiB RAM, macOS 15.8.1 (24H32), Rust 1.99.0. Built with `cargo build --release --locked -p breeze-pdf --example resolution_probe`. Disposable Rust probes link the resulting workspace libraries with optimization and debug assertions disabled, and call the actual public `PdfWorker` APIs. Processes run sequentially, one DPI per process.

RSS below is the **maximum resident set of the probe process** from macOS `/usr/bin/time -l`, including source/engine/result allocations. It excludes the Breeze GUI, application raster residency/prediction, and GPU textures. These are not the user's observed application memory counters or a Windows comparison. No disk-cache flush was performed.

### User's 343-page book

First load requests metadata for page index zero. Each pass requests 20 spread page indices `floor(i * 342 / 20)` for `i = 1..20`; the second pass repeats them in the same process. The first pass is document-warm but visits previously unrasterized pages. Final page rasters are uncached in the worker; the second thumbnail pass hits its result cache. Quantiles use sorted sample indices 9 and 18 (20 observations). Maximum latency is also shown because p95 can hide the slowest page in this small sample.

| DPI / request | Maximum output buffer | First-pass p50 / p95 / max | Repeat-pass p50 / p95 / max | Process peak RSS |
|---|---:|---:|---:|---:|
| 24 / thumbnails | 0.16 MiB | 8.63 / 21.54 / 47.96 ms | 0.03 / 0.08 / 0.13 ms | 174.44 MiB |
| 144 / pages | 5.70 MiB | 19.47 / 34.86 / 71.11 ms | 21.68 / 25.44 / 61.00 ms | 239.89 MiB |
| 288 / pages | 22.80 MiB | 52.68 / 78.43 / 164.44 ms | 44.69 / 81.75 / 289.13 ms | 264.27 MiB |
| 432 / pages | 51.31 MiB | 104.70 / 243.54 / 625.79 ms | 101.26 / 239.72 / 610.51 ms | 286.11 MiB |

Output dimensions on the last sampled page were 183 × 228, 1094 × 1366, 2188 × 2732, and 3282 × 4098 respectively. Initial metadata requests in these four processes took 108.94, 50.28, 59.21, and 51.83 ms; those single observations are not cold-opening quantiles. Source bytes had already been read for identification, so these runs do not establish cold-disk performance.

The first-pass 432-DPI maximum was page index 342 (display page 343). The result supports two separate investigations: raster scale can make page turns expensive, and substantial engine memory is present even when thumbnail pixel buffers are tiny. It does not assign all 350 MB to page textures or establish a memory leak. Fixed DPI is used for reproducibility; it is not a claim that the user's maximized window selected that DPI. The 144-DPI page sample meets the proposed 100 ms raster p95; higher-DPI misses cannot silently redefine that gate.

### Bundled page: isolate scale growth

The project-authored `field-notes.pdf`, page index zero, 612 × 792 pt, SHA-256 `38ee1558450f88101995c5610c4814df599b275abde9ef939edf5a797da5cf4e`, is embedded in memory. After metadata, each process performs one first raster and 20 same-page rerenders, with no worker page-raster cache. These timings include the worker round trip, rasterization and pixel conversion, not presentation.

| DPI | Pixel dimensions | Output | First raster | Warm p50 / p95 | Process peak RSS |
|---|---|---:|---:|---:|---:|
| 144 | 1224 × 1584 | 7.40 MiB | 23.74 ms | 18.52 / 20.36 ms | 65.52 MiB |
| 288 | 2448 × 3168 | 29.58 MiB | 67.09 ms | 64.45 / 66.99 ms | 73.12 MiB |
| 576 | 4896 × 6336 | 118.34 MiB | 274.39 ms | 241.84 / 250.35 ms | 251.12 MiB |
| 769 | 6537 × 8459 | 210.94 MiB | 458.35 ms | 433.35 / 468.22 ms | 436.20 MiB |

769 DPI models the current rounded request for 400% at a 2× display. The raw-buffer copy alone can temporarily require about 422 MiB for two such pixel arrays. An illustrative 2048 × 1536 visible region is only 12 MiB; this is arithmetic, not a measured region implementation. The zoomed probe is not an ordinary-reading acceptance test.

The existing persistent-renderer `resolution_probe` was also run at 144 and 384 DPI: warm p50/p95 were 18.25/20.44 ms and 103.21/109.11 ms. It does not establish a meaningful speedup from merely retaining the renderer. All raw logs, downloaded reference sources, probe source, and the scheduling model remain under `target/performance-research/` and are excluded from Git.

Verification for this documentation-only investigation: the locked Release probe build, formatting check, documentation-link checks, recorded quantile checks, and unchanged-input SHA-256 check passed. Workspace tests, Clippy, application startup, native visual inspection, and a Windows comparison were not rerun or claimed as passed.

## Proposed order and acceptance checks

| Priority | Proposed change | Dependency / expected benefit | Verification before claiming success |
|---|---|---|---|
| P0 | Reserve bytes before speculative rendering; admit a stable subset; do not retry rejected predictions until demand or capacity changes. | Application scheduler. Stops avoidable allocations, eviction churn, and thumbnail starvation. Preserve requested visible-page quality. | A 40 + 40 + 40 MiB demand against 96 MiB reaches idle; oversized predictions do not start; visible thumbnails complete; native trace has no repeating idle renders. |
| P0 | Give visible thumbnails bounded service ahead of full-resolution neighbor prediction; decouple visible geometry/pixels from nonessential glyph work. | Application scheduling. Improve perceived thumbnail and page arrival without multiplying documents. | Timestamp queue wait, render, conversion, upload and first paint separately; verify selection/search still become ready and input remains responsive. |
| P1 | Inventory CPU/GPU and in-flight memory; correct whole-page estimates; retain only necessary transition images and canonicalize raster identity by effective DPI. | Application ownership/accounting. Removes redundant rasters and explains the remaining baseline. | Same-file scroll/zoom/back-tab/close/idle scenarios plateau; no stale source reuse; old images disappear from the atlas. Avoid counting shared buffers twice. |
| P1 | Persist bounded Home covers; reuse an existing page raster for a thumbnail when available and cheap. | Storage and image work off the UI thread. Faster repeat launch/navigation; lower redundant rendering. | Revision/rotation/size/render-version cache identity, bounded disk and decoded-image residency, no UI-thread image encoding, changed-source invalidation. |
| P1, upstream | Eliminate duplicate source ownership, expose resource-byte accounting, and evaluate file-backed input. | Published Rust engine API. Lower baseline/search overhead; important for large files. | Measure source, parsed-object, font, decoded-image and scratch allocations separately. Repeated search should not retain additional documents. Preserve source consistency. |
| P1, upstream | Add a true device-region raster API; use whole-page rendering only when its estimated cost is small. | Engine support is required. Largest structural improvement for high-DPI fit width and zoom. | Output allocation follows region size; crop/rotation/UserUnit/alpha/color fidelity passes; page turns arrive coherently rather than as a visible grid; measure decoder peaks separately. |
| P2, upstream | Byte-bounded retained page preparation and cooperative cancellation. | Engine changes. Avoid repeated parsing and shorten obsolete work; consumes extra retained memory. | Separate preparation from replay cost; verify invalidation and eviction; test cancellation during slow operators and disclose noninterruptible decodes. |

The stock `render_page_region` is explicitly a full-page render followed by decoding/cropping/re-encoding; it cannot deliver the region-memory improvement. Do not replace the current path with that function and call it tiling. A hard bound on the visible page's pixels while preserving arbitrary zoom requires genuine engine support. App-side prefetch fixes can reduce waste in the meantime, but cannot provide that guarantee.

This assessment originally left engine choice open. The user subsequently
selected MuPDF via `mupdf-rs`; see [the strategy decision](10-engine-strategy-comparison.md)
and [migration plan](../plans/22-mupdf-migration.md). More rendering threads, a
larger cache, lower final DPI, or removal of ICC/antialiasing are not substitutes
for measured region rendering. A bounded display-list cache and thumbnail lane
also need memory budgets; caching everything would recreate the original
problem. Sumatra's GPLv3 C++ implementation is a reference for mechanisms, not
code to transplant into Breeze.

For the next application benchmark, use an isolated `BREEZE_DATA_DIR` and a local copy/reference of the user's input, plus redistributable text/CJK/image/scan/complex-vector fixtures. Record build identity, hardware/OS, logical and device viewport sizes, scale factor, fit mode, sidebar state and exact page sequence. Measure launch/open-to-first-readable-frame, thumbnail-first/all-visible arrival, page-turn p50/p95/max, cache hits, wasted/aborted work, idle CPU, process private/RSS memory, GPU allocations and close-after-idle behavior. Run an equivalent native Windows workload in SumatraPDF before promising its 120 MB figure as an attainable Breeze target.

## Source references

1. SumatraPDF release [RenderCache.cpp](https://github.com/sumatrapdfreader/sumatrapdf/blob/16c59fde8b824ab54c56f23aef910a6fdd874ad0/src/RenderCache.cpp#L359) and [RenderCache.h](https://github.com/sumatrapdfreader/sumatrapdf/blob/16c59fde8b824ab54c56f23aef910a6fdd874ad0/src/RenderCache.h#L11): tiles, request ordering, cancellation, transition painting and count limits.
2. SumatraPDF release [EngineMupdf.cpp: file input](https://github.com/sumatrapdfreader/sumatrapdf/blob/16c59fde8b824ab54c56f23aef910a6fdd874ad0/src/EngineMupdf.cpp#L437) and [region rasterization](https://github.com/sumatrapdfreader/sumatrapdf/blob/16c59fde8b824ab54c56f23aef910a6fdd874ad0/src/EngineMupdf.cpp#L2985).
3. SumatraPDF release [FileThumbnails.cpp](https://github.com/sumatrapdfreader/sumatrapdf/blob/16c59fde8b824ab54c56f23aef910a6fdd874ad0/src/FileThumbnails.cpp#L22): cached file/Home previews and modification-time checks.
4. SumatraPDF development [PageThumbnails.cpp](https://github.com/sumatrapdfreader/sumatrapdf/blob/bf941f965c6d94f0af4f56792099373337d93c65/src/PageThumbnails.cpp#L636): visible-first thumbnail batches and retention ranges.
5. SumatraPDF development [RenderCache.cpp](https://github.com/sumatrapdfreader/sumatrapdf/blob/bf941f965c6d94f0af4f56792099373337d93c65/src/shared/RenderCache.cpp#L833) and [header](https://github.com/sumatrapdfreader/sumatrapdf/blob/bf941f965c6d94f0af4f56792099373337d93c65/src/shared/RenderCache.h#L11): chained prediction, thread and cache limits.
6. SumatraPDF development [display-list preparation](https://github.com/sumatrapdfreader/sumatrapdf/blob/bf941f965c6d94f0af4f56792099373337d93c65/src/shared/EngineMupdf.cpp#L6288) and [replay](https://github.com/sumatrapdfreader/sumatrapdf/blob/bf941f965c6d94f0af4f56792099373337d93c65/src/shared/EngineMupdf.cpp#L7058).
7. PDF Oxide [source-byte ownership](https://github.com/yfedoseev/pdf_oxide/blob/ad49c4cb3638dc950a29350ef882161372fde473/src/document.rs#L1087), [page rasterization](https://github.com/yfedoseev/pdf_oxide/blob/ad49c4cb3638dc950a29350ef882161372fde473/src/rendering/page_renderer.rs#L478), and [post-render region crop](https://github.com/yfedoseev/pdf_oxide/blob/ad49c4cb3638dc950a29350ef882161372fde473/src/rendering/mod.rs#L489).
