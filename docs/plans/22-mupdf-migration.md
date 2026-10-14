# Work package 22: MuPDF reader migration

## Decision and objective

On October 14, 2026, the user selected MuPDF through `mupdf-rs` and accepted an
AGPL-compatible open-source distribution. Replace PDF Oxide as Breeze's reading
and rendering backend. Keep application logic/UI/core/storage in Rust, use the
`mupdf` 0.8.0 safe API over MuPDF C/C++, preserve PDF page content as read-only,
and retain `lopdf` for the existing restricted annotation-copy export.

The objective is a measured reduction in page-turn latency and raster working
set while preserving device-density output, text geometry, and supported PDF
fidelity. The isolated probe in [the strategy comparison](../specs/10-engine-strategy-comparison.md)
is feasibility evidence, not application acceptance.

## Boundaries

- All MuPDF objects remain owned by the `breeze-pdf` worker/adapter. No MuPDF
  types cross into `breeze-app` or `breeze-core`.
- Do not expose page-content editing. Sidecars remain the default annotation
  storage; export remains a separate `lopdf` job that writes a new PDF.
- Use only MuPDF's `base14-fonts` and `system-fonts` features initially. Do not
  enable MuJS, OCR/Tesseract, XPS, EPUB, HTML, CBZ, SVG, Brotli, or document
  writing unless a concrete product requirement needs it.
- Keep one document/render worker until measurements show a need for more.
  MuPDF contexts and display lists have thread-family ownership and cache costs.
- Pin `mupdf`/`mupdf-sys` in `Cargo.lock`. Do not introduce a second PDF engine
  as a runtime fallback.
- `mupdf-sys` compiles C/C++ and exposes AGPL-covered source. Keep exact source
  archives, `Cargo.lock`, notices, and build scripts available for every release.

## Phases

### 0. AGPL source and release path

- [x] Record the user's engine/license decision in contributor and product
  specifications.
- [x] Set the workspace license to `AGPL-3.0-only`; include root `LICENSE` and
  MuPDF's `COPYING` text.
- [x] Add a source bundle builder for a release revision plus vendored Cargo
  sources, and upload/checksum it with installers.
- [x] Include the license, MuPDF notice, and corresponding-source instructions
  in macOS, Windows, Linux, and local review packages.
- [ ] Audit enabled build features and packaged assets; record dependency
  notices and native build prerequisites.

### 1. MuPDF adapter compatibility

- [x] Replace `pdf_oxide` calls inside `breeze-pdf` while preserving the
  application-facing `PdfWorker`, `Page`, `Glyph`, `Outline`, `Pixels`, search,
  and fingerprint contracts.
- [x] Open external documents file-backed in MuPDF. Stream the source SHA-256
  and recheck file identity (size, mtime, device and inode; on Windows volume and
  file index) around open and on every request, so a replaced file reloads.
- [x] Map page count, MediaBox/CropBox, rotation, UserUnit policy, outline
  destinations, text quads, thumbnails, and whole-page output.
- [x] Keep document/page objects on the worker thread. Convert
  samples to owned pixels before returning through the bounded job queue.
- [x] Add focused regressions for open errors, geometry, outline flattening,
  stale generations, cancellation, BGRA/alpha, and source modification.
  `source_identity.rs` replaces a source through rename with an identical size
  and timestamp; `page_requests.rs` rejects a page beyond the document. A
  password-protected fixture and a deterministic write *during* open are still
  missing.

### 2. Clipped device-region rendering

- [x] Render clipped page regions on demand into byte-bounded pixmaps. The
  current adapter replays the page directly through MuPDF's clipped device; it
  does not retain display lists. Region keys include the source page-space
  origin and view rotation.
- [x] Return page-raster results with device regions, page-space
  clips, effective scale, rotation, source fingerprint, and owned pixel data.
- [x] Schedule viewport/adjacent regions with visible work first, byte admission
  before lookahead, stale-request cancellation, and stable prediction admission.
- [x] Paint region textures at matching page bounds, preserving annotation,
  selection, search, scroll anchoring, and rotation transforms.
- [x] Verify region stitching against whole-page output for four view rotations,
  PDF `/Rotate`, a nonzero CropBox, fractional-DPI whole-page output, zoom
  replacement, tile seams at 72 DPI, and GPU atlas eviction.
- [x] Report content MuPDF would drop silently. Page rendering verifies every
  drawn `Do`/`sh` resource, recurses into form XObjects with a depth and cycle
  bound, decodes named and inline `DCTDecode`/`JPXDecode` images with MuPDF's own
  image loader, and rejects an image mask whose stream is shorter than its
  declared scanlines. Damage verdicts are cached per object for the loaded
  document. `render_integrity` and the reader's end-to-end case are enabled.
- [ ] Verify image/mask/transparency/ICC fidelity across a representative corpus
  and source changes. The integrity pass covers undecodable images; visual parity
  for ICC, transparency and masks beyond the current fixtures is still open.
- [ ] Establish an effective MuPDF resource-store bound. Published `mupdf 0.8.0`
  does not expose the newer safe store-size setter found in its repository
  branch. If measurements exceed the budget, evaluate a narrowly scoped wrapper
  API extension before considering raw FFI in Breeze.

### 3. Search and fidelity parity

- [x] Move search extraction to MuPDF structured text.
- [ ] Add regressions for the 300-result cap and cancellation contract. The
  cancellation test passes; the cap is present in code but has no focused test.
- [ ] Compare semantic text and glyph quads on the existing corpus before
  switching annotation/selection geometry to the new adapter.
- [x] Keep rendering errors visible. Do not turn unsupported or malformed
  drawing operations into successful blank/partial pages. MuPDF drops some
  malformed content without a Cookie error, so `breeze-pdf::integrity` verifies
  what a page draws before its pixels are published; damage the engine recovers
  from (padded masks, tolerated truncation) still renders.
- [ ] Verify CJK/subset/Type 3 fonts, scans, JPEG2000/JBIG2, ICC/transparency,
  cropped/rotated pages, large page boxes, and malformed documents.

### 4. Performance and release acceptance

- [ ] Benchmark Release on macOS, Windows, and Linux with the user's PDF kept
  local plus redistributable text, scan, CJK, transparency, and vector fixtures.
- [ ] Record open-to-first-frame, visible-thumbnail p50/p95, page-turn
  p50/p95/max, UI frame time, RSS/private memory, GPU residency, cache plateau,
  output dimensions, and cold/warm conditions.
- [ ] Compare against PDF Oxide through the same Breeze application workload and
  memory scope; the isolated probes are not an end-to-end engine shootout.
- [ ] Run formatting, locked workspace tests, Clippy, startup, and native visual
  inspection on each available platform. Leave unavailable checks open.
- [ ] Remove PDF Oxide and obsolete full-page raster code/docs only after parity
  and platform release gates pass.

## Current gate

The source tree now selects MuPDF for the `breeze-pdf` reader adapter, and the
viewport region path is implemented and covered by deterministic geometry and
stitching tests. This is not production acceptance: the broad-corpus fidelity
render, missing encrypted/source-race regressions, unbounded MuPDF store, broad
fidelity corpus, Release same-workload benchmark, and native platform checks
remain open. Do not mark a phase complete while its listed regressions or gates
are outstanding.
