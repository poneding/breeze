# EPUB and Markdown reading

## Confirmed scope

On October 15, 2026, the user requested EPUB and Markdown reading, native
format-specific controls, Rust libraries where practical, and high performance.
PDF page numbers, thumbnails, rotation, annotation tools, and PDF export must
not become an application-wide document interface. Both new formats are
read-only. DRM, encrypted publications, scripting, and editing remain deferred.

## Integration assessment

Source and published crate APIs were inspected on October 15, 2026. Library
claims below are not application benchmarks.

| Candidate | Role and tradeoff | Decision |
|---|---|---|
| `rbook` 0.7.12 (Apache-2.0) | Rust EPUB 2/3 package, spine, navigation, metadata, lazy resources; no CSS layout engine. Read and write features can be separated. | Use its read-only configuration for EPUB structure. |
| `epub` 2.1.5 (GPL-3.0) | Rust EPUB 2/3 extraction and navigation; also needs an independent renderer. Resource reads allocate complete entries. | Viable alternative; do not add a second package parser. |
| `mupdf` 0.8.0 (AGPL) | Existing pinned safe Rust binding, EPUB/XHTML/CSS layout, embedded images/fonts, device-resolution rasterization. Layout and native objects stay on a worker. | Use for EPUB presentation, with its EPUB/HTML features enabled. |
| GPUI Kit 0.7.1 `TextView` | Native selectable rich text, tables, lists, links and code; its HTML reader explicitly does not implement publication CSS. Whole-document state measures every block initially. | Use for bounded Markdown render blocks, not EPUB layout. |
| `pulldown-cmark` 0.13.4 (MIT) | Pull events with source offsets, CommonMark and selected GFM extensions, HTML serializer. Avoids an editable full AST. | Use for Markdown parsing, headings and block boundaries. |
| `comrak` 0.56.0 (BSD-2-Clause) | Full AST, GFM extensions and formatter; default CLI/highlighter dependencies can be disabled. | Useful for editing/transformation; unnecessary for this read-only path. |
| `markdown` 1.0.0 (MIT) | Already transitive through Kit; full mdast conversion. | Keep Kit's dependency; avoid parsing the complete source through it for every reader update. |

EPUB extraction alone is not EPUB rendering. Feeding publication XHTML into
Kit's simplified HTML reader would lose CSS layout. Reusing MuPDF avoids that
unapproved fidelity compromise and introducing another document engine.

The binding's `page_count()` and outline resolution can paginate the whole
publication. Opening must start from package navigation and the requested
chapter; do not eagerly resolve every outline destination or generate thumbnails.
Its safe API still maps chapter destinations through absolute page numbers;
distant chapter jumps may lay out preceding chapters. Measure this explicitly.
Upstream chapter-local loading is a possible later optimization, not an excuse
to add first-party unsafe FFI or silently reduce rendering quality.

## Model and UI

| Surface | PDF | EPUB | Markdown |
|---|---|---|---|
| Content toolbar | Read/annotate and existing tools | Text size, chapter navigation, search, favorite | Text size, search, favorite |
| Left panel | Outline / thumbnails | Publication contents | Heading outline |
| Right panel | Information, annotations, search | Publication information / search | Document information / search |
| Status | Pages, zoom, fit, layout, rotation | Chapter and reading progress, reading navigation | Reading progress and text information; no page count |
| Position | Page plus PDF view settings | Chapter href plus an in-chapter position | Source block/offset and scroll position |

The titlebar, 38 px content headers, 32 px status bar, panel sizing, library,
categories, favorites and opening policies remain shared. Actions must be
guarded at dispatch as well as hidden visually. PDF sidecars and export remain
PDF-only. Session persistence must accept old snapshots and retain independent
locations/preferences across format switches.

## Performance and integrity

- Parse/read/render off the UI thread. Keep one running and at most one newest
  queued EPUB render, rejecting stale UI results by generation and source.
- Bound archive size, entry count, decompressed resources, markup nesting,
  document text, raster dimensions, and retained images. Reject unsafe package
  paths and encrypted entries; never extract a publication into its source folder.
- Parse Markdown once per source revision. Keep source offsets and complete
  top-level semantic blocks, resolving references before virtualizing. Lay out
  visible blocks with bounded lookahead rather than measuring the whole document.
- Preserve native text shaping and original EPUB colors. Changing UI themes
  must not invert publication images or page colors.
- Report current backend limits openly. Do not claim EPUB 3 multimedia,
  JavaScript, fixed-layout fidelity, MathML, or all Markdown extensions without
  corresponding implementation and fixtures.
- Measure Release open/render/parse p50 and p95, cold-process versus warm runs,
  fixture sizes/hashes, hardware, viewport/device scale, and process memory.
  Existing frame-time and memory goals remain acceptance gates.

## Primary references

- [rbook source and read-only features](https://github.com/DevinSterling/rbook)
- [epub-rs](https://github.com/danigm/epub-rs)
- [MuPDF Rust Document API](https://docs.rs/mupdf/0.8.0/mupdf/struct.Document.html)
- [GPUI Kit](https://github.com/longbridge/gpui-kit)
- [pulldown-cmark](https://github.com/raphlinus/pulldown-cmark)
- [comrak](https://github.com/kivikakk/comrak)

Implementation and measurements are tracked separately in
[work package 24](../plans/24-reflowable-reading.md) and [STATUS](../plans/STATUS.md).
