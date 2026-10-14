# Architecture and State

## Current workspace

```text
breeze-app -> breeze-core
           -> breeze-pdf -> mupdf (reading/rendering)
                         -> lopdf (annotation-copy export)
           -> breeze-storage -> breeze-core
```

- `breeze-core` owns navigation, zoom, and mode state without UI or PDF-engine types.
- `breeze-pdf` owns the `mupdf-rs` adapter, reading, clipped rasterization, outlines, thumbnails, text geometry, and the worker queue, plus a separate protected `lopdf` annotation-copy exporter. PDF content editing is not part of the product.
- `breeze-storage` owns background state writes, process locking, atomic JSON, fingerprints, and optimistic sidecar revision checks.
- `breeze-app` owns the native window, document sessions, Home/Reader/Settings views, appearance preferences, command dispatch, and component tests.

`annotation_editing.rs` owns page-local text drafts, selection targets, and preview-to-history commits. Its geometry helpers hit-test the independent sidecar shapes after converting the pointer through each page's CropBox/rotation transform. Targets contain the source, fingerprint, history revision, item index, and expected annotation; asynchronous saves and UI callbacks cannot apply stale edits to a replacement item. The history revision is transient and does not change the sidecar schema. Text Input entities are created only for active page editing and respect native IME composition. Annotation previews are independent overlays and do not invalidate PDF raster tiles.

The initial exploratory tool is no longer part of the workspace. Do not add empty storage/platform crates ahead of concrete responsibilities. No dynamic plugins, generic event bus, script runtime, or multiple-engine fallback framework is needed.

## Format and session boundaries

- Share application-level identity, source, display name, library membership, and capability presentation where genuinely common.
- Keep PDF pages, glyph geometry, content streams, resource references, and outline destinations in the PDF backend.
- Each document session has its own reading position, zoom, and mode. Do not share page navigation globally across tabs.
- EPUB and Markdown locations should use native anchors, chapter/character offsets, or progress. Do not force reflowable formats into a PDF page-number model.
- Extract a common document interface when a second real backend reveals the shared requirements; avoid a speculative all-format `render_page` abstraction.
- Unsupported formats produce a visible error rather than being treated as renamed PDFs.

## Historical PDF Oxide prototype rendering

The current runtime still uses PDF Oxide and returns whole-page pixmaps. The
details below describe the earlier PDF Oxide tile prototype; they do not
describe the running adapter.

A single background worker owns the current engine document and renderers. At most one job is waiting: a newer request replaces the pending job. A running synchronous render is not preempted. UI generations reject stale results after navigation or a document change.

The historical PDF Oxide prototype requested device-pixel regions at native surface size. Its 508-pixel tile interiors and two-pixel gutters fit GPUI's 512-pixel atlas cell. These cache and scheduler details do not describe the running whole-page renderer or the MuPDF target.

The previous prototype used a 32 MiB worker result cache, 8 MiB/eight metadata results, 56 MiB of UI tiles, and 8 MiB/64 thumbnails. It prioritized visible work over bounded lookahead and explicitly retired GPU atlas images. Those are historical budgets, not bounds on current process or target MuPDF memory.

The engine returns premultiplied RGBA. The adapter swaps red/blue channels in the worker for GPUI's BGRA `RenderImage`. The display path avoids PNG encoding/decoding, and rendering the UI must not allocate a new page texture on every frame.

The MuPDF target derives requests from the visible content mask, coalesces demand, discards stale results, and retains display lists and pixel regions within separate budgets. Effective device density, scroll anchors, page rotation, and crop transforms remain explicit. These behaviors are not implemented in the running renderer; see [work package 22](../plans/22-mupdf-migration.md).

## MuPDF target rendering

Work package 22 replaces the PDF Oxide prototype with `mupdf-rs`. The target
adapter records page display lists on the PDF worker, replays only visible
device-space clips into bounded pixmaps, and returns owned pixel regions to the
UI. Display-list and MuPDF store limits, tile keys, cancellation, device origin,
rotation, CropBox/UserUnit mapping, RGBA/BGRA and alpha representation must be
measured and validated before their budgets or behavior are described as
implemented. The UI paints cached regions at their corresponding page bounds;
sidecar annotations remain a separate overlay.

## Geometry

A production page transform must include PDF user space, CropBox origin/intersection, UserUnit, page rotation, the vertical-axis flip, logical zoom, device scale, and viewport offset. Hit testing and annotation placement require the inverse transform.

Do not assume that `page_height - y` is sufficient for rotated/cropped pages. Verify each engine API's coordinate convention. The current implementation maps user-space glyph bounding boxes through CropBox/quarter-turn rotation for rectangular selection and markup. It is not an exact general glyph-outline/complex-script selection engine; rotated text runs are excluded from text selection.

## Target data model

The current typed JSON schema implements library/settings and annotation fields below:

| Model | Required information |
|---|---|
| Library entry | Stable ID, source path, title, format, last access, favorite/category state, native reading location |
| Settings | Language, Follow System or a GPUI Kit registry theme name, update/window policies, independent file/position restoration switches |
| Annotation | Document fingerprint, page anchor, quads/path/bounds, style, author/time, stable ID |
| UI transient state | Hover, selected tool, drag draft, popup, focus, request generation |

Implemented persistence uses the OS local application-data directory for preferences/library metadata (or `BREEZE_DATA_DIR`) and a document-adjacent `name.pdf.breeze.json` annotation sidecar. Report unwritable locations and ask for an explicit destination; do not silently redirect to an alternative store.

Source fingerprint mismatches stop automatic sidecar application. Original files are not overwritten by default. Use temporary-file replacement for durable writes, and avoid migration frameworks before there is a real supported historical schema.

## Future invalidation

Cache keys need content revision, page, render/device scale, rotation, and color policy. Annotation drafts should be an independent drawing layer, not a reason to rerasterize the PDF page on every pointer event.

## Current safety limits

PDF input is capped at 256 MiB and 20,000 pages. State JSON is capped at 4 MiB, sidecars at 16 MiB, annotation count at 5,000, combined annotation geometry at 100,000 points/quads, current-page glyph output at 50,000, and search results at 300. Undo retains at most 50 snapshots within an 8 MiB estimated geometry/text budget. These are application limits, not a hard limit on engine decoding memory.

## Annotation export and opening policies

Export captures an immutable source fingerprint and annotation list, writes a temporary new PDF with vector normal appearances, verifies source-object signatures and annotation structure, rechecks the source, and publishes without clobbering a destination. It is a single background job with separate UI progress/receipt state; it does not modify sidecar acknowledgements. Existing annotation arrays are copied before appending so shared arrays remain intact.

Opening policies are applied once to an accepted load generation. Fit Document accounts for the current window origin and waits for a native bounds event when restoring a maximized window. It does not poll or change page zoom. Foreground page-load tasks are owned and cancelled on replacement/Home/close instead of remaining detached after their view's lifetime.

## User categories and state version 6

`breeze-core::library::Category` contains a stable numeric ID, user name, and a semantic color from eight choices. Missing legacy colors default to blue; unknown colors reject loading. An empty membership array means Unassigned; zero is not a valid category ID. Names are trimmed, case-insensitively unique, and limited to 40 characters; at most 100 named categories are stored. Deletion removes only the deleted membership and does not reuse an ID. State version 6 stores a bounded unique array of category IDs per document. Versions 1–5 migrate their single category field to an empty or one-member array; the legacy field is read but no longer written. State version 2 introduced the category collection, next ID, and each document's layout. A narrow version-1 transition preserves only legacy named categories actually referenced by saved documents; fresh states do not seed them. Invalid category references/IDs/names reject state loading rather than being silently rewritten.

## Built-in themes within the current state schema

The current state schema (version 8) stores Follow System or a named GPUI Kit theme, replacing Breeze-owned light/dark palettes. Before creating the application view or restoring preferences, Breeze registers 36 compiled-in official gallery themes alongside Kit's two defaults; source provenance and licensing are in `assets/themes/README.md`. A grouped searchable Select and the titlebar cycle share the same registry-derived order. Every legacy version still loads: explicit Light/Dark maps to `Default Light` / `Default Dark`, System remains System, and retired palette fields are read but omitted from subsequent snapshots. Unknown named themes reject state loading rather than silently applying a different theme. Library, category, reading, and annotation references retain their existing semantics.
