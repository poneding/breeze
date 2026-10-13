# Page-turn presentation: one region per viewport

## Report

On October 11, 2026 the user reported that turning a page in the reader draws the page
block by block instead of appearing complete. They compared it with Skim (macOS) and
SumatraPDF (Windows), where a page turn is instantly sharp.

The earlier device-resolution work (work package 09) fixed a real fidelity problem: a fixed
144 DPI page raster stretched to the window was visibly blurry. It introduced a 508-pixel
cell grid, and the reader then requested one cell per worker job. At 2x display scale a
fit-width viewport is roughly 3 x 4 to 6 x 4 cells, so one page arrived as 12 to 24 separate
results through a single-threaded, single-slot worker. Each result was painted as it landed,
which is the reported block-by-block assembly.

## Why the reference viewers do not have this problem

**PDFKit (Skim)** does not rasterize a page for display at all. `PDFView`'s default
`drawPage:toContext:` "erases page to white and calls
`[page drawWithBox: [self displayBox] toContext: context]`", drawing the page as vector
content into the window's graphics context. There is no CPU raster step, so there is nothing
to assemble. GPUI requires a texture, so this path is not available to Breeze. Recorded here
so the difference is not mistaken for a tuning gap.

**MuPDF (SumatraPDF)** rasterizes, and uses three mechanisms:

- A cached `fz_display_list` per page (`EngineMupdf.h`), replayed for each render. MuPDF
  documents the intent as "a caching-mechanism to reduce parsing of a page" and
  `fz_run_display_list(ctx, list, dev, ctm, scissor, cookie)` replays it under any transform.
- `RenderCache::GetTileRes` returns `res = 0`, meaning one whole-page bitmap, unless the page
  exceeds `maxTileSize` (the screen size) by more than about 1.5x, and the threshold is halved
  for Fit Page/Fit Width. Ordinary reading zoom therefore renders a complete page, not tiles.
- Predictive rendering: `kMaxPredictiveRequests = 4` pages ahead, "one at a time (chained), so
  they don't flood the render queue", with `MAX_BITMAPS_CACHED = 128` and `MAX_PAGE_REQUESTS = 8`.
  The chain is anchored to the visible page and stops when it is no longer visible.

Sources: `sumatrapdfreader/sumatrapdf@82450d4c` `src/RenderCache.h`, `src/RenderCache.cpp`,
`src/EngineMupdf.h`; MuPDF `fitz/display-list.h` and `fitz/util.h`.

The perception of an instant turn comes mainly from the third mechanism: while the reader
looks at page N, pages N+1 and N+2 have already been rasterized at the current zoom.

## Why not copy whole-page rendering

Breeze rasterizes at true device pixels, which is four times the pixel count of SumatraPDF's
96 DPI output on a 2x display. Measured/derived whole-page rasters:

| Condition | Whole-page raster |
|---|---:|
| 1x, Fit Width, 1392 logical px viewport | 1392 x 1800 = 2.5 Mpx = 10 MiB |
| 2x, same viewport | 2784 x 3604 = 10.0 Mpx = 40 MiB |
| 2x, 2304 logical px window | ~23 Mpx = 91 MiB |

A whole-page raster would exceed the previous 56 MiB pixel budget for a single page, so the
SumatraPDF policy cannot be adopted literally without either giving up device resolution or
raising memory far beyond the accepted target. The next section therefore keeps the cell
partition and changes only what is requested and when.

## Implemented behavior

**One region per viewport.** `RasterView::next_group` takes the cell the existing priority
order selected as an anchor, collects the missing cells of the same page, rotation and scale,
and grows a grid-aligned bounding rectangle from the anchor in order of cell distance while the
region stays within `MAX_REGION_PIXELS` (12 Mpx, about 48 MiB of RGBA). The worker renders that
one region; `RasterView::insert_group` slices it back into the unchanged per-cell cache. A
region never mixes rotations, and the request carries the group's rotation so the engine's
existing region-to-source mapping and rotate-back path are reused unchanged.

The partition, seam gutters, per-cell cache, eviction, previous-resolution fallback, coverage
rules and GPU release path are untouched. Every member cell is a sub-rectangle of the same
page transform translated by the region origin, so a slice reproduces the cell that the
previous schedule rendered separately.

**Predictive prefetch of the two following pages.** `Breeze::lookahead` predicts forward two
pages (one page width of rows in Spread layouts) when the visible region already reaches the
page end, so a stale reading position cannot keep predicting, and backward one page when the
visible region starts at the page top. Prefetch keys are the neighbor page's viewport at its
own device scale.

**Budgets.** The pixel cache is 96 MiB, with a 56 MiB ceiling on the prefetch share; the
remainder stays available for visible pixels. On a 2x Fit Width display one page is roughly
20-40 MiB, so the current page plus two predicted pages fits, and larger windows degrade to
fewer predicted pages rather than exceeding the budget.

**Region ceiling is a shared contract.** `breeze_pdf::MAX_REGION_PIXELS` bounds both
`DeviceTile::new` and the renderer's `max_output_pixels`, which previously rejected any region
larger than 4 Mpx and capped `DeviceTile` at 512x512. A cell remains at most 512x512 so a
guttered cell still fits one 512-pixel atlas texture.

## Validation

`crates/breeze-pdf/tests/device_tiles.rs` adds the central equivalence test: one whole-page
region request, sliced at each interior cell's offset, is compared with that cell requested
separately, at both an integer (2.0) and a fractional (2.3466) device scale and at all four
view rotations. Agreement is **not** byte equality. A handful of antialiased edge pixels take
a different coverage path depending on the size of the surface being rasterized: measured on
this fixture, 12-33 bytes of about 2,048,000 differ by more than 2 units, with a maximum of 41
of 255. That is the tolerance the existing whole-page comparison already applies to tile seams,
and it is two orders of magnitude tighter than a one-pixel misalignment, which would change
roughly half of the compared bytes. The test bound is 1 changed byte per 10,000 compared, with
a 64-unit ceiling that a content change would exceed.

The same file checks the relaxed region ceiling, that a quarter-turned region keeps its
requested pixel extents, and a device viewport-sized region is accepted while an unallocatable
one is still refused.

`crates/breeze-app/src/raster/region_tests.rs` adds six component tests over real canvases:
one request covers every visible cell, a 400% zoom viewport is still grouped and stays under
the ceiling, rotated views group cells of their own rotation, both following pages are
predicted without navigating, a region slices into every member cell in one result, and a
failed region marks every cell it covered.

`crates/breeze-pdf/examples/region_probe.rs` measures one region against the same cells
rendered separately (Release, Apple M1 Pro, macOS 15.7.9, bundled `field-notes.pdf` page 0,
a 3 x 2 cell block = 1528 x 1020 = 1.56 Mpx, 20 warm repeats per path):

| DPI | One region (1 job) | Same cells separately (6 jobs) | Cell p50 / p95 |
|---|---:|---:|---:|
| 288 | 26.1 ms | 30.9 ms | 7.50 / 8.86 ms |
| 384 | 13.6 ms | 16.6 ms | 3.87 / 4.59 ms |

Total raster time changes little; the arrival pattern changes completely. Separately the cells
land roughly one cell p50 apart, so the last of six arrives 30-40 ms after the first, and
nothing is complete until they all do. The 288 DPI rows are consistently slower than the
384 DPI rows; that was not investigated and is not a claim about DPI scaling.

Real application trace (`cargo run --locked -p breeze-app -- assets/samples/field-notes.pdf`,
Release, `BREEZE_TRACE_RENDER=1`, isolated `BREEZE_DATA_DIR`):

```
metadata page=0 / page=1 / page=2
page=0 region 1195x1546 (1.85 Mpx, 7.39 MB) 29.07 ms   <- visible page, one job
page=1 region 1195x1546                     15.19 ms   <- predicted, before any turn
page=2 region 1195x1546                     14.68 ms   <- predicted, before any turn
```

The same viewport previously required 12 cell jobs per page (3 columns x 4 rows), so 36 jobs
became 3, and the two pages a reader is about to reach are already resident.

**Interaction with rendering-integrity failures.** The concurrent rendering-integrity
follow-up (recorded in `vendor/pdf_oxide/BREEZE-PATCH.md`) makes the engine propagate a failed
image, mask, Form or shading draw instead of returning incomplete pixels, and Breeze then
refuses such a result. This change is compatible with that behavior: a region is rendered by
executing the page's operators, so one damaged draw anywhere on the page fails every region of
that page, every cell of that page is marked failed, and the reader shows its page-rendering
notice instead of partial content. `insert_group` runs only after every check passes, so a
failed result is still never admitted to Breeze's pixel cache.

**Localization.** The new messages reuse existing catalog keys (`页面分块不匹配`,
`渲染器未返回所需清晰度`) instead of adding untranslated strings, so
`every_literal_call_has_a_catalog_entry` passes without new entries. `region_tests.rs` is not in
that test's scanned file list and contains no user-facing text.

Checks: `cargo fmt --all -- --check`, `cargo test --workspace --locked` (187 passed, three
existing ignored probes, zero failed) and
`cargo clippy --workspace --all-targets --locked -- -D warnings` all pass.

## Not done, and open risks

- **Native GUI inspection was not performed.** The Computer Use path was unavailable in this
  session (`orca` is not installed), so no screenshot or pointer interaction confirms the
  visual result. The evidence above is the application's own render trace, component tests and
  probes. This is a tool limitation, not a passed visual check.
- **Retained page preparation (display list) is still unimplemented.** `PageRenderer` clears
  its font, color space and ICC transform caches on every render call
  (`vendor/pdf_oxide/src/rendering/page_renderer.rs`), and each call re-interprets the page.
  This is why a cold region costs tens of milliseconds rather than single digits, and it is
  the remaining reason a cold turn is not instantaneous. It requires a deeper change in the
  pinned engine and is deliberately deferred until the current behavior is measured on real
  documents.
- **GPU residency is not covered by the 96 MiB pixel budget.** Each cached cell holds a texture;
  eviction releases it, but marked-up pages can hold roughly the same order of GPU atlas memory
  as CPU bytes.
- **Memory is not re-measured.** The 350 MiB RSS target has not been re-verified with the 96 MiB
  budget and predictive residency on real documents. Large windows at 2x scale reduce the number
  of predicted pages through the budget rather than bypassing it, but no native measurement
  confirms the resulting footprint.
- Cold (unpredicted) jumps can still show a brief incomplete state; the region removes
  block-by-block assembly of a page that is being prepared, it does not make every input
  render in one frame.
