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

Reflowable formats keep PDF's control placement; only the format-specific
controls inside those slots differ. The navigation toggle lives at the left edge
of the panel header it controls and returns to the toolbar's left edge while the
panel is collapsed. Search, favorite and the document-panel toggle occupy the
toolbar's right edge. Zoom or text size sits at the right end of the status bar,
after the reading-progress area. Both panel headers label themselves with icons
and keep the panel toggle at their right edge.

The navigation panel itself is shared: an outline list with the PDF reader's row
height, per-depth indentation, active highlight, tooltip, nesting chevron, title
search and expand/collapse menu, and an empty state that names the missing match.
EPUB rows come from the publication navigation and fold by depth; Markdown rows
come from headings. Because a reflowable document has no fixed pages, only the
controls that need one are dropped.

| Surface | PDF | EPUB | Markdown |
|---|---|---|---|
| Content toolbar | Read/annotate and existing tools | Chapter navigation | Nothing format-specific |
| Left panel | Outline / thumbnails | Publication contents | Heading outline |
| Right panel | Information, annotations, search | Publication information / search | Document information / search |
| Status | Pages, zoom, fit, layout, rotation | Chapter and reading progress, text size | Reading progress, text size, reading width; no page-turn controls |
| Position | Page plus PDF view settings | Chapter href plus an in-chapter position | Source block/offset and scroll position |
| Reading width | Not applicable | Not applicable; the page is laid out at the viewport | Centred maximum measure by default, toggleable to the full window |

The titlebar, 34 px content headers, 32 px status bar, panel sizing, library,
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
- `mupdf-sys` compiles no CJK font — it prunes Droid and Source Han to stay under
  the registry's package limit — and the safe crate's system-font loader
  implements the CJK hook on Windows only. A CJK publication therefore needs the
  bundled runtime font providers (`bundled-fonts-droid`, `bundled-fonts-noto`);
  without them every Han character is a failed lookup, which both omits the glyph
  and costs about a millisecond, and a page of Chinese prose renders as a scatter
  of its Latin words and digits. Any release build must keep those providers
  enabled, and any new platform must be checked for the same gap.
- A Markdown image target is a URI: resolve percent-encoded names and drop a
  query or fragment before opening the file. A reference that still cannot be
  opened is a property of that one image — record it, show nothing in its place,
  and report the count in the document panel rather than in the status bar,
  which describes the document as a whole.
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

## EPUB chapter packaging

MuPDF lays out one chapter at a time, so the adapter repackages that chapter as a
temporary publication. That package must carry everything the chapter's markup
and stylesheets reference, transitively:

- A resolved reference path is percent-encoded while a package manifest is keyed
  by decoded hrefs, so lookup decodes first. Comparing the raw path against
  decoded keys silently dropped every stylesheet, image and font whose name
  contained a space or a non-ASCII character, which is what made a chapter
  render unstyled and sparse.
- A reference the manifest does not declare is still a real archive entry and is
  included, with its media type inferred from the extension.
- `url(...)` inside a `style` attribute, `@import`, and `@font-face` sources
  count as references; at-imported stylesheets are followed.
- The package carries the publication's language and, for right-to-left
  publications, `page-progression-direction="rtl"`.
- A declared-but-absent file is skipped rather than failing the chapter.
