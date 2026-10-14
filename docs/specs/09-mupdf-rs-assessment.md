# MuPDF Rust binding assessment

Date: October 14, 2026. Status: feasibility assessment and isolated probe, not an engine replacement decision.

## Verdict

`mupdf-rs` is technically capable of replacing the current PDF reading backend, and it offers the missing primitive needed for the memory problem: record a page once as a display list, then replay only the requested device rectangle into a clipped pixmap. The smallest viable migration keeps `breeze-pdf` as the adapter, keeps `lopdf` for the restricted annotation-copy exporter, and keeps all MuPDF types behind that crate boundary.

The replacement is **not admissible under the current product constraints**. `mupdf-rs` is a safe Rust wrapper around `mupdf-sys`, which compiles MuPDF's C/C++ source. The crate and MuPDF are AGPL-3.0. The contributor guide currently forbids MuPDF and C/C++ PDF engines. Adopting this path therefore requires an explicit product and licensing decision before implementation.

If that decision changes, this is the most direct path to true region rendering without waiting for a PDF Oxide upstream API. It is a backend migration, not a dependency rename.

## Evidence inspected

- Crates.io package: `mupdf 0.8.0`, with `mupdf-sys 0.8.0`; the isolated probe used `default-features = false` and only `base14-fonts`.
- Source checkout inspected: `messense/mupdf-rs` commit `537d50556ee8e4abf2435f81357dfef3c145d883`, dated September 14, 2026. The repository contains a MuPDF git submodule at `mupdf-sys/mupdf`.
- The wrapper README documents reading, rendering, text extraction, outlines, annotations, display lists and PDF objects. The source exposes the APIs listed below.
- The probe compiled and ran on this host with the macOS Rust toolchain. It built the C/C++ MuPDF dependency through Cargo. Windows and Linux builds were not run in this session.

References: [mupdf-rs](https://github.com/messense/mupdf-rs), [mupdf-rs README](https://github.com/messense/mupdf-rs/blob/537d50556ee8e4abf2435f81357dfef3c145d883/README.md), [MuPDF](https://github.com/ArtifexSoftware/mupdf), and [MuPDF licensing](https://mupdf.com/licensing).

## Region-rendering path

The relevant safe wrapper flow is:

1. `Document::open` or `Document::from_bytes` opens the file.
2. `Document::load_page` obtains page geometry and content.
3. `Page::to_display_list(false)` records page operators once. The flag can exclude page annotations, which matches Breeze's sidecar-overlay model.
4. `Pixmap::new_with_rect` allocates only the device rectangle needed by a tile or viewport.
5. `Device::from_pixmap_with_clip` creates a clipped draw device.
6. `DisplayList::run_with_cookie` replays the list with the requested transform, clip and cancellation cookie.
7. The pixmap samples are copied into Breeze's BGRA `RenderImage` format.

`Page::to_pixmap` remains available for a whole-page fallback and thumbnails. `DisplayList::to_pixmap` is useful for simple whole-list output, while the device path is the one that permits an application-owned clip rectangle.

This is materially different from PDF Oxide's current `render_page_region`, which renders a full page and crops the result afterward. A MuPDF display-list replay still has decoder and image-store costs, but the output pixmap and most raster work follow the requested region.

## Isolated probe

Input: the local 343-page PDF supplied for the Breeze memory report, 18,648,370 bytes, SHA-256 `00660ce85cecc47817ab84fa4cae241e73d94f77ace38376e0fd459acb335f4e`. The probe sampled 21 page indices across the document in Release mode. It was a standalone process, so these numbers exclude Breeze UI, GPUI image atlas allocations, annotations, and application scheduling.

| MuPDF request | Output | p50 | p95 | max | Probe peak RSS |
|---|---:|---:|---:|---:|---:|
| Whole page, 2x matrix (about 144 DPI) | 1094 x 1366, 5.70 MiB | 5.70 ms | 44.86 ms | 49.54 ms | 77.4 MiB |
| 512 x 512 clipped display-list replay | 1.00 MiB | 2.65 ms | 17.76 ms | 39.83 ms | 26.1 MiB |

The region probe creates a display list for each sampled page and replays it once. It does not yet measure a retained display-list cache, GPU upload, scroll scheduling, or repeated zooms. The result shows the expected allocation difference; it does not prove a final Breeze working-set number or visual fidelity.

## Breeze capability mapping

| Breeze responsibility | MuPDF Rust surface | Migration note |
|---|---|---|
| Page count and open | `Document::open`, `from_bytes`, `page_count` | Prefer file-backed `open` for external PDFs; do not duplicate the source bytes unless a caller needs them for fingerprinting. |
| Password state | `needs_password`, `authenticate` | The product currently defers password documents, so keep the visible unsupported state until scope changes. |
| Page geometry | `Page::bounds`, `PdfPage::media_box`, `crop_box`, `rotation` | Revalidate CropBox, nonzero origins, rotation and the current `UserUnit == 1` restriction. |
| Outline | `Document::outlines` with nested `down` entries and destinations | Flatten into Breeze's existing title/depth/page model while preserving unsupported external destinations. |
| Text and selection | `Page::to_text_page`, `TextPage::chars`, `TextChar::quad`, `words` | Better fit for glyph quads and complex text than the current adapter, but coordinate mapping still belongs in Breeze. |
| Search | `TextPage::search` or `DisplayList::search` | A page-local indexed/cacheable search adapter can replace the current repeated text extraction. |
| Page raster | `Page::to_pixmap` | Whole-page fallback and thumbnail path. |
| Region raster | `to_display_list`, `Pixmap::new_with_rect`, `Device::from_pixmap_with_clip`, `DisplayList::run_with_cookie` | Main reason to migrate. Cache identity must include source revision, page, rotation, transform, clip and effective device scale. |
| Cancellation | `Cookie::abort`, `progress`, `errors`, `incomplete` | Add an abort flag to each job. The worker must retain the cookie while MuPDF runs and discard stale output after abort. Large decoder operations may still respond between checkpoints. |
| Sidecar annotations | Breeze's existing overlays and `breeze-storage` | Keep page content read-only. Render sidecars in GPUI; do not use MuPDF page editing for normal annotation saves. |
| Restricted export | Existing `lopdf` exporter | Keep this separate until an independent MuPDF export comparison passes. A reader-engine switch does not authorize changing export semantics. |

## Thread and cache model

The wrapper's `Document`, `Page`, `Pixmap` and most context-bound objects are not advertised as general `Send + Sync` values. Its context module uses one process-wide base context by default; cloned thread contexts share MuPDF's store, caches and locks. `init_thread_context` can create an independent context, but each independent context has its own font/resource store and increases memory.

The safest first integration keeps the existing single `breeze-pdf` worker and owns the MuPDF `Document`, page objects, display lists and context on that worker thread. Do not send pages or pixmaps to the UI. Convert samples to an owned `Pixels` buffer before crossing the worker boundary. If future parallel rendering is needed, measure separate contexts and their duplicated stores; adding threads may increase the working set.

MuPDF's context exposes a maximum store size. Set it deliberately for the reader worker and record evictions/misses. This is an internal resource-store limit, not a complete RSS limit: output pixmaps, display lists, decoder scratch, font data, application caches and GPU textures need separate budgets.

Display lists are marked `Send` and `Sync` in the wrapper, but they retain a context-family check and their documentation says they must be used and dropped under the family that recorded them. Keeping them on the worker avoids a subtle lifetime and context mismatch.

## Major blockers and risks

### Product and license

The repository explicitly forbids MuPDF and C/C++ PDF engines. `mupdf` and `mupdf-sys` declare AGPL-3.0. A distributable Breeze binary would need a licensing review covering the application license, source-offer obligations, dynamic/static linkage, bundled MuPDF source, notices, and whether an Artifex commercial license is required. This is a release blocker, not a Cargo detail.

### Build and packaging

`mupdf-sys` runs bindgen and compiles a large C/C++ tree. The wrapper README requires a C/C++ toolchain and libclang; Linux system-font builds need Fontconfig development files. The repository's own CI covers Linux, macOS and Windows variants, but Breeze has only verified the isolated build on macOS. Windows MSVC/SDK, libclang discovery, linker settings, package size, and Linux system dependencies need native checks before changing the workspace dependency.

### Fidelity and behavior

MuPDF's broad PDF support is promising, but it does not remove Breeze's acceptance gates. Compare representative text/CJK/subset/Type 3 fonts, scans, JPEG2000/JBIG2, ICC/transparency, cropped and rotated pages, malformed documents, outline destinations, text quads, and annotation appearance handling. Keep the current source fingerprint and rejection limits while the adapter is under test.

### Adapter scope

The current `breeze-pdf` adapter depends directly on PDF Oxide types for page geometry, glyphs, outlines, rendered pixels, text extraction and error paths. A replacement needs a new internal engine-neutral mapping inside `breeze-pdf`; changing imports in `breeze-app` would leak backend types and make future format support harder. The existing `PdfWorker` queue can remain, but its `Loaded`, `Page`, `Glyph`, render and search implementations need to be rewritten.

## Recommended migration sequence if constraints change

1. Record the license decision and update the product/architecture specifications before adding the dependency.
2. Add a feature-gated, isolated MuPDF adapter inside `breeze-pdf`; do not add a runtime fallback engine or expose MuPDF types to `breeze-app`.
3. Implement metadata, outlines, text quads, page rotation/CropBox, thumbnails and whole-page rendering first. Keep annotations and export on the existing paths.
4. Add display-list caching and clipped region replay. Use a byte budget for pixmaps and a separate budget for retained display lists. Make visible work outrank prediction and thumbnails according to the performance assessment.
5. Add cancellation, source-revision checks, cache invalidation and worker-store limits. Verify all stale-result and close-document paths.
6. Run pixel and semantic corpus comparisons against the current engine and an independent validator. Run the user's 343-page book in Release on macOS, Windows and Linux, recording open-to-first-frame, page-turn p50/p95/max, thumbnail arrival, private/RSS memory, GPU allocation and idle plateau.
7. Only after those gates pass, remove PDF Oxide and update the lockfile, packaging, notices, contributor guide and release documentation.

## Decision table

| Question | Assessment |
|---|---|
| Can it provide true region rendering now? | Yes. The safe API is sufficient for a clipped pixmap and display-list replay. |
| Is it likely to address Breeze's high-DPI memory spike? | Yes, materially. The isolated 512 x 512 probe used 1 MiB of output versus 5.7 MiB for the sampled full page, before application/GPU overhead. |
| Is it a drop-in replacement? | No. `breeze-pdf` needs a substantial adapter rewrite and new fidelity tests. |
| Does it preserve the current Rust-only engine constraint? | No. It brings MuPDF C/C++ through `mupdf-sys`. |
| Is it license-neutral? | No. AGPL-3.0 requires a legal/product decision. |
| Should Breeze adopt it immediately? | Only after the constraint and license decisions are changed. Otherwise keep the current engine and implement scheduler/memory fixes that do not require an engine swap. |
