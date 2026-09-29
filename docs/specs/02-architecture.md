# Architecture and State

## Current workspace

```text
breeze-app -> breeze-core
           -> breeze-pdf -> pdf_oxide
```

- `breeze-core` owns navigation, zoom, and mode state without UI or PDF-engine types.
- `breeze-pdf` owns the read-only adapter, loading, rasterization, outlines, thumbnails, and worker queue.
- `breeze-app` owns the native window, document sessions, Home/Reader/Settings views, appearance preferences, command dispatch, and component tests.

The initial exploratory tool is no longer part of the workspace. Do not add empty storage/platform/editor crates ahead of concrete responsibilities. No dynamic plugins, generic event bus, script runtime, or multiple-engine fallback framework is needed.

## Format and session boundaries

- Share application-level identity, source, display name, library membership, and capability presentation where genuinely common.
- Keep PDF pages, glyph geometry, content streams, resource references, and outline destinations in the PDF backend.
- Each document session has its own reading position, zoom, and mode. Do not share page navigation globally across tabs.
- EPUB and Markdown locations should use native anchors, chapter/character offsets, or progress. Do not force reflowable formats into a PDF page-number model.
- Extract a common document interface when a second real backend reveals the shared requirements; avoid a speculative all-format `render_page` abstraction.
- Unsupported formats produce a visible error rather than being treated as renamed PDFs.

## Prototype rendering

A single background worker owns the current engine document and renderers. At most one job is waiting: a newer request replaces the pending job. A running synchronous render is not preempted. UI generations reject stale results after navigation or a document change.

The current output is a 144-DPI page capped at four megapixels plus up to four nearby thumbnails. The UI retains the current preview rather than a texture for every page. Thumbnail work currently completes before the combined result is presented; production scheduling should prioritize the visible page and deliver lower-priority thumbnails separately.

The engine returns premultiplied RGBA. The adapter swaps red/blue channels in the worker for GPUI's BGRA `RenderImage`. The display path avoids PNG encoding/decoding, and rendering the UI must not allocate a new page texture on every frame.

Zoom currently scales the fixed raster. Adaptive resolution, byte-budgeted LRU caches, continuous scrolling, and real tile rendering are not implemented. The output-pixel cap is not a complete memory limit.

## Geometry

A production page transform must include PDF user space, CropBox origin/intersection, UserUnit, page rotation, the vertical-axis flip, logical zoom, device scale, and viewport offset. Hit testing and annotation placement require the inverse transform.

Do not assume that `page_height - y` is sufficient for rotated/cropped pages. Verify each engine API's coordinate convention. The prototype displays the engine raster and does not claim to implement exact text-selection geometry.

## Target data model

These are future responsibilities, not implemented persistence contracts:

| Model | Required information |
|---|---|
| Library entry | Stable ID, source path, title, format, last access, favorite/category state, native reading location |
| Settings | Language, theme preference, independent palettes, update/window policies, independent file/position restoration switches |
| Annotation | Document fingerprint, page anchor, quads/path/bounds, style, author/time, stable ID |
| Edit transaction | Source fingerprint, object/stream reference, operator or usage identity, before/after data, affected resources, undo command |
| UI transient state | Hover, selected tool, drag draft, popup, focus, request generation |

Future persistence should use the OS application-data directory for preferences/library metadata and a document-adjacent `name.pdf.breeze.json` annotation sidecar. Report unwritable locations and ask for an explicit destination; do not silently redirect to an alternative store.

Source fingerprint mismatches stop automatic sidecar application. Original files are not overwritten by default. Use temporary-file replacement for durable writes, and avoid migration frameworks before there is a real supported historical schema.

## Future invalidation and undo

Cache keys need content revision, page, render/device scale, rotation, and color policy. Annotation drafts should be an independent drawing layer, not a reason to rerasterize the PDF page on every pointer event.

A structural edit invalidates only affected pages and resource users. Dragging updates a transient preview; pointer-up commits one undoable operation. Parsing, font subsetting, saving, and output validation stay off the UI thread.
