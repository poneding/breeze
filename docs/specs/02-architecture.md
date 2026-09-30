# Architecture and State

## Current workspace

```text
breeze-app -> breeze-core
           -> breeze-pdf -> pdf_oxide (reading)
                         -> lopdf (annotation-copy export)
           -> breeze-storage -> breeze-core
```

- `breeze-core` owns navigation, zoom, and mode state without UI or PDF-engine types.
- `breeze-pdf` owns reading, rasterization, outlines, thumbnails, and the worker queue, plus a separate protected annotation-copy exporter. Source-content editing is not exposed.
- `breeze-storage` owns background state writes, process locking, atomic JSON, fingerprints, and optimistic sidecar revision checks.
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

The reader requests page metadata separately from pixels. Visible page surfaces request direct device-pixel regions at their actual logical size multiplied by the window scale factor. Tile interiors are 508 x 508 pixels, with up to two pixels of gutter on each edge, so the uploaded texture fits a 512-pixel atlas cell. No full-page high-resolution raster is allocated for these requests. A pinned local patch to PDF Oxide 0.3.78 translates the original page transform into the tile; source page dictionaries and streams remain unchanged. The existing whole-page API remains available for validation, not the application's page display.

Thumbnails remain independent 24-DPI requests capped at 120,000 pixels and do not extract glyphs. The worker retains one running/one replaceable pending job and a 32 MiB accounted result cache; tiles are not duplicated in that cache. The UI retains up to 8 MiB/eight metadata results, 56 MiB of page tiles, and 8 MiB/64 thumbnails. Visible work takes priority over thumbnails. Cache identities include page, exact device scale, and region, with document fingerprint/generation checks and source-change invalidation. Obsolete zoom levels are released before new ones are allocated. Image eviction and source changes explicitly remove GPU atlas entries as well as CPU references; dropping an Arc alone does not release GPUI's atlas allocation. These accounting limits exclude engine internals, transient frame references, GPU atlas fragmentation, and window swapchain surfaces, and are not an RSS bound.

The engine returns premultiplied RGBA. The adapter swaps red/blue channels in the worker for GPUI's BGRA `RenderImage`. The display path avoids PNG encoding/decoding, and rendering the UI must not allocate a new page texture on every frame.

Zoom and display-density changes request new pixels. Each page canvas derives its visible rectangle from the native content mask during prepaint; layout measurement cannot enqueue work. Requests are coalesced per frame, stale viewport results are discarded, and cached tiles are painted without re-rendering. Missing tiles remain blank while being prepared rather than stretching a lower-resolution page. Reading layouts construct visible page surfaces, while outline/thumbnail navigation remains virtualized. True tile allocation is implemented; retained operator/display-list preparation and broad engine fidelity/platform acceptance remain future work.

## Geometry

A production page transform must include PDF user space, CropBox origin/intersection, UserUnit, page rotation, the vertical-axis flip, logical zoom, device scale, and viewport offset. Hit testing and annotation placement require the inverse transform.

Do not assume that `page_height - y` is sufficient for rotated/cropped pages. Verify each engine API's coordinate convention. The current implementation maps user-space glyph bounding boxes through CropBox/quarter-turn rotation for rectangular selection and markup. It is not an exact general glyph-outline/complex-script selection engine; rotated text runs are excluded from text selection.

## Target data model

The current typed JSON schema implements library/settings and annotation fields below. The edit-transaction row remains a future responsibility:

| Model | Required information |
|---|---|
| Library entry | Stable ID, source path, title, format, last access, favorite/category state, native reading location |
| Settings | Language, theme preference, independent palettes, update/window policies, independent file/position restoration switches |
| Annotation | Document fingerprint, page anchor, quads/path/bounds, style, author/time, stable ID |
| Edit transaction | Source fingerprint, object/stream reference, operator or usage identity, before/after data, affected resources, undo command |
| UI transient state | Hover, selected tool, drag draft, popup, focus, request generation |

Implemented persistence uses the OS local application-data directory for preferences/library metadata (or `BREEZE_DATA_DIR`) and a document-adjacent `name.pdf.breeze.json` annotation sidecar. Report unwritable locations and ask for an explicit destination; do not silently redirect to an alternative store.

Source fingerprint mismatches stop automatic sidecar application. Original files are not overwritten by default. Use temporary-file replacement for durable writes, and avoid migration frameworks before there is a real supported historical schema.

## Future invalidation and undo

Cache keys need content revision, page, render/device scale, rotation, and color policy. Annotation drafts should be an independent drawing layer, not a reason to rerasterize the PDF page on every pointer event.

A structural edit invalidates only affected pages and resource users. Dragging updates a transient preview; pointer-up commits one undoable operation. Parsing, font subsetting, saving, and output validation stay off the UI thread.

## Current safety limits

PDF input is capped at 256 MiB and 20,000 pages. State JSON is capped at 4 MiB, sidecars at 16 MiB, annotation count at 5,000, combined annotation geometry at 100,000 points/quads, current-page glyph output at 50,000, and search results at 300. Undo retains at most 50 snapshots within an 8 MiB estimated geometry/text budget. These are application limits, not a hard limit on engine decoding memory.

## Annotation export and opening policies

Export captures an immutable source fingerprint and annotation list, writes a temporary new PDF with vector normal appearances, verifies source-object signatures and annotation structure, rechecks the source, and publishes without clobbering a destination. It is a single background job with separate UI progress/receipt state; it does not modify sidecar acknowledgements. Existing annotation arrays are copied before appending so shared arrays remain intact.

Opening policies are applied once to an accepted load generation. Fit Document accounts for the current window origin and waits for a native bounds event when restoring a maximized window. It does not poll or change page zoom. Foreground page-load tasks are owned and cancelled on replacement/Home/close instead of remaining detached after their view's lifetime.

## User categories and state version 6

`breeze-core::library::Category` contains a stable numeric ID and user name. Zero means Unassigned. Names are trimmed, case-insensitively unique, and limited to 40 characters; at most 100 named categories are stored. Deletion removes only the deleted membership and does not reuse an ID. State version 6 stores a bounded unique array of category IDs per document. Versions 1–5 migrate their single category field to an empty or one-member array; the legacy field is read but no longer written. State version 2 introduced the category collection, next ID, and each document's layout. A narrow version-1 transition preserves only legacy named categories actually referenced by saved documents; fresh states do not seed them. Invalid category references/IDs/names reject state loading rather than being silently rewritten.
