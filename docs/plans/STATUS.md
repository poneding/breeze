## October 16 Markdown semantic pipeline and virtualized layout (P3)

[Work package 25](25-markdown-live-editing.md) phase P3 is complete for the
shipped behaviour; measured performance is recorded below with one open
limitation at the 16 MiB cap.

**Semantics.** `breeze-reflow::markdown::semantic` parses one bounded snapshot
with the reader's own option set (`semantic::options`, now shared by
`Document::parse`) and returns per-line block kinds plus styled inline spans
with exact source geometry. Because the parser is the authority, the editor now
hides and reveals reference-style links, composes nested emphasis instead of
emitting overlapping runs, shows decoded character entities, hides setext
underlines, and classifies footnotes, tables and fences correctly.
`Projection::build_semantic` consumes that layer; `Projection::build_with_literal`
remains the line-local fallback used before the refined projection arrives.

**Bounded work.** `breeze-reflow::markdown::pipeline` admits one active parse and
one newest pending snapshot, drops requests a running job already covers, and
rejects results whose session or revision is no longer current. The editor
paints a provisional projection immediately, submits the snapshot to a worker,
and swaps in the refined projection only when it still matches the session,
revision, view mode and worker epoch. A caret move reuses the cached semantic
layer instead of parsing again.

**Virtualized layout.** `MarkdownEditor`'s layout is a list of units with
cumulative tops and estimated heights. Only units intersecting the viewport
plus one viewport of lookahead are shaped; measurements further away are evicted
against an 8 MiB shaped-line budget and fall back to their estimate. A source
line longer than 4096 display bytes is split into windows at character
boundaries, so a megabyte on one line becomes a few hundred units instead of one
shape call. Wrapping is per unit, so a pathological single-line document can
wrap differently at a window seam than a full layout would.

**Measured cost** (Release, `cargo test --release -p breeze-reflow --test
markdown_performance -- --ignored --nocapture`, 20 samples, this checkout's
machine — Apple M1 Pro, macOS 15, the corpus generator in that test):

| Source | Lines | Parse p50/p95 | Semantic build p50/p95 | Provisional p50/p95 |
|---|---|---|---|---|
| 10 KiB | 475 | 0.1/0.1 ms | 0.2/0.3 ms | 0.6/0.8 ms |
| 100 KiB | 4,655 | 1.0/1.2 ms | 0.4/0.5 ms | 2.6/3.2 ms |
| 1 MiB | 47,025 | 8.2/9.3 ms | 5.4/6.1 ms | 29.5/30.7 ms |
| 16 MiB | 742,064 | 135/142 ms | 441/495 ms | 438/444 ms |

Materializing a 1 MiB rope costs 0.18 ms, so the snapshot handoff is not the
constraint. Parsing and the semantic build run on a worker in every case. The
provisional rebuild runs on the UI thread and meets the 50 ms input budget
through 1 MiB; **at the 16 MiB cap it does not**, and a keystroke would pause
about 0.44 s. Options recorded, not yet adopted: reduce the input cap, accept a
degraded provisional (show the typed text without re-styling the rest of the
document), or complete an incremental projection patch (an earlier attempt was
removed rather than shipped because it did not match a full rebuild in every
generated case). Until one of those is measured and approved, the 16 MiB case is
a known limitation rather than a met budget.

Validation in an isolated checkout containing only this change: `cargo test
--workspace --offline` (193 app, 19 core, 39 PDF with three ignored, 6 reflow
unit, 7 EPUB, 16 Markdown editing, 8 Markdown projection, 12 Markdown semantic
unit, 10 Markdown semantic integration, 7 storage; exit 0) and `cargo clippy
--workspace --all-targets --offline -- -D warnings` (exit 0). New app component
tests assert that a megabyte on one line becomes many units with only a fraction
shaped, and that 2000 paragraphs measure under 200 units. The 16 MiB stress case
was measured, not natively exercised in the editor.

## October 16 text selection follows reading order

Selecting text used to be rectangular: the gesture's bounding box decided which
glyph centers were inside it, so a drag across two lines picked up a box of
characters instead of the run between the two points, and the highlight was
painted from each glyph's own box. Those boxes do not agree across fonts — a
Chinese font's boxes are taller than its leading, and two lines of justified CJK
prose report different heights for the same line pitch — so a translucent
highlight showed a gap or an overlap between rows, worst on Chinese text.

`breeze-pdf` now owns page text as a structure rather than a glyph list.
`PageText` keeps the characters in reading order and groups the engine's lines
into visual rows: lines on one baseline join into one row, and each row records
the user-space axes it advances and descends along, from the character quads, so
a `/Rotate`d page reads along its own axis. A selection is the run of that order
between the carets nearest the two gesture points; a point beyond a row's end
lands on that row's start or end, which is what makes dragging past a line end
or into a paragraph gap select whole lines. Copy writes one line per row, with a
space where the engine continued a row across a wide gap.

`PageText::highlight_rects` returns one rectangle per selected row and settles
the shared edges: the line gap between two rows is split at its middle, which
closes loose leading and trims overlapping font boxes alike, while a gap
noticeably wider than its neighbours is a paragraph break and stays open. A
truncated row falls back to the leading left open by its neighbours, never
trimming its own glyph boxes. Underline and strikeout keep hugging the glyph
boxes (`line_rects`), and annotation markup now uses the same range as the
selection, so a highlight drawn over two lines is the same shape as the copy.

The reader draws the range live while dragging, between the page raster and the
annotation overlay, and the pointer release fills the status message. Verified on
three real Chinese fixtures — CoreText with Songti SC, CoreText with PingFang SC
on tight leading, and MuPDF's own HTML layout with Source Han Serif — as well as
the bundled English sample: seven rows, every intra-paragraph pair sharing one
edge exactly (no gap and no overlap), the paragraph break open by 10 pt, and
copy returning the paragraphs with one newline per row.

Validation: `cargo fmt --all -- --check`, the workspace suite (194 app tests
including the reading-order selection test, 39 PDF with three ignored, and the
remaining suites) and `cargo clippy --workspace --all-targets --locked -- -D
warnings` pass. Eight new `breeze-pdf` unit tests cover reading-order ranges,
shared edges on overlapping font boxes, loose leading versus a paragraph break,
pointer snapping at line ends and in paragraph gaps, column gaps, rotated page
axes, wide-gap row joining, and steep or invalid runs. Native visual inspection
was not performed: this session's screen captures contain only the desktop
wallpaper because screen-recording permission is denied (ScreenCaptureKit
reports TCC -3801), so the app was only confirmed to launch against an isolated
data directory. Windows and Linux native checks remain open.

## October 16 Markdown live editing in the reader content area (P2)

[Work package 25](25-markdown-live-editing.md) phase P2 now runs inside the
reader: a Markdown tab shows a pencil button in its existing 34 px toolbar, and
pressing it swaps the read-only text in the same content region for the native
editing surface. The title bar and document tabs, navigation panel, toolbar and
32 px status bar stay the reader's own. A second, separate editor window was an
earlier development harness and has been removed.

The draft session is created on first entry and kept in the tab while it stays
open, so leaving edit mode, reading, or switching tabs does not discard it.
Closing a tab with unsaved Markdown requires a second confirmation and states
that saving arrives in a later phase. Live and source projections share one
source buffer, selection and undo history; switching between them changes no
bytes. The status bar reports the projection, undo depth, modified state and
whether a native composition is in progress.

`breeze-reflow` gained the pure projection layer used by the surface:
`Projection::{source,live,build_with_literal}`, `ProjectionMode`, `BlockKind`,
`DisplayLine`, `DisplayRun` and `RunStyle`. It hides and reveals ATX heading,
quote, list, fence and non-nested inline markers by mapping source byte ranges
onto display ranges, and keeps every unsupported construct editable as literal
source. `EditSession` gained `empty`, `begin_composition`, `apply_composition`,
`commit_composition`, `cancel_composition` and `composition_range`, so repeated
IME marked-text updates preview without entering history and commit as one
undoable entry; a cancelled composition restores the original bytes and
selection. `SourceBuffer` gained `Default`.

`breeze-app::markdown_editor` is the native surface: a GPUI `Element` that shapes
and wraps the projection, paints selection and caret, hit-tests pointers,
scrolls, and implements `EntityInputHandler` for text, marked text, range
replacement, caret bounds and UTF-16 conversion. `MarkdownEditor` keys its layout
cache by revision, projection and width; the initial cursor sits at offset zero,
so the heading marker is revealed until the caret leaves it.

Validation in an isolated checkout of this change, excluding unrelated
in-progress `breeze-pdf` work: `cargo test --workspace --offline` (193 app, 19
core, 31 PDF with three ignored, 6 reflow unit, 7 EPUB, 16 Markdown editing, 8
Markdown projection, 7 storage; exit 0) and `cargo clippy --workspace
--all-targets --offline -- -D warnings` (exit 0). Four new app component tests
cover live/source switching, composition commit/cancel, grapheme and CJK caret
movement with bounds/hit-test inversion, and typing plus undo. Two more cover
click clamping outside the text and the reader-content-area integration. Native
macOS inspection of an isolated `BREEZE_DATA_DIR` confirmed the rendered surface
inside the reader chrome with an outline panel and the shared status bar.

Recorded limits for this phase: the projection is rebuilt on the UI thread and
materializes the source, the whole document is laid out without virtualization,
there is no background parse, no save, and no table or image editing. Those are
P3–P6 work. Windows and Linux native input checks remain open.

## October 16 Markdown editor source model (P1)

[Work package 25](25-markdown-live-editing.md) phase P1 is complete; the editor
surface, semantic pipeline, save workflow and application integration are still
unstarted. The new `breeze-reflow` Markdown model keeps one valid-UTF-8 source
buffer with exact bytes, a monotonic revision, a content checkpoint distinct
from that revision, source-based selection with affinity and a desired visual x,
atomic multi-range replacement, inverse-delta history and explicit source/
history/entry budgets.

Ropey `2.0.0-beta.1` (already transitive) is now an explicit pinned dependency
with the UTF-16 and LF/CR line metrics enabled, and `unicode-segmentation
1.13.3` is pinned for grapheme movement. `Cargo.lock` changed only by adding
those two edges to `breeze-reflow`; no GPUI or other dependency moved.

New public API in `breeze-reflow::markdown`: `SourceBuffer`, `EditSession`,
`EditLimits`, `EditKind`, `Replacement`, `Snapshot`, `Position`, `Selection`,
`Affinity`. The existing read-only `Document::open/parse` path is unchanged.

Validation in this working tree: `cargo test --workspace --offline` (187 app,
19 core, 31 PDF with three ignored, 6 reflow unit, 7 EPUB, 13 new Markdown
editing, 7 storage; exit 0), `cargo clippy --workspace --all-targets --offline
-- -D warnings` (exit 0), and `rustfmt --edition 2024 --check` on every new or
changed Markdown file. `cargo fmt --all -- --check` still reports unrelated
in-progress formatting differences in `crates/breeze-pdf/src/lib.rs` and
`crates/breeze-pdf/src/text.rs`, which this phase does not touch.

The 13 Markdown tests cover byte preservation for BOM, CRLF/LF/mixed endings,
front matter, hard-break spaces, final-newline and unknown syntax; invalid UTF-8
and interior-character offsets; ordering, overlap and stale-revision refusal;
undo/redo byte and selection restoration; typing coalescing within 500 ms and
its break conditions; undo-to-saved-checkpoint dirtiness, stale acknowledgements
and worker-proven equality; refusal before mutation at the undo and source
budgets; entry-limit eviction; UTF-16 and grapheme round trips for surrogate
pairs, combining marks and ZWJ sequences; surrounding-line-endings selection;
and a 120-edit generated sequence that undoes and redoes back to the original
bytes.

## October 16 grid cover fallback icon size

Library grid covers without a thumbnail showed the list view's 16-pixel format
mark inside a 120x156 card, so the file type was hard to read. The grid fallback
now renders that mark at 48 logical pixels; real first-page covers and the list
row's compact icon are unchanged. A component test measures both fallback
bounds, and native macOS inspection of an isolated library confirmed the grid
and list sizes.

## October 16 annotation selection, shape resizing, and eraser removal

The Select annotation button uses Kit's filled primary variant while active and
exposes its pressed state to accessibility. Selected page annotations use a blue
outline with a white outer edge instead of the application theme's primary
color, which could disappear against a white PDF page. Selected text notes also
use the document-independent blue border.

Ellipses, rectangles, and area highlights expose eight handles with 16-logical-
pixel hit targets and 8-pixel visible squares. Side handles change one dimension;
corners change both. Resizing follows the displayed axes through page and reader
rotation, preserves the opposite edge, clamps to the page and a 0.5-point minimum,
and commits one undoable history change on release. The preview remains subject
to the existing source/history identity guards. A pointer that returns to its
starting position restores the original preview for both moving and resizing.

The eraser button, explanation, settings branch, and ink-erasure event handler
are removed. The session-only tool indices have been compacted; note, shape,
line, wave, and arrow dispatch and their settings follow the new indices.
Selection/Delete and the annotation list still remove annotations explicitly.
The persisted annotation kinds and sidecar format have not changed.

Validation used an isolated copy of the committed baseline plus these changes,
excluding unrelated in-progress PDF text-layer and Markdown work. Formatting,
`cargo test --workspace --locked` (186 app tests and all remaining workspace
suites), and `cargo clippy --workspace --all-targets --locked -- -D warnings`
pass. Components exercise all three resizable shapes, a one-step undo/redo,
constant handle hit size, and the tool palette after eraser removal. Geometry
tests cover all eight handles, sixteen page/view rotation combinations, minimum
sizes, and page-edge constraints. `cargo run --locked -p breeze-app` started with
isolated data under `target/`. Native macOS inspection confirmed the filled
Select button, clear selection frame and handles on white pages, and an ellipse
side-handle drag that changes width while keeping height and the opposite edge.
The inspected window also showed resized rectangle and area-highlight bounds.
Windows/Linux native interaction remains unverified.

## October 16 smooth annotation strokes

Native annotation outlines now use round caps and joins with tessellation
precision tied to device scale. Straight lines, pen strokes, arrows, and text
markup no longer have flat-cut endpoints. Ellipses use four cubic curves instead
of a fixed 48-edge polygon.

Free wavy lines use tangent-continuous cubic quarter waves. Their wavelength
scales with line width so thick waves keep open troughs, short gestures reduce
amplitude, and both ends meet the gesture anchors. The core owns this geometry;
the native overlay, selection hit testing, and exported PDF appearances consume
it. The PDF appearance uses cubic `c` operators with round caps/joins, while its
editable InkList samples the same curves. Stored gesture points and the sidecar
format are unchanged. Generated waves are bounded to 1,024 cubic segments and
8,193 sampled points.

A maximum-length 20,000-point pen stroke with round joins exceeded GPUI's u16
tessellation index limit and disappeared. Open polylines now tessellate in bounded
chunks with shared endpoints, then combine into one painted path so transparency
is applied once. Regression coverage includes both 2 pt and 100 pt strokes at
this limit, round cap coverage and absence of miter spikes, wave anchor/tangent
continuity and amplitude bounds, hit testing at the visible crest, sampled-path
error below 1% of line width, and exported curve/InkList preservation.

Validation used an isolated source copy of the committed baseline plus this
change, excluding unrelated in-progress PDF text-layer work that did not compile
in the working checkout. `cargo fmt --all -- --check`, `cargo test --workspace
--locked` (181 app, 19 core, 31 PDF unit tests with three ignored probes, and all
remaining suites), and `cargo clippy --workspace --all-targets --locked -- -D
warnings` pass. `cargo run --locked -p breeze-app` started with an isolated
`BREEZE_DATA_DIR` under `target/`. Native macOS inspection of an authored fixture
confirmed round endpoints and smooth 1/2/8/16 pt waves, short and diagonal waves,
and the ellipse/pen outlines. Windows/Linux native checks remain open.

## October 16 vertical arrow scrolling

Up Arrow and Down Arrow scroll Markdown and continuous PDF reading by 48 logical
pixels per key event, using the existing native list handles. Both continuous
single-page and continuous-spread PDF layouts are supported. PDF page loading
and navigation still follow the virtual list's visible range; Markdown records
its bounded scroll position and reports completion at the document end.

The bindings exclude Input contexts and use normal system key-repeat events.
They do not turn pages in EPUB or non-continuous PDF layouts. The shortcut
reference lists their supported formats in all three interface languages.

Validation used an isolated checkout and build directory to exclude concurrent
PDF adapter work: formatting, the full workspace suite (180 app tests), and
Clippy with warnings denied pass. Component coverage checks pixel steps, key
repeat, both continuous PDF layouts, non-continuous exclusion, document bounds,
Markdown progress, and focused inputs. Native macOS checks confirmed Markdown
content/progress movement and PDF scrolling in both directions with page-number
updates across a page boundary. The UI tool's held-key simulation emitted only
one event, so physical held-key repetition remains a manual check; repeated
presses and repeat-event dispatch were verified. Windows/Linux native checks
remain open.

## October 16 horizontal arrow page shortcuts

Left Arrow and Right Arrow invoke the existing Previous Page and Next Page
commands. PageUp and PageDown remain available. The bindings use the same
`Breeze && !Input` context, preserving caret movement inside text inputs and
the existing layout-aware page steps and document boundaries.

Validation on an isolated checkout containing this change, excluding unrelated
uncommitted work: `cargo fmt --all -- --check`, `cargo test --workspace --locked`
(including 178 app tests), and `cargo clippy --workspace --all-targets --locked
-- -D warnings` pass. New native-component tests cover all four PDF layouts,
first/last-page bounds, the original page-key aliases, and caret movement in the
page-number and search inputs. A macOS run with an isolated `BREEZE_DATA_DIR`
confirmed Right/Left move between sample pages 1 and 2 and preserve input
editing. Windows and Linux native checks remain open.

## October 16 macOS traffic-light fullscreen

The October 15 change coupled AppKit's green traffic light to immersive reading
by adopting the window's fullscreen state into `Breeze::full_screen`. That was
wrong: the green button is the macOS window control, while content-only
fullscreen belongs to the reader's status-bar control.

The green traffic light now keeps AppKit's native fullscreen behavior and enters
a new macOS Space. Breeze does not adopt that window state into
`Breeze::full_screen`, so the Breeze title bar, navigation, reader toolbar, and
status bar remain visible. AppKit does not expose its hidden fullscreen title
bar to the content view, so keeping the document tabs in Breeze's own title row
is the public-API-safe way to keep tab switching reachable. The custom title
bar's double-click handler remains suppressed while the window is natively
fullscreen, avoiding AppKit's zoom behavior inside the fullscreen Space.

The status-bar Fullscreen and F11 controls continue to request native window
fullscreen and then render only the document. When the window is already in
native fullscreen, they toggle the immersive chrome layer without creating a
second Space.

Automated coverage keeps a native fullscreen transition outside the Fullscreen
command from hiding reader chrome, and continues to cover entry and exit through
the status-bar control.

Validation: `cargo fmt --all -- --check`, focused fullscreen component tests,
and a native macOS check confirm that the green traffic light enters the
fullscreen Space at 1512×945; the title bar is absent until the pointer reaches
the top edge, where the traffic lights and document tabs reappear as an overlay.
The navigation, toolbar, and status bar remain in place throughout.

## October 15 reader toolbar matches the title bar at 34 px

The reader toolbar and the navigation and document-panel headers that align with
it were 38 px, 4 px taller than the title bar above them. `style::TOOLBAR_HEIGHT`
now holds 34 px, matching the title bar, and the three headers read that one
constant; the status bar keeps its own 32 px. The panel search row that replaces
a header keeps sharing the header height.

The reading viewport and the fit-to-document window policy subtract the same
constant, so page geometry follows the shorter toolbar instead of keeping a 4 px
gap at the bottom of a fitted page. Component tests assert the toolbar and both
headers at 34 px while the status bar stays 32 px, in the PDF and reflow readers.

Validation: `cargo fmt --all -- --check`, `cargo test --workspace --locked`
(173 app, 16 core, 30 PDF with three ignored, and the remaining suites) and
`cargo clippy --workspace --all-targets --locked -- -D warnings` pass. A native
run against an isolated `BREEZE_DATA_DIR` confirmed the 34 px content toolbar in
both the PDF and Markdown readers, with the status bar unchanged at 32 px.
Windows and Linux native checks remain open.

## October 15 macOS traffic-light fullscreen

Superseded by the October 16 behavior above: the green traffic light remains a
window-level fullscreen control and does not enter document-only immersive
reading.

Reported on macOS: entering fullscreen with the green traffic light removed the
only way back out with the pointer, because the traffic lights never reappeared
when the pointer moved to the top of the screen; F11 was needed. A second
double-click on the title bar then shrank the window, leaving a strip of the
desktop above and below the page.

Two causes, both measured on this checkout:

- The reader followed only its own Fullscreen command. A window fullscreened by
  AppKit left `Breeze::full_screen` false, so the reader kept drawing the title
  bar inside a fullscreen window instead of the document-only immersive layout.
- With that title bar present, its double click reached
  `Window::titlebar_double_click`, which reads `AppleActionOnDoubleClick` and
  calls AppKit's `_zoomFill:` (`Fill` on this machine) or `zoom:`. A probe built
  against AppKit directly reproduced the shrink: a 1512×945 fullscreen window
  became 1512×891 at y=53 inside a 1512×982 Space, which is the desktop strip the
  user saw. A real fullscreen window in a stock macOS application does not have a
  title bar to double click.

The reader now adopts the window's own fullscreen state from its existing bounds
observer, so the green traffic light and the status-bar control leave the same
document-only layout and the same exits (Escape, F11, and the green light). The
title bar ignores a double click while the window is fullscreen, which covers
Home, where chrome stays visible.

Verification: `cargo fmt --all -- --check`, the full workspace suite (163 app,
14 core, 30 PDF plus 3 ignored probes, 8 PDF integration, 5 storage, plus the
single-test binaries) and `cargo clippy --workspace --all-targets --locked -- -D
warnings` pass on a tree containing only this change. The new component test
`native_fullscreen_adopts_document_only_reading` fails without the state
adoption. The title-bar guard has no automated regression test: the pinned test
platform's `titlebar_double_click` is a no-op, so the shrunken-window effect is
only observable natively.

Native checks on the fixed build:

- Clicking the green traffic light now leaves the document-only layout: the
  captured top strip holds page pixels only, with no application title bar or
  toolbar row, where the pre-fix build still showed both.
- With the pointer driven to the top edge of the built-in display, the traffic
  lights appeared at the system position, matching a stock application on the
  same display in the same session.

The auto-hide reveal is not entirely under application control. On the external
Mi Monitor (2048×1152 points) neither the fixed build nor a fullscreen TextEdit
window revealed its title bar: both kept the window-server titlebar overlay at
`y = -44` with `alpha = 0` while the pointer pressed against the top edge. The
same check on the built-in display revealed for Breeze and for Finder. Treat a
missing reveal on a given display as a macOS/display condition, not proof of a
reader defect, and check a stock application there before investigating Breeze.

The double-click shrink has no native regression check yet: the guarded path on
Home needs the same traffic-light/layout observation. Do not treat it as
visually accepted until a fullscreen Home window has been double-clicked.

## MuPDF Migration — October 14, 2026

This section is authoritative for the current checkout. Earlier October 14
migration and strategy entries below preserve decisions and measurements as
they were recorded at that point; they are not current implementation status.
The runtime migration is committed (`feat(pdf): replace PDF Oxide with MuPDF as
the reader engine`), followed by the review remediation recorded below.

The application-facing `breeze-pdf` adapter now uses `mupdf 0.8.0` for PDF
metadata, outlines, glyph quads, search, thumbnails, whole-page requests, and
clipped region requests; `lopdf` remains the separate annotation-copy exporter.
External PDFs open file-backed, with size/mtime and streaming fingerprint
checks before and after MuPDF opens them. Page geometry, CropBox, `/Rotate`,
UserUnit policy, owned BGRA pixels, and worker-thread ownership are covered by
the current test suite. The race guard itself still lacks a deterministic
source-mutation-during-open regression, and password rejection lacks a focused
fixture.

The reader requests grid-aligned viewport regions, draws each texture at its
page-space bounds, admits visible regions before predicted work, and bounds
resident UI rasters to 96 MiB with a 56 MiB prefetch allowance. MuPDF renders
each request directly through a clipped page device; display lists are not
retained. Region stitching matches whole-page output for four reader rotations,
and fixtures cover PDF `/Rotate`, a nonzero CropBox, zoom replacement, and GPU
atlas eviction. These tests establish the region mapping and output allocation
bound, not a complete process-memory bound.

Validation on this macOS checkout: `cargo fmt --all -- --check` passes;
`cargo test --workspace --locked` passes 216 tests with 3 ignored; and
`cargo clippy --workspace --all-targets --locked -- -D warnings` passes with no
lint exceptions. The three fixed-width `chunks_exact` uses in
`breeze-app/src/about.rs`, `breeze-pdf/src/export/tests.rs`, and
`breeze-pdf/src/raster.rs` now use `as_chunks`, so Clippy 1.99 accepts them.
`cargo run --locked -p breeze-app` reached the
running application with `BREEZE_DATA_DIR` under `target/mupdf-migration-smoke`
and was then manually stopped. This was a startup smoke test, not visual
acceptance.

Page content is verified before its pixels are published: drawn `Do`/`sh`
resources must resolve, forms are bounded by depth and cycle checks,
`DCTDecode`/`JPXDecode` images (named and inline) must decode through MuPDF's own
image loader, and an image mask must carry its declared scanlines. The two
previously ignored integrity gates now run and pass, covering eight damage
classes plus the two the engine legitimately recovers from.

Production gates remain open: the adapter has no verified
MuPDF store-size cap; ICC/transparency and broad font/image corpus parity,
Release same-workload latency/RSS/GPU measurements, and native Windows/Linux
validation remain outstanding. macOS visual acceptance was not performed in
this pass. See [work package 22](22-mupdf-migration.md) for item-level status.
Do not describe the migration as production accepted or claim measured
memory/latency wins from the standalone probe.

## OS file association and Open With — October 15, 2026

Implemented in [work package 23](23-file-associations.md). The local macOS
review bundle and every packaged platform now declare PDF viewing support, so
the operating system's context menu offers Breeze:

- macOS: `CFBundleDocumentTypes` with role `Viewer`, `LSHandlerRank`
  `Alternate`, and `com.adobe.pdf` in both `scripts/bundle-macos.sh` and the
  release packaging path. `Alternate` keeps Launch Services from taking a
  default handler over.
- Windows: MSI and Inno components register `Breeze.Document` under
  `Software\\Classes`, add it to `.pdf\OpenWithProgids`, list `.pdf` in
  `Applications\breeze-app.exe\SupportedTypes`, and uninstall those keys. The
  `.pdf` default value itself is never written.
- Linux: the shared desktop entry declares `MimeType=application/pdf;`; the
  existing `%F` placeholder passes selected files to the command line.

Delivery was fixed and covered by tests: `open_events` decodes `file:` URLs and
command-line documents, queues requests that arrive before the reader window
exists, and—after the first local check showed a warm open silently dropped the
document—updates the reader entity instead of casting the window's `Root`
wrapper. A queued request is also held until the saved library finishes
restoring, and a launch that cannot take the data-directory lock still opens the
requested document instead of discarding it. `settings.rs` no longer claims the
association is "Unavailable"; it states that Breeze is offered under Open With
and that the default stays a system choice.

Validation on this macOS checkout: `cargo fmt --all -- --check`,
`cargo test --workspace --locked` (162 app tests plus the other workspace
suites) and `cargo clippy --workspace --all-targets --locked -- -D warnings`
pass; `scripts/test_release.py` passes 14 tests with one Linux-only container
test skipped. Native checks: the review bundle's Launch Services record reports
`claimed UTIs: com.adobe.pdf` with `rank: Alternate` and `roles: Viewer`; opening
a PDF while an isolated instance was already running (`open -a`) delivered and
persisted the document; a spaced and CJK filename opened correctly. Windows and
Linux "Open With" registration and default-app selection remain unverified on
their own hosts, and no signed installer has been exercised.

### Review remediation

Fixes from the post-migration code review, in commit order:

- Engine swap and Clippy: three fixed-width `chunks_exact` uses became
  `as_chunks`/`as_chunks_mut`, so the documented Clippy command passes without
  an exception. (committed with the migration)
- Silently dropped page content: `breeze-pdf::integrity` verifies what a page
  draws before publishing pixels. (committed)
- Source identity: the reload stamp now includes the device and inode (Windows:
  volume and file index) in addition to size and mtime, so a same-size
  same-timestamp replacement through rename reloads instead of serving cached
  pages. A deliberate same-inode in-place edit that preserves both size and
  mtime still requires the fingerprints that sidecar saves and exports already
  verify.
- Open cost: measured, not assumed. A synthetic 100/1,000/20,000-page document
  reaches metadata-ready in 2.9 / 8.9 / 115.9 ms (debug build, macOS, this
  machine), so the document-wide geometry pass is bounded well inside an
  opening budget. `document_open.rs` keeps a 2 s guard for the 20,000-page case.
- Outline bounds: entries are capped at 10,000, nesting at 32 levels and titles
  at 512 characters, so a malformed outline is truncated instead of stalling or
  overflowing.
- Page bounds: a request for a page outside the document now fails explicitly
  instead of silently rendering the last page.
- Search reuse: extracted page text is retained per source revision (64 MiB
  budget) and handed back to the next query, so a repeat search over an unchanged
  document is an in-memory scan instead of a reopen. The 300-hit cap and cached
  cancellation have focused tests.
- Whole-page budget: the QA whole-page path now enforces a 16.7 Mpx (64 MiB)
  ceiling instead of accepting any scaled request, matching the order of
  magnitude of the reader's own 12 Mpx region budget.
- MuPDF store: the pinned binding creates its context with `FZ_STORE_DEFAULT`
  (256 MiB, `mupdf-sys/mupdf/include/mupdf/fitz/context.h`), which is the
  effective resource-store bound; no safe API changes it, so a smaller bound
  needs an upstream binding API. The ignored Release probe now reports
  `peak_rss_kib` and `mupdf_store_limit_bytes` alongside latency.
- Save and export I/O: a sidecar save hashes the source once and then re-checks
  the file identity before publishing, instead of hashing the whole document
  twice; annotation export streams the source fingerprint and loads the document
  file-backed instead of reading the entire PDF into an extra buffer.

- Worker scheduling: the single pending slot became one slot per priority
  class. A library cover or look-ahead thumbnail can no longer displace the page
  the reader is waiting for, and a request replaced by a newer same-class request
  is answered with an explicit "superseded" error instead of a closed channel.
- Reserved category name: `Unassigned` is reserved in every shipped interface
  language, not only in Simplified Chinese, so a user category can never shadow
  the built-in filter.
- Outline and raster bounds: `outline_bounds.rs` nests an outline 80 levels deep
  and asserts it is truncated at 32; `page_requests.rs` builds a 20000 x 20000
  point page and asserts both the whole-page and region paths refuse it instead
  of allocating the raster, while metadata still reports the geometry.
- AGPL source bundle: building the archive locally produced a 182 MiB
  `Breeze-0.1.0-alpha.1-source.tar.gz` whose checksum verifies and which contains
  the vendored MuPDF C/C++ sources (`vendor/mupdf-sys-0.8.0/mupdf/`, 2,842 files)
  plus `LICENSE`, `Cargo.lock` and `.cargo/config.toml`. The packaged
  `assets/licenses/COPYING-MUPDF.txt` is byte-identical to the copy inside
  `mupdf-sys`. The hosted workflow run and Windows/Linux installer inspection are
  still unperformed.
- Error language: `breeze-pdf` now reports failures as `PdfError` variants
  (format, size, encryption, page count, page bounds, unreadable/render-failed
  pages, zoom, render budget, search) with English catalog keys, and
  `breeze-app::errors` words them through the selected interface language. Pure
  engine diagnostics stay as context and translate only when their text is itself
  a catalog key. The reader's damaged-page test asserts the localized text, so the
  adapter-to-UI path is covered end to end, not just in a unit test.
- Stale dependency notes: the README and engine assessment no longer ask for
  `office_oxide 0.1.9` (retired with `pdf_oxide`), and the `block` 0.1.6
  future-incompatibility notice is recorded as coming from
  `cocoa`/`gpui-pre-macos` at its newest published version.

Release probe on the bundled sample (this machine, macOS, debug-independent
Release build, 20 samples, excludes GPU residency): whole-page
document-cold p50 15.35 ms / p95 20.44 ms, warm p50 0.71 ms, thumbnail cold
p50 8.83 ms / p95 9.61 ms, peak RSS about 195 MiB including the test harness.
These are sample-only numbers and do not replace the release gates above.

## October 14 MuPDF migration authorized

The user accepted AGPL-compatible source distribution and selected MuPDF through
`mupdf-rs`. Phase 0 now declares `AGPL-3.0-only`, carries the root and MuPDF
license texts into installer payloads, and builds a corresponding-source archive
with vendored Cargo dependencies. Release CI is configured to upload and verify
that archive. The source bundle was built from commit `d6c38c5`, its SHA-256
verified, and its contents checked for `LICENSE`, `Cargo.lock`, `.cargo/config.toml`,
and the source builder. Package payload tests, formatting, workspace tests (144
app, 14 core, 21 PDF, 6 PDF integration, 7 storage; three existing probes
ignored), and Clippy with warnings denied on Rust 1.97.1 pass. The hosted workflow
remains unrun. This initial snapshot preceded the runtime adapter work; see the
current status above.

## October 14 PDF engine strategy comparison

[The strategy comparison](../specs/10-engine-strategy-comparison.md) recommends
MuPDF via `mupdf-rs` as the selected backend, conditional on the migration's
adapter/corpus/platform gates. The user explicitly accepted AGPL-compatible
distribution. Keep
the existing UI, core, storage and `lopdf` export boundaries. The option-2 scheduler
improvements (visible-first service, byte admission, prediction backoff) are shared
work and remain valuable under either engine. Sumatra-inspired scheduling alone
does not bound PDF Oxide's whole-page output raster; a maintained region renderer
would still be required for the same high-DPI memory objective. This assessment
snapshot preceded the user's AGPL acceptance and runtime migration; see the
current status above.

## October 14 MuPDF Rust binding assessment

[The `mupdf-rs` assessment](../specs/09-mupdf-rs-assessment.md) records a viable
technical alternative to waiting for PDF Oxide's region API. An isolated Release
probe built `mupdf 0.8.0`/`mupdf-sys 0.8.0` on macOS and rendered the user's 343-page,
17.78 MiB book through MuPDF display lists: whole-page p95 44.86 ms with 77.4 MiB
probe RSS, versus a 512 × 512 clipped replay p95 17.76 ms with 26.1 MiB probe RSS.
These figures exclude Breeze UI, GPU, scheduling and fidelity checks. The wrapper
requires MuPDF C/C++ sources and declares AGPL-3.0. At the time of this
assessment, the contributor constraint still rejected adoption; the user's
subsequent decision and runtime work supersede that snapshot.

## October 14 performance investigation

[The reader performance assessment](../specs/08-performance-assessment.md) records
SumatraPDF release/development source research, the then-current whole-page rendering
policy, a prefetch admission/eviction risk, and visible-thumbnail scheduling behind
speculative work. The October 13 region-rendering entry below predates the vendor
removal in [work package 20](20-upstreaming-engine-changes.md) and no longer describes
the running code.

Release worker probes on the user's 343-page, 17.78 MiB book sampled 20 spread
pages twice per DPI. First-pass thumbnail p95 was 21.54 ms at 24 DPI; page p95 was
34.86 / 78.43 / 243.54 ms at 144 / 288 / 432 DPI. Probe-process peak RSS was
174.44 / 239.89 / 264.27 / 286.11 MiB respectively, excluding application caches,
UI and GPU textures. On the bundled page at 769 DPI, one output was 210.94 MiB,
warm rerender p95 was 468.22 ms, and probe peak RSS was 436.20 MiB. The assessment
records hardware, sample conditions, maximum latency, input hashes and limitations.
These are not a native same-machine comparison with SumatraPDF or application
opening/presentation measurements. No runtime code or rendering policy changed
at that measurement snapshot; the later MuPDF region path is tracked above.
Private inputs and raw research outputs are not committed.

## Code review remediation — October 3, 2026

[Work package 13](13-review-remediation.md) tracks the authorized remediation sequence. Phase 0 registers the missing library-container test observation: formatting, workspace tests (103 passed, three existing ignored probes), Clippy and isolated macOS startup passed. No visual behavior changed.

Phase 1 fixes transactional annotation Save As and blocks unloaded histories. Failed/cancelled writes preserve the previous association; successful writes acknowledge only their captured snapshot. Five permanent regressions pass. Formatting, the complete workspace suite (108 passed, three existing ignored probes), Clippy and isolated startup passed. Native macOS cancellation and successful Save As were exercised; the new association and exact annotation data were read back, and the source PDF hash was unchanged.


The accepted compact UI has progressed into a local reader with durable state, sidecar annotations, restricted standard-PDF annotation export, and opening-window policies. It is **not** a completed production PDF content editor.

Library presentation refinements are tracked item by item in [work package 10](10-library-presentation.md). Follow-up Popover, row, grid, and empty-state changes are recorded in [work package 11](11-library-interactions.md).

Phase 2 implements annotation recovery and gesture ownership. Transient I/O failures can be retried, source and sidecar conflicts remain distinct, validated annotations from a stale-fingerprint sidecar are retained but hidden, and recovery copies preserve that original fingerprint. Explicit reassociation requires confirmation, checks page bounds and rechecks the source fingerprint during publication; explicit discard restores only the in-memory saved snapshot. Close attempts route users to the affected document's annotation panel. Mode and navigation changes clear active gestures. Regression tests cover these conflict, persistence, stale-dialog, close and gesture paths. Formatting, the workspace suite (116 app tests, 11 core, 19 PDF, 3 PDF integration and 7 storage tests passed; three existing PDF probes ignored), Clippy with warnings denied, and isolated app startup pass. The upstream `block 0.1.6` future-incompatibility notice remains. A native macOS capture confirmed the retained annotation, hidden stale coordinates, conflict message and recovery actions with the PDF page still rendered. Reassociation entered its confirmation flow; component tests cover confirmation/cancellation and file-picker behavior. Windows/Linux runtime checks are pending. See [work package 13](13-review-remediation.md). Rendering diagnostics and later product/release gates remain pending.

Phase 3 rendering-integrity changes are implemented and awaiting native acceptance. Audited failed image/inline-image/ImageMask, Form, mask and shading operations no longer produce successful incomplete rasters. Full-page, thumbnail and device-tile requests carry page-numbered errors; the UI exposes a bounded page error and persistent status message, and navigation to good pages recovers. Reading preflight explicitly rejects non-default or invalid leaf-page UserUnit; default units keep the original annotation-coordinate model and align with export's supported boundary. Both the working checkout and isolated candidate pass formatting, 172 workspace tests (three existing probes ignored) and all-target Clippy; isolated startup also passes. Independent Poppler checks confirm unchanged source/export backgrounds and CropBox/rotation dimensions. Native error-panel inspection is blocked by a locked macOS session (`CGSSessionScreenIsLocked=true`), so this phase remains uncommitted; startup and component tests do not replace native acceptance. See [work package 13](13-review-remediation.md) for evidence and remaining fidelity/platform gates.

## Home navigation, About, and Shortcuts — October 12, 2026

[Work package 18](18-home-navigation.md) consolidates global pages into Home.
`Screen::Settings` is gone; `HomePage` selects the library, About, Shortcuts, or
Settings page inside the Home content area. The sidebar keeps library filters and
categories scrollable and adds About (`info`), Shortcuts (`keyboard`), and Settings
(`settings`) icon buttons along its bottom edge. The titlebar Settings button and
Primary+, select the Settings page; the titlebar Home button and Primary+Shift+H
select the library page.

About shows the bundled logo, application name, installed version, update check and
localized status, author, and repository address. Shortcuts lists real bindings
resolved from `Kbd::binding_for_action` / `Kbd::global_binding_for_action` and omits
actions without a registered binding; custom shortcut editing remains unavailable.
Settings was reduced to title-plus-control rows: the appearance/data-directory/GPUI
Kit subtitles, the inline shortcut note, and the inline version line were removed.

A follow-up pass stacked the sidebar entries full-width in the order Shortcuts
(Primary+K), About (Primary+I), and Settings (Primary+,), with shortcut tooltips
resolved from the real bindings. The three global pages share a bounded, centered
column that leaves clearance before the scroll clip; About is centered on both axes
and shows `Pone Ding<poneding@gmail.com>`. Settings dropped the "Never" update
policy, and state version 8 keeps older snapshots valid by mapping the retired value
to Manual.

Validation: `cargo fmt --all -- --check`, the workspace suite (126 app tests plus
core, PDF, storage and packaging tests; three existing PDF probes ignored), and
`cargo clippy --workspace --all-targets --locked -- -D warnings` pass.

Native macOS inspection ran twice. The first pass (before the session locked)
confirmed the library page, sidebar selection, and titlebar Settings activation. The
second pass after the screen was unlocked confirmed the refined layout in an
isolated data directory: the three sidebar entries stack in the requested order, the
Settings page keeps every control inside a centered 720 px column with clear space
before the right edge, and About is centered on both axes with `版本 0.1.0`, the
author line, and the repository link. Component tests pin the exact tooltip keys
(⌘I/⌘K/⌘, on macOS, Ctrl equivalents elsewhere), the stacked order, the capped and
centered columns, and About's two-axis centering. The upstream `block 0.1.6`
future-incompatibility notice remains. Windows/Linux runtime checks are pending.

## Current implementation

| Area | Delivered behavior |
|---|---|
| Library | All Documents first; whole-row opening, aligned list/cover-grid views, empty new libraries, persisted multi-category memberships and eight tag colors, inline hover actions, favorites/filtering, and removal without deleting source files |
| Preferences | Persistent Follow System / GPUI Kit built-in theme selection, sidebar width, independent file/position restoration, and opening-window policy |
| State integrity | Background/coalesced JSON, process locking, bounded inputs, atomic replacement, external-revision checks, and normal-close acknowledgement |
| Reading | Single/Continuous/Spread layouts; per-document view rotation in either direction; immersive fullscreen that keeps the document only; virtualized full outline/thumbnail lists; visible-range scheduling; 32 MiB worker, 64 MiB UI-page, and 8 MiB UI-thumbnail budgets; page navigation and zoom |
| Text/search | Reading-order text selection/copy with per-row highlight fills; cancellable background search with up to 300 results and page destinations |
| Annotations | Text/area highlight, underline, strikeout, ink, ink-only eraser, text note, ellipse, rectangle, line, free wavy line, and arrow |
| History/sidecars | Bounded undo/redo, per-page removal, source SHA-256, explicit Save/Save As only, and conflict refusal |
| PDF export | New-file-only standard annotations with vector normal appearances; old content/resources/annotations checked before publication |
| Export UX | Immutable snapshot, native save dialog, Primary+Shift+E, one in-flight job, close protection, receipt, and disclosed sticky-note semantics |
| Window policies | Default keeps geometry; Maximized maximizes once; Fit Document sizes once using remaining display space without changing zoom |
| Page-task lifetime | Foreground page tasks are owned and cancelled on replacement, Home, and close rather than left detached |

## Plans executed in this follow-up

- [05 — Annotation export](05-annotation-export.md) was written before implementation, then implemented and validated for the stated subset.
- [06 — Opening-window policies](06-window-policy.md) was written before implementation, then implemented and tested. Native QA found an origin-related sizing issue, which was corrected with a regression test.
- [07 — Remaining work](07-remaining-work.md) defines concrete renderer, layout, localization, platform/release, recovery, and imported-annotation packages. It is a backlog, not a completion claim.

## Export restrictions

- Export creates a new PDF and never overwrites the source, aliases, or an existing destination. It does not acknowledge a sidecar save.
- Notes export as standard sticky-note icons with the full Unicode contents in the popup, not inline FreeText boxes with guessed fonts.
- Existing PDF annotations are preserved but are not imported into editable Breeze history.
- Encrypted, signed, form/widget, unsupported page-geometry/UserUnit, excessive object-count/ID/nesting, and oversized-output cases are rejected.
- Original page streams/resources and existing annotation entries are checked, as are newly serialized annotation/appearance objects. Older supported PDF headers are raised to 1.7 for the introduced transparency/appearance features.
- This is not a sanitizer, a signature-preserving workflow, or an archival-conformance guarantee.

## Executed validation

- **64 automated tests passed**. Three fixture/performance tests are ignored by the normal suite; the export fixture generator was run explicitly for independent verification.
- Formatting, Clippy with `-D warnings`, and local macOS development packaging passed. The upstream `block 0.1.6` future-incompatibility notice remains.
- Export tests cover every stored annotation kind, exact Unicode bytes, source-content preservation, indirect/shared Annots arrays, rotation/cropping/inheritance, old headers, invalid geometry/style/page indices, signed/form/encrypted inputs, sparse IDs, aliases, and non-clobbering behavior.
- UI tests verify immutable export snapshots, in-flight close protection, failure cleanup, unchanged sidecar acknowledgement, window sizing, and unchanged page zoom.
- Poppler and macOS PDFKit rendered an eleven-kind controlled export. Both were visually inspected. Pypdf and PDFKit read back the exact Chinese/emoji note text.
- Poppler's first-page rendering with exported annotations hidden was pixel-identical to the source first-page rendering. This is a controlled one-page visual check, not a whole-corpus claim.
- A native macOS instance with isolated data exercised the export shortcut, actual save dialog, successful receipt, and a one-annotation output independently reopened afterward.
- Native inspection also confirmed that a saved ink annotation is visible after restoration, closing the prior overlay-positioning visual check for the controlled sample.

### Native window-policy caveat

The initial Fit Document check resized the window, but using full display dimensions while preserving the old origin could leave the footer off screen. The calculation now uses the remaining space at the actual origin, including secondary-display coordinates. A maximized-to-fit transition waits for a native bounds event; fullscreen mode is preserved.

The final native recheck after this correction was blocked by the locked Mac. Post-fix window-position/maximize-transition checks and Windows/Linux native acceptance remain pending. Do not mark those passed based on unit tests alone.

## Known engine risks

- Standard-font substitution/fidelity and the earlier real-paper raster p95 miss remain open.
- PDF Oxide 0.3.78's high-level annotation Contents API decodes UTF-16 strings as UTF-8. Export retains valid standard encoding, independently verified externally. Future imported-note UI needs a tested decoder/upstream correction.
- The annotation writer does not resolve these reader defects or establish general document interoperability.

## Performance scope

The prior reader-only Release probe on the bundled eight-page sample measured first preparation around 112.62 ms and cached-page round-trip p95 around 0.77 ms, excluding GPU presentation. These remain historical sample observations, not current release budgets.

One controlled export fixture run took about 14.72 ms in the development profile, excluding rendering and UI. This is not an export p95 or large-file memory result. Export performs explicit background parsing, hashing, serialization, and revalidation; representative costs still need measurement under the remaining-work plan.

## Remaining work

- Renderer fidelity/corpus coverage, adaptive DPI/tiles, and representative-corpus layout/scroll performance acceptance. Fixed-raster continuous/spread layouts and virtualized navigation are implemented.
- Importing/editing pre-existing PDF annotations without duplicate appearances.
- Native localization acceptance outside macOS; Windows and Linux Open With
  registration and default-app selection on their own hosts; signed
  update/install delivery and production-ready installers/packages.
- Complete conflict recovery/reassociation UX and native three-platform/resource/fuzz acceptance.

The remaining-work plan lists prerequisites and explicit gates; release automation does not waive them.

## SemVer release automation

The tag-push workflow validates SemVer (optional lowercase `v`) and exact workspace-version equality, checks/builds macOS arm64/x64, Windows x64, and Linux x64, and publishes archives with SHA-256 checksums only after the full matrix succeeds. Prerelease tags are marked as such; upload failures leave a resumable draft, and published releases are not overwritten. Packaging preserves the maintained notices and lockfile. See [release operations](../releases.md).

Local validation passed: eight release-tool tests (including all four archive layouts), Actionlint 1.7.12, Rust formatting/check/tests (64 passed, three ignored), Clippy, and an optimized macOS arm64 build. The real macOS ZIP was extracted and checked for its checksum, valid plist, executable permissions, architecture, and system-only dynamic-library paths. The existing upstream `block 0.1.6` future-incompatibility warning remains.

No GitHub remote is configured in this checkout, so no tag was pushed and no hosted workflow/release publication has been executed. Other-platform builds/runtime, signing/notarization, and the full dependency-license audit remain unverified or pending. Local phase commits are now required by AGENTS.md; unrelated in-progress changes remain untouched.

### Makefile command layer

The root Makefile exposes formatting/check/lint/test/verify, debug build/run, optimized release build, local packaging, and clean targets. All Cargo resolution/build/test commands use the lockfile; `make run` defaults to isolated data under `target/manual-state/`. Release/package targets never tag, push, or publish.

`make verify`, `make build`, and `make package` passed locally; the tooling suite now has 13 passing tests, including Make dry-run contracts and Windows executable naming. The then-current working-tree Rust suite passed 65 tests with three ignored. An injected failing Cargo command was propagated by Make. `make run` started the real executable with a unique data directory under `target/release-validation/`; the scratch process was then interrupted and stopped. This is startup verification, not visual or cross-platform runtime acceptance. `clean` and the mutating `fmt` command were checked by dry run only to avoid deleting local outputs or changing unrelated work.

## September 30 reading/library refinement

[Work package 08](08-reading-layout-and-library.md) records the requested density, left alignment, user-category management, unified rounded tabs, and real reading layouts. Title/toolbar/status heights remain 34/38/32 px. Root/Kit base typography is now 13/15 px, and Small controls replace XSmall. Tab close remains hover-only with Primary+W as its keyboard alternative. Primary+Shift+L cycles layouts.

Document title tabs now use one Kit Button surface with direct icon/text content, no inner text-button background or outer shadow, 8 px left padding, and no extra right padding around Close. Their Small height matches Home. Formatting, workspace tests (67 passed, three ignored), Clippy, isolated application startup, and native macOS inspection passed; component assertions cover equal height/alignment, right-edge close placement, and closing without activation.

A follow-up removes all inter-tab separators and insets an 18.5 px square Close button with independent hover/pressed colors. The requested selected-tab emphasis was subsequently withdrawn: the tab retains its original background with no added border. The tab uses the same group-hover and pressed fill across its entire surface. Workspace tests (67 passed, three ignored), formatting, Clippy, and isolated startup pass. Native interaction/visual validation remains pending: the computer-use API exposed only window controls and rejected interaction with `noWindowsAvailable`, including after rebinding a separate review bundle. This follow-up was committed with that native interaction check still pending.

Native macOS review verified category creation and assignment, two actual side-by-side pages, thumbnail scrolling through the final sample page, continuous scrolling across page boundaries, restored layout/page state, and the synchronized page input. The first category smoke test inadvertently used the default data directory because interactive terminal entry lost the environment variable's initial character; the review-created category was removed under the state lock, without touching real documents. Subsequent review uses direct command execution with a verified isolated `BREEZE_DATA_DIR` under `target/`.

A new Release worker probe measured full-page cold p50/p95 16.92/54.19 ms and thumbnail cold p50/p95 2.61/3.09 ms on the project sample (20 observations each). Cached p95 was 1.09 ms / 0.02 ms respectively. See work package 08 for hardware, cache conditions, dimensions, and explicit timing/memory scope. These are not end-to-end or corpus acceptance results.

The user began actively interacting with the review instance during validation. Its process and newly created category were preserved. The latest scheduler-only refinements and allocation guard were checked automatically and rebuilt, but the in-use review process was not forcibly restarted; a final native recheck of those refinements and an isolated idle-CPU/frame-time measurement remain pending. Do not count the running older process as acceptance of the latest binary.

## Built-in theme settings refinement

The settings screen now has one native searchable Kit Select for Theme instead of segmented mode buttons and separate light/dark palette rows. The initial implementation mistakenly exposed only Kit's two automatically registered defaults. This has been corrected by embedding all 21 official gallery JSON files from the same source commit as Kit 0.7.0 and explicitly registering their 36 themes. The dropdown contains **39 choices**, grouped by System / Light / Dark: Follow System, 12 light themes, and 26 dark themes. Each appearance group starts with its default, followed by case-insensitive name order. The complete names and group membership match the user-provided list.

[Theme provenance](../../assets/themes/README.md) records the pinned upstream source and license. Original author metadata and JSON bytes are preserved; every file was verified against the upstream Git blob ID. No runtime network access, external theme directory, or theme-directory watcher is required. Kit's selected configuration supplies both component and application-chrome colors without Breeze palette overrides; compact typography/radii and 34/38/32 px chrome heights are retained. PDF colors and page jobs are unchanged.

The titlebar command and Primary+Shift+T cycle the same choices. Follow System continues to use window appearance events; named themes retain their declared mode. The theme name is persisted in the current state schema (version 8). Every legacy version preserves System or migrates Light/Dark to the matching built-in theme, and the retired palette fields are read but omitted from new snapshots without changing library, category, or annotation references.

Validation for this change:

The shared working tree carries unrelated uncommitted follow-up work, so this phase was verified in an isolated tree containing only the theme changes. That tree passes `cargo fmt --all -- --check`, `cargo test --workspace --locked` (115 passed; three existing fixture/performance tests ignored), and `cargo clippy --workspace --all-targets --locked -- -D warnings`. The upstream `block 0.1.6` future-incompatibility notice remains.
- Tests assert the complete ordered 38-theme list and appearance groups, exact color/gradient-token application for every theme, default-theme resolution, and legacy-state migration. Real component tests cover Aurora Light dropdown confirmation, Tokyo Night search/confirmation/cancellation with correct grouped indices, titlebar/shortcut synchronization, Catppuccin Mocha restoration, unchanged saved document state, and preserved chrome dimensions.
- With the complete gallery registered, `cargo run --locked -p breeze-app` built and launched successfully on macOS from the isolated tree, with a process-verified `BREEZE_DATA_DIR` under `target/theme-commit-review`. The scratch process and terminal pane were stopped after the startup check.
- Native visual/interaction inspection remains **unverified**: the configured Computer Use entry point failed with `/bin/bash: orca: command not found`. Startup and component tests are not a substitute for visual inspection or native OS appearance-event testing; Windows/Linux native checks also remain pending.


## Status layout and zoom controls

The status bar now exposes independent one/two-page and continuous choices in a native Kit menu, separated by a divider. New documents default to continuous single-page reading. All four combinations render real pages, including virtualized continuous spreads with a lone odd final page. Fit Page / Fit Width / Actual Size share a second native menu, with an icon and current mode or percentage on the trigger.

Primary+wheel zooms over the document without scrolling it; Primary+0 / + / - use the same zoom mutations (Primary is Command on macOS and Control on Windows/Linux). Existing equals-key zoom-in remains available. Percentage zoom stays within 25–200%, and fit-to-percentage steps begin at the displayed scale. Per-document settings and the defaults for new documents persist in state version 4, independently of position restoration; explicit legacy layout/zoom values are preserved.

The full working checkout passes formatting, workspace tests (78 passed, three existing tests ignored), and Clippy with warnings denied. Component tests cover menu selection, independent layout choices, continuous spreads through an odd final page, platform-primary keyboard/wheel behavior, ordinary scrolling, JSON restoration, and new-document defaults. Native macOS startup used a process-verified isolated data directory. Visual inspection confirmed both menus, their icons, divider/checkmarks, current status labels, and preserved compact chrome; the state file contains the selected layout and percentage. The review instance remains open after user interaction. Restart behavior is covered by component restoration tests; a manual native restart and Windows/Linux runtime checks were not performed.

The isolated commit, excluding pre-existing theme and raster changes, also passes its workspace suite (69 tests, three existing tests ignored).

## September 30 PDF display quality fix

The reader now requests metadata separately and renders visible device-resolution tiles with the pinned PDF Oxide engine. It no longer enlarges a fixed 144-DPI page raster. Scale-aware caching, stale-result checks, explicit GPU atlas eviction, and 508-pixel tile interiors with two-pixel gutters preserve sharpness while bounding resident page data. The 64 MiB UI page budget is split into 8 MiB metadata and 56 MiB pixel tiles; thumbnails retain 8 MiB. A local upstream-source patch adds direct region allocation without modifying source documents.

[Work package 09](09-pixel-accurate-display.md) records native old/new comparison on the user's Chinese documents, independent PDFKit checks, Release tile p50/p95 measurements, and the GPU memory faults found and fixed during validation. The integrated checkout passes 79 tests (three existing ignored probes), formatting, Clippy, and local build/startup checks. Native large-window text is visibly sharper. Broad fidelity, frame-time, peak-memory/RSS, and Windows/Linux release gates remain open; the final observed macOS physical footprint was 216.0 MiB with a 361.6 MiB peak, not a universal memory guarantee.


## Separate zoom mode and percentage

The single-page icon is now `file`. Zoom mode is independent from its numeric status: Fit Page / Fit Width / Actual Size / Custom Ratio remain in one native menu, followed by a separate minus / calculated percentage / plus group. Fit percentages recompute with the viewport; changing the percentage switches to Custom Ratio, including custom 100%. Discrete zoom commands move to the next 25% boundary in their direction (167 → 175 or 150), and smooth trackpad input retains fine-grained zoom. State version 5 distinguishes Actual Size from custom 100% and migrates legacy 100% values to Actual Size.

The complete working checkout passes formatting, workspace tests (81 passed, three existing probes ignored), and Clippy with warnings denied. The isolated commit on top of the PDF display fix also passes formatting, 78 tests, and Clippy. Tests cover the four modes, separate control geometry, fit-scale recalculation, 167% directional grid steps, mode switching on percentage changes, custom 100% restoration, and legacy 100% migration. Native macOS inspection with isolated data confirmed the file icon, four-option menu, separate percentage, keyboard grid steps, and Fit Width at 136% switching to Custom Ratio at 150% after zoom-in; state JSON persisted the chosen mode/percentage. The review window remains available. Windows/Linux native validation remains pending.

## October 1 reader-continuity follow-up — native verification pending

The working tree now prefetches a bounded region around the viewport and adjacent pages, retains previous pixels during scale replacement, uses stable semantic zoom identities, and preserves continuous scroll position when the outline width changes. Fit modes reuse density reserved for a collapsed outline where the budget permits; larger cases keep the previous picture during native-density replacement. See [work package 11](11-reader-continuity.md).

Six new continuity regressions pass, including asynchronous prefetch completion with no additional frame/scroll event. Formatting and Clippy pass. The workspace run excluding one independently failing library observation test passes 99 tests; it is not a full-suite pass. Native verification is blocked by the locked Mac, so this follow-up was committed without native verification, which remains pending; it must not be marked visually accepted yet.

## Outline navigation refinement (October 3, 2026)

- Leaf entries omit the chevron; rows are 24 logical pixels high with 4 pixels of top inset.
- Exactly one resolved outline entry is active: the closest destination at or before the current page, preferring deeper entries and then the first peer on ties. Pages between headings retain the current section.
- Outline destinations currently contain page numbers only; matching the reader position to separate headings within one page remains unavailable.
- Regression coverage includes same-page parents/peers, unresolved destinations, gaps between headings, out-of-order destinations, and backward navigation. Formatting and Clippy passed; the app suite passed 60 tests but failed the unrelated `library_empty_component_is_centered_in_both_views` test (missing `home-content` observation), so this phase is not committed. An isolated native macOS run confirmed the bundled sample leaf has no chevron and has the top inset; multiple-heading visual coverage remains pending.

### Parent outline folding follow-up

- Parent chevrons now toggle their descendant rows without changing pages. All titles only navigate and never change branch folding. Unresolved titles are disabled while their parent chevrons remain usable. Leaf titles have no chevron.
- Expanded parents use a downward chevron, collapsed parents use a rightward chevron. Nested fold choices survive collapsing/reopening an ancestor and page navigation; changing the loaded source or fingerprint resets these transient choices.
- The virtual list uses visible outline indices while preserving original entry identities. When the active entry is hidden, only its visible collapsed ancestor is highlighted. Title buttons fill the remaining row width.
- Logic and real-component click regressions pass, covering nested folds, unresolved parents, same-page active ancestors, full-width title targets, and leaf navigation. Formatting and Clippy pass. The app suite passes 62 tests and still fails the pre-existing `library_empty_component_is_centered_in_both_views` observation test; other workspace crate tests pass. This follow-up was committed with that native recheck still pending.
- An isolated native macOS run with a three-level synthetic outline confirmed downward chevrons, leaf omission, branch collapse, and promotion of the single highlight to the collapsed parent. Native automation intermittently returned `noWindowsAvailable` or delayed actions; reopening and the final row-width refinement are covered by component tests, with native rechecking pending.

- Title/arrow separation was subsequently verified in a native macOS run: clicking a parent title navigated from page 3 to page 1 while preserving both expanded and collapsed states; only its chevron changed child visibility. The full-width title presentation was also visually checked. The updated component regression passes; the full app suite remains at 62 passes and the same unrelated library observation failure.


## October 3 titlebar tab styling

Document tabs now use the theme accent hover color, with the same 50% opacity in dark mode as ordinary Kit ghost buttons, instead of the list-active fill. The selected tab keeps the theme background and uses `shadow_xs`, matching the settled segmented Tab shadow; no accent border is added. Hover/pressed fills remain separate from the selected background so the selected tab can also respond to hover. Existing separator removal and inset Close-button behavior are retained.

Formatting, the tab-close component test, Clippy, and the debug build pass. The initial workspace run failed in the existing `library_empty_component_is_centered_in_both_views` test because `home-content` was not registered for test lookup; unrelated staged library changes were preserved. An isolated native review opened the project sample and exposed an overly heavy first shadow, which was reduced to `shadow_xs`. The Mac then locked, blocking the final visual recheck. These changes were committed with that final visual recheck still pending.

On October 8, the selected-tab background was corrected to apply the complete `colors.background` token directly. Kit's Custom button variant had mixed the supplied base color with transparency, so passing the theme color through that variant did not preserve its value. Direct background styling preserves the theme's color/gradient while retaining the existing hover, close-button behavior, and `shadow_xs`. Formatting, workspace tests (108 passed, three ignored), Clippy, and isolated native startup pass. Visual inspection is still pending because the configured native GUI tool is unavailable (`orca: command not found`); this follow-up was committed with that visual inspection still pending.

On October 9, the selected-tab shadow was increased from `shadow_xs` to `shadow_sm` at the user's request. This uses Kit's stronger two-layer shadow without changing the exact background token, hover behavior, or control geometry. Formatting, the complete workspace suite (116 passed, three existing probes ignored), Clippy, and isolated native startup pass. The native review used two project-sample copies under `target/tab-shadow-strength/`; the captured window remained on its initial frame and the macOS session reported itself locked. Final visual inspection is therefore unverified; this shadow adjustment was committed with that check still outstanding.

## October 9 adaptive navigation sidebar

The Outline tab now appears only when the loaded document has outline entries. Documents without an outline show Thumbnails automatically, including through the Outline shortcut, without changing the saved navigation-view preference. Cached document outlines remain available while an uncached page loads, so page navigation does not temporarily remove the tab or its content.

Thumbnails use virtualized grid rows with compact cards capped at 144 logical pixels wide: one column at 168/208 px, two at 280 px, and three at 360 px. Cards preserve the page aspect ratio, page number, and current-page border. Incomplete final rows retain their column alignment. Visible rows map to complete page ranges; page navigation maps back to the correct row. Resizing retains the top visible page and fractional row offset, with integral row heights preventing layout-rounding drift. Scheduling covers the entire visible range through the existing serial worker and 8 MiB thumbnail cache.

Validation:

- Formatting, workspace tests (111 passed, three existing fixture/performance probes ignored), Clippy with warnings denied, and the debug build pass in the complete working checkout. The isolated phase on the committed baseline also passes formatting, 98 workspace tests (the same three probes ignored), and Clippy.
- Real component regressions cover outline availability, tab selection and shortcuts, retained outlines during page loading, automatic thumbnail preparation without an outline, 168/208/280/360 px grid geometry, resizing around page 61, page-index navigation, and a 121-page document's incomplete final row. The asynchronous worker test confirms that every visible grid column loads within the thumbnail byte budget.
- `cargo run --locked -p breeze-app` launched on macOS with process-verified isolated state under `target/sidebar-grid-review/`. Native window screenshots confirmed the conditional tabs, centered headers, two- and three-column grids, complete portrait/landscape thumbnails, page labels, and current-page borders. Narrow-column bounds and scrolling to the final thumbnail are covered by component tests; the attempted narrow native screenshot captured restoration rather than the finished reader and is not counted as visual validation.
- Orca native interaction automation was unavailable (`zsh:1: command not found: orca`); native visual inspection used system window captures. Windows/Linux native checks and the existing production fidelity/performance gates remain open. The upstream `block 0.1.6` future-incompatibility notice remains.

Generated fixtures/screenshots/state are excluded from version control. Review windows that received interaction were preserved; their isolated data remains under `target/sidebar-grid-review/`.

## October 9 outline controls and title search

Navigation tabs are left-aligned. Outline view shows title search on the right when entries exist, plus an `ellipsis-vertical` dropdown only for a nested outline. Its Expand All / Collapse All items use `list-chevrons-up-down` / `list-chevrons-down-up` and change folding without changing pages. Flat outlines omit the entire disclosure-icon slot. The required existing branch-folding and active-section implementation is included in this phase; unrelated theme, rendering-continuity, and library changes remain outside it.

Title search uses a compact Kit Input below the 38 px header. Matching is case-insensitive and retains ancestors, including titles below previously collapsed branches. Filtered results keep their original destination identities and use temporary folding choices. Closing search or pressing Escape restores the prior folds and scroll position; clearing the query restores the original folds. Input editing keys retain their normal behavior, and missing matches have an explicit empty state. The search state belongs to the loaded document and resets with its source/fingerprint.

The navigation minimum is now 192 logical pixels, with the default still 208 and the maximum still 360. The native 168 px review exposed an edge-clipped More button despite the headless geometry check; 192 px provides space for native font metrics. Older saved widths in 168–191 px remain valid and are raised when restored; future snapshots save the clamped width.

Validation: the complete working checkout passes formatting, 116 workspace tests (three existing fixture/performance probes ignored), Clippy with warnings denied, and the debug build. The isolated outline phase passes formatting, 106 workspace tests (the same three ignored), and Clippy. Regressions exercise the real dropdown through keyboard selection, filtering and destination clicks, Unicode titles, folded matches and restoration, scroll restoration after no matches, Escape, absence of controls in Thumbnails/empty outlines, the flat-title inset, minimum-width geometry, and legacy-width restoration.

Native macOS startup used `cargo run --locked -p breeze-app` with process-verified isolated state under `target/outline-controls-review/`. System window captures confirmed the final 192 px header, left-aligned tabs, both right-side icons, the nested outline, and the flat outline without a disclosure slot or More button. Orca interaction automation remains unavailable (`zsh:1: command not found: orca`), so dropdown/search interaction was verified with real component tests, not automated native input. Windows/Linux native acceptance and the existing rendering/release gates remain open. Screenshots, fixtures, and local state are excluded from version control; the review window that received interaction was preserved.

## October 9 search-row visual consistency

Outline title search now uses the same shared row styling as document search: a fixed 34 px height, Kit horizontal padding and control gap, centered Small controls, and the theme's bottom border. The existing navigation-header border supplies the top edge. Removed the asymmetric bottom-only inset and the extra in-field clear button; the row contains the input and its separate close button. Filtering, Escape, and fold/scroll restoration behavior is unchanged.

Formatting, the complete workspace suite (116 passed, three existing probes ignored), and Clippy with warnings denied pass. The isolated phase passes 106 tests (the same three ignored) and Clippy. Native macOS inspection confirmed the expanded row at the 192 px sidebar minimum, including its bottom divider and balanced spacing. Because native control automation did not reliably open the input, an isolated preview opened the existing search UI on its first loaded frame; that temporary state setup was removed after the capture. This verifies native layout, while the existing component tests verify opening, filtering, and closing. Review data and screenshots are under `target/search-bar-review/` and excluded from version control.

## October 9 navigation-divider consistency

The Kit splitter now owns the single 1 px boundary between navigation and content; the navigation panel no longer paints an adjacent border. Its drag indicator is 4 px wide and explicitly centered over that line. The appearance override retains Kit's hover/press/drag lengths, opacity transitions, pointer hit area, and resizing behavior. The divider uses the same palette border color as the toolbar and navigation header.

Validation: the complete working checkout passes formatting, 116 workspace tests (three existing probes ignored), and Clippy with warnings denied. The isolated phase passes 106 tests (the same three ignored) and Clippy. Existing component tests exercise pointer resizing, width bounds, and collapse/restoration. Native macOS startup uses `cargo run --locked -p breeze-app` with process-verified isolated state under `target/sidebar-divider-review/`. System window captures at 2× confirm matching two-physical-pixel horizontal/vertical borders and an eight-physical-pixel hover indicator extending three physical pixels equally past each side of the divider. Orca is unavailable (`command not found`); macOS accessibility and CoreGraphics supplied the native hover interaction. Native Windows/Linux checks remain unperformed. Generated review files are excluded from version control; the review window is retained after interaction.

## October 9 document icon colors

Document tabs and library cover placeholders now share a format-specific `file-text` foreground: PDF orange-red, EPUB green, and Markdown blue. Each hue has light/dark variants and remains independent of the application's accent and the tab's active/hover state. Extension matching is case-insensitive; the bundled sample uses PDF coloring and unknown/missing extensions retain the theme's muted foreground. Original page pixels, real covers, warning icons, and tab labels/backgrounds keep their existing presentation. EPUB and Markdown remain presentation mappings only; their reading backends are unavailable. Exact colors are recorded in [the icon specification](../specs/06-icons.md#document-identity-colors).

Validation: the complete working checkout passes formatting, 116 workspace tests (three existing probes ignored), and Clippy with warnings denied. The isolated phase passes formatting, 106 workspace tests (the same three ignored), and Clippy. Existing real component tests cover tabs, close controls, library covers, and theme changes. Native macOS startup used `cargo run --locked -p breeze-app` with verified isolated state under `target/document-icon-review/`. Native screenshots confirm the colors in light/dark themes, selected/unselected PDF tabs, and hover states; manually prepared library entries exercise the planned format mappings and neutral fallback without enabling their backends. Orca was unavailable (`zsh:1: command not found: orca`), so macOS window capture and CoreGraphics supplied native inspection. Windows/Linux native checks remain unperformed. Generated fixtures, screenshots, and review state are excluded from version control.


## October 9 Settings icon

The titlebar Settings command now uses the bundled Lucide `settings` gear (`IconName::Settings`), and the icon map records that selection. Formatting, workspace tests (116 passed, three existing probes ignored), Clippy, and isolated native startup passed. A capture of the native macOS window confirmed the gear in the existing titlebar button. Review state and screenshots are under `target/settings-icon-review/` and excluded from version control.

## October 9 unavailable cover icons

Both library views now use the shared, format-colored `file-text` icon whenever a cover is pending or unavailable. Cover failures retain their explanation in the document tooltip, and successfully loaded covers remain thumbnails.

Formatting, workspace tests (116 passed, three existing probes ignored), and Clippy with warnings denied pass. Native `cargo run --locked -p breeze-app` used isolated state under `target/cover-fallback-review/`. macOS window captures confirmed the light list and dark grid with orange-red PDF, green EPUB, blue Markdown, neutral unknown-file icons, and an unchanged real sample cover. Planned-format entries were prepared only in the review state; reading support is unchanged. Native Windows/Linux checks remain unperformed. Generated fixtures, state, and captures are excluded from version control.


## October 9 leading document-tab spacing

The horizontal document-tab scroller now reserves 4 logical pixels inside its left edge so the first selected tab's shadow fits within the scroll clip. Tab-to-tab spacing, tab content padding, and titlebar height are unchanged. Formatting, the complete workspace suite (116 passed, three existing probes ignored), Clippy, and isolated native startup pass. A native macOS capture of two tabs confirms the first active tab's left shadow is fully visible. Review sample copies, state, and screenshots remain under `target/tab-leading-space/` and are excluded from version control.

## October 9 filled favorites

Library and reader Favorite buttons now use an outline bookmark for ordinary documents and a filled Lucide derivative for favorites. Favorite state no longer adds a persistent button background. Hover/press feedback, Primary+D, persistence, and independent library secondary clicks remain intact; accessible labels reflect Favorite/Remove Favorite.

Formatting, workspace tests (116 passed, three existing probes ignored), and Clippy with warnings denied pass. Native macOS inspection of the isolated `cargo run --locked -p breeze-app` window confirmed the solid reader bookmark and adjacent solid/outline library states without persistent button fills. Review data and captures are under `target/reader-tools-review/`. Windows/Linux native checks remain unperformed. The remaining sidebar/tool-settings work is tracked in [work package 14](14-reader-tools-and-details.md).

## October 10 document sidebar and tool settings

The reader toolbar now ends with Search, Favorite, and a document-sidebar toggle in every mode. The optional 256 px sidebar starts closed, with Information and Annotations tabs in a 38 px header. Information uses the loaded document's name, format, pages, current-page dimensions, categories, and source/sidecar locations. The virtualized annotation list covers the current document across pages, supports type and current-page filters, navigates to an item's page, and deletes the checked item through the existing undo/redo and sidecar-save workflow. Save, Save As, export, and error reporting remain reachable; annotation errors also appear in the reader status.

Tool configuration moved out of the permanent sidebar into interactive Kit hover cards (350 ms open / 300 ms close). Pointer travel into the card retains it; keyboard activation opens a focusable Kit popover with Escape dismissal. Hovering alone does not select a tool. Annotation styles are independent per tool for the current session. The palette shares eight fixed, saturated colors with category tags. Highlight/area tools default to 40% amber; strokes/notes default to full opacity, and the displayed percentage is the actual saved opacity. Existing annotations and original PDF page content are unchanged.

Validation: formatting, the full workspace suite (122 passed; three existing probes ignored), and Clippy with warnings denied pass. The isolated commit candidate passes 112 workspace tests (the same three ignored) and Clippy. Six new logic/component tests cover real hover-card pointer travel, eight colors and saved stroke geometry, independent tool settings, note input, keyboard activation/Escape, sidebar mode independence and narrow geometry, filter indices/order, current-page filtering, deletion/undo, navigation, and toolbar search focus. A test caught the filter menu's closing transition intercepting an immediate synthetic row click; the regression now waits for that transition before testing the row.

Isolated native macOS runs used `cargo run --locked -p breeze-app` and review state under `target/reader-tools-review/`. Light-theme captures confirm the toolbar order, annotation sidebar/filter, and interactive tool-card layout; dark-theme capture confirms the toolbar and unchanged page colors. Native activation/accessibility was intermittent (`Could not activate review window`, `target unavailable`), so dark sidebar/card interactions and the final small swatch/row sizing refinements are covered by component checks rather than a complete native recheck. Windows/Linux native checks remain unperformed. Review captures, sample copies, sidecars, and scratch builds are excluded from version control.

## Library Empty state (October 10, 2026)

The fourth item in [library interaction refinement](11-library-interactions.md) is complete. Empty libraries and filtered results use the native Kit Empty component with a small icon and “暂无文档”; table headings and the persistence footer are omitted for empty results. The committed baseline plus this change passed formatting, 116 workspace tests (three existing probes ignored), Clippy, startup, and native macOS empty-layout inspection. Other in-progress reader/settings changes remain outside this validation; Windows/Linux runtime acceptance remains pending.

## October 10 native annotation controls

Text annotations use Lucide `type` in the toolbar and annotation list. Line width uses a Kit NumberInput with decimal input, 0.25 pt steps, and a 0.25–100 pt range. Opacity uses a Kit Slider across 0–100% and displays the current percentage. Controls retain each tool's independent values and reject stale events after changing tools; invalid line-width input does not change the applied style.

Formatting, workspace tests (123 passed, three existing probes ignored), and Clippy with warnings denied pass. Component checks cover typed/stepped widths, slider pointer updates, persisted gesture values, invalid numeric values, and stale tool events. An isolated native macOS run confirmed the Type icon, decimal NumberInput, and continuous opacity Slider in the tool hover card. Screenshots and state are excluded under `target/annotation-edit-review/`; the review window that received user interaction is preserved. Windows/Linux native checks remain unperformed. Text placement and page selection are implemented in the second phase of [work package 15](15-direct-annotation-editing.md).

## October 10 direct sidecar annotation editing

Text annotations now start at the clicked page position with an empty native input. Enter commits one sidecar item and Escape cancels; IME marked composition never commits partial text. Existing sidecar items can be selected on the page, moved within page bounds, edited, styled, or deleted. Selection controls use the annotation color, width, and opacity; text overlays remain page-local and preserve their foreground color and opacity. Numeric, slider, text, and completed-move edits each form one undoable history transaction. Source identity, sidecar fingerprint, transient history revision, item index, and expected value reject stale edits. Existing PDF content remains read-only, and sidecar saves preserve the source PDF bytes.

Direct editing tests cover text creation, Enter/Escape behavior, IME composition, text revision, deletion, movement, style transactions, stale selections, shape hit testing, CropBox/rotation transforms, spread/page-local placement, invalid source/fingerprint handling, and sidecar save integrity. The isolated locked candidate passes `cargo fmt --all -- --check`, workspace tests (126 passed, three existing PDF probes ignored), `cargo clippy --workspace --all-targets --locked -- -D warnings`, and `cargo build --locked -p breeze-app`. Native captures under `target/annotation-edit-review/` show the inline text editor, selected annotation controls, Type icon, NumberInput, and Slider. Native automation could not reliably activate the final input window; real component tests cover the complete interaction. Windows/Linux native checks and the existing production fidelity/release gates remain open.

## October 11 right document sidebar layout

The document sidebar now sits beside the complete reader content below the title bar, matching the left navigation panel's vertical placement. Its header aligns with the 38 px reader toolbar. Information and Annotations are separate icon buttons; the reader toolbar's sidebar button remains selected while the panel is open, and the active content button is selected when search is not taking over the panel.

Document text search now opens the right sidebar automatically. Its input, confirm button, and close button occupy the right sidebar toolbar in place of Information and Annotations; results remain below that toolbar. The left Outline/Thumbnails navigation remains unchanged while search is active. Opening search records whether the right sidebar was already open: closing with X, Search, or Escape restores the prior open/closed state. The sidebar toggle cancels active search before closing the panel.

Formatting, the complete workspace suite (96 passed, three existing PDF probes ignored), and Clippy with warnings denied pass. Focused component tests cover uniform icon spacing, top alignment, narrow-window bounds, icon-button switching, toolbar search controls, automatic right-sidebar search, search-result placement, Escape dismissal, and restoration from both sidebar states. Native macOS capture under `target/right-sidebar-review/` confirms the aligned right panel and evenly spaced Information/Annotations buttons; search-toolbar interaction remains covered by component tests. Windows/Linux native checks and the existing production fidelity/release gates remain open.
## October 11 interface languages

The language preview is now a native Kit Select offering English, Simplified Chinese,
and Traditional Chinese. Application-owned chrome, menus, tooltips, input placeholders,
annotation tools and new notices use a compiled catalog. Theme groups and existing
inputs refresh immediately; filenames, user category names, document content and
official theme names are preserved. The three choices persist using the existing
numeric preference mapping (0/1 retained, 2 added for Traditional Chinese). Kit's own
locale follows the selection. Platform dialogs and underlying library/OS diagnostic
details can retain their platform or dependency language; previously emitted notices
retain the language in which they were produced.

Formatting, workspace tests (143 passed, three existing PDF probes ignored), all-target
Clippy with warnings denied, build and isolated `cargo run --locked -p breeze-app`
startup pass. The isolated commit candidate passes 131 workspace tests (the same
three probes ignored), formatting and Clippy. New tests cover catalog/parameter completeness, actual dropdown keyboard
selection, live chrome/input refresh, user-content preservation and persisted restore.
Native macOS interaction with the isolated review bundle verified all three dropdown
choices and complete settings layouts in English and both Chinese variants. The
review data directory is `target/language-update-review/native/`. Windows/Linux native
localization checks remain unperformed. Release checks are the second phase of
[work package 17](17-language-and-updates.md).

## October 11 stable-release checks

Settings now checks the latest stable GitHub Release for `poneding/breeze`, the future
repository supplied by the user. Manual remains the default. Automatic checks once
after startup restoration or when first enabled in the session; Never prevents
background checks. A manual check remains available in every mode. Policy persists;
results are session-only. A newer version shows a dismissible notification outside
Settings and a button to open its release page. No download, installation, remote
creation or publication occurs.

The background Rust HTTP client enforces HTTPS, a 15-second total deadline (including
body reads), a 1 MiB metadata limit, no redirects, and one in-flight request. SemVer
comparison handles numeric ordering, optional v/V tags, prereleases and build metadata.
Only a validated stable tag can produce a link, constructed under the fixed GitHub
repository. Missing releases/repositories, invalid metadata, network errors, rate
limits and HTTP failures cannot produce an up-to-date result. An automatic result
is discarded if background checks were disabled while its request was running.

Validation: formatting, 148 workspace tests (three existing PDF probes ignored),
all-target Clippy with warnings denied, and the debug build pass. Five new tests
exercise version ordering and link validation, real loopback HTTP headers/statuses,
response-size and body-read timeouts, automatic/manual/never policy and in-flight
bounds, and real Kit loading/result/banner interactions across languages. Native macOS startup used `cargo run --locked -p breeze-app` with isolated state.
The review bundle restored Traditional Chinese across restart, performed a real
HTTPS check, showed GitHub's actual rate-limit failure (HTTP 403 with remaining
quota zero), re-enabled Check Now, and relocalized that result when switched to
English. Both update layouts were inspected without clipping. The English reader was also
inspected with an isolated bundled-sample copy, including navigation, toolbar and
status labels. The future repository
has no user-published release yet; successful/newer-release and missing-release
responses are verified with local HTTP fixtures, not a claimed live publication.
Windows/Linux native checks remain unperformed.

The isolated release-check commit candidate passes 136 workspace tests (the same
three existing probes ignored), formatting and all-target Clippy. The control test
uses an actual simulated 900×650 viewport and verifies the check button remains
inside it. Only the four HTTP-client support packages and the required Rustls log
feature were added to the lockfile; unrelated locked resolutions were retained.

## October 12 left navigation button state

The left navigation toolbar button now derives its selected appearance from the
existing sidebar open state, matching the right document-sidebar toggle. Clicking
the toggle, using its shortcut, opening Outline/Thumbnails, and restoring saved
sidebar state all use that same state.

Formatting, the full working-checkout workspace suite (142 passed; three existing
PDF probes ignored), and all-target Clippy with warnings denied pass. Existing
component tests cover shortcut toggling and collapse/width restoration. An isolated
`cargo run --locked -p breeze-app` launched on macOS with state under
`target/navigation-toggle-review/native/`. The initial native visual check was blocked
by a locked Mac. A subsequent isolated review under `target/search-close-review/native/`
confirmed the selected appearance while expanded, its removal after clicking to
collapse, and restoration after Command+B reopened navigation. Windows/Linux native
checks remain unperformed.

## October 12 simplified document search toolbar

The right-sidebar search toolbar now contains only the expanding search input and
its Close icon button. The separate Search confirmation button is removed; Enter
continues to submit the query through the existing input event handler.

Formatting, the full working-checkout workspace suite (142 passed; three existing
PDF probes ignored), and all-target Clippy with warnings denied pass. Isolated
`cargo run --locked -p breeze-app` startup and native macOS review confirmed the
input filling the toolbar beside Close, Enter returning document matches, and
Close restoring the previously closed right sidebar. Review state is under
`target/search-close-review/native/`; the review window is retained after user
interaction. Windows/Linux native checks remain unperformed.


## October 12 titlebar tab compression and hover Close

Titlebar content and its document scroller now have zero flex bases so Kit's enclosing bar cannot request the combined intrinsic width of every tab. Home and the grouped Open/theme/Settings controls retain their widths, and Kit's Windows/Linux window-control sibling keeps its own space. Document tabs share the remaining width equally up to 228 px, compress to 40 px, then scroll only within their bounded region. The 4 px leading shadow inset is retained.

Close is now an absolute 18.5 px square overlay with no reserved text slot. On hover, a 12 px transparent-to-surface gradient precedes a solid area around Close, hiding text without changing its layout. The fade uses the same composited hover color as the tab, including dark-theme opacity. Independent Close feedback, click propagation, keyboard closing, the exact theme active background, and the stronger active shadow are retained. This phase includes the previously pending title-tab surface and Close refinements as their required base.

The complete working checkout passes formatting, 143 workspace tests (three existing PDF probes ignored), and Clippy with warnings denied. The isolated titlebar phase on the committed dependency versions also passes formatting, 137 workspace tests (the same three ignored), and Clippy. Component regressions cover 3/16/60 tabs at 298–1000 px, equal compression, bounded overflow, reachable global buttons, and stable full-width titles overlaid by Close. Intermediate widths model the 102 px consumed by all three Kit window buttons; this is geometry coverage, not native Windows/Linux validation. Isolated macOS startup and captures of 16 long titles confirmed compression at 1340 px and the 900 px native minimum, separate global controls, light/dark hover fades, active-tab fading, and independent Close hover. Native Windows/Linux checks remain unperformed. Orca is unavailable; native inspection used the existing macOS window-capture and CoreGraphics review helper. Review copies/state/screenshots are under `target/titlebar-tab-density/` and excluded from version control.


## October 12 add-document control after tabs

A 1 × 14 px theme-border divider and a Small Lucide Plus button now follow the document-tab scroller. Plus stays visible outside the scroll clip and opens the existing document picker through the Open action and its shortcut tooltip. Its idle foreground is the theme's muted foreground; Kit's ordinary ghost-button hover restores the normal icon foreground and background. The former Open button is removed from the right-side group, which now contains only theme and Settings. The tab region is capped at its preferred total width so Plus follows the last tab even when only one document is open; compression and bounded overflow are retained. The control remains available with no open tabs.

Formatting, the working-checkout suite at verification time (143 passed, three existing PDF probes ignored), and all-target Clippy with warnings denied passed. Subsequent unrelated in-progress annotation changes temporarily prevented a new working-checkout startup, so the isolated phase on committed dependencies was also checked: 137 workspace tests (the same three ignored) and Clippy pass. Real-component coverage checks one, three, 16, and 60 tabs plus an empty strip, divider/Plus position, action-group bounds, removal of the old Open target, and Plus opening the native path prompt. Isolated native macOS startup and captures confirmed Plus following a single tab, staying visible after 16 compressed tabs, muted idle versus normal hover styling, and no Open icon in the right-side group. Native Windows/Linux checks remain unperformed. Review files are under `target/titlebar-add-document/` and excluded from version control.


## October 12 view rotation and immersive fullscreen

Rotate Left and Rotate Right sit immediately left of the layout menu in the status bar, and Fullscreen sits at its right edge. Rotation is view-only and per document: each click advances one clockwise or counterclockwise quarter turn, the result persists with the document, and the source PDF is never rewritten. A rotated page keeps its own `/Rotate`; the reader renders raster tiles and navigation thumbnails in the rotated view space, so a quarter turn exchanges the displayed width and height. Annotation geometry, text selection, and search destinations continue to use unrotated PDF user space and are mapped through the combined rotation, so existing highlights, notes, and gestures stay aligned.

Fullscreen requests native window fullscreen and renders only the document: the title bar, navigation panel, document sidebar, reader toolbar, and status bar are all removed and the page area uses the entire window, refitting fit modes to the larger area. F11 toggles the mode in both directions; Escape leaves it unless a text annotation is being edited. Per-document layout, zoom, and rotation are preserved, so a turned document stays turned in fullscreen.

`breeze-core` owns the `Rotation` value, the `ViewGeometry` mapping, and the per-document `ReaderState.rotation` field; `breeze-pdf` maps a view tile onto its axis-aligned source region and rotates rendered pixels off the UI thread, with rotation included in the tile identity and the worker cache key. The view rotation persists per document; snapshots written before it existed load unrotated because the field defaults. New coverage: rotation composition and point mapping, tile-region bounds and out-of-page rejection, pixel rotation of full pages and tiles, 90° tile reassembly against an independently rotated page, and component tests for the status controls, the width/height exchange, per-document restoration, immersive chrome removal, and both exit keys.

Verification status: an isolated tree containing the committed dependencies plus this feature passed formatting, the full workspace suite (119 app, 14 core, 23 PDF, 4 PDF integration, and 7 storage tests; three existing PDF probes ignored) and all-target Clippy with warnings denied. That tree deliberately excluded the neighboring phases that were still uncommitted at the time. With those phases committed, the complete checkout now also passes formatting, the workspace suite (131 app, 14 core, 23 PDF, 7 PDF integration, and 7 storage tests; the same three probes ignored) and all-target Clippy with warnings denied, including the rotation and fullscreen component tests, and an isolated `cargo run` with `BREEZE_DATA_DIR` stayed up. Native macOS visual inspection of immersive reading remains unperformed because the session reports `CGSSessionScreenIsLocked=true`, which blocks window capture and interaction; do not treat this phase as visually accepted yet.


## October 12 content-sized tabs and continuous hover shadows

Document-tab preferred widths now use Kit's Small text size and the active UI font to shape each filename, plus the actual icon/gap/padding widths. Short Latin and Unicode names remain compact rather than stretching to 228 px. Longer titles keep that maximum; tabs shrink from their preferred widths down to 40 px and retain bounded overflow. The Close overlay still reserves no layout space. The scroller cap is the sum of those preferred widths, keeping the divider and Plus beside the last tab.

Inactive tabs now use the same `shadow_sm` as active tabs while their hover group is active, including over Close. The tab scroller retains its 4 px leading inset and gains a 6 px trailing inset so the final active/hover shadow stays within the clip and clear of the trailing divider.

The isolated phase on the current committed base passes formatting, 168 workspace tests (three existing PDF probes ignored), and all-target Clippy with warnings denied. Regressions cover short Latin/Chinese names beside a maximum-width title, shrinking in a narrow viewport, stable hover/Close geometry, the divider's shadow clearance, 1/3/16/60 tabs, and reachable titlebar buttons. A native macOS run with isolated state confirmed short versus long widths, the last active tab's uncut shadow, inactive hover and Close-hover shadows, and matching dark-theme behavior. Review files remain under `target/titlebar-tab-intrinsic/` and are excluded from version control. The main working checkout's additional in-progress rendering tests prevented a complete combined check and were preserved; they are outside this commit. Windows/Linux native checks remain unperformed.


## October 12 balanced document-tab padding

Document tabs now use equal 0.5 rem side padding (7.5 logical pixels at the current 15 px rem), including when a short filename sets the preferred width. Width measurement includes both sides; the 40–228 px bounds, hover shadows, divider clearance, and Close fade remain in place. Close is inset by that same padding without reserving a layout slot.

Formatting, the working-checkout workspace suite (173 passed, three existing PDF probes ignored), and all-target Clippy with warnings denied pass. Existing component regressions now check equal content insets on short Latin/Chinese and maximum-width titles, plus Close alignment. Isolated-data `cargo run --locked -p breeze-app` startup and a native macOS capture confirmed the balanced short-title spacing. Review data/screenshots are under `target/tab-balanced-padding/` and excluded from version control; native Windows/Linux checks remain unperformed.


## October 12 independent title and Close insets

The title retains a 0.5 rem right inset matching the left icon inset, while the hover Close overlay again sits 2 logical pixels from the tab's right edge. The title's extra padding is applied inside its row so the existing content clip still contains Close. Preferred tab widths, the gradient fade, and hover shadows are unchanged.

Formatting, the workspace suite (173 passed, three existing PDF probes ignored), and all-target Clippy with warnings denied pass. Component regressions check the title inset separately from Close's 2 px alignment. Isolated-data native macOS startup and captures confirmed both the normal short-title spacing and the restored Close-hover position. Review files remain under `target/tab-close-inset/` and are excluded from version control; native Windows/Linux checks remain unperformed.

## October 13 page-turn presentation

The reader now requests one grid-aligned device region per viewport instead of one job per 508-pixel cell, and slices it back into the unchanged per-cell cache. It also predicts the two following pages at their own device scale, within a 96 MiB pixel budget and a 56 MiB prefetch share. `breeze_pdf::MAX_REGION_PIXELS` (12 Mpx) replaces the previous 512x512 `DeviceTile` cap and the renderer's 4 Mpx ceiling as a shared contract.

This addresses the reported block-by-block page turn. A region's painted interior agrees with the separately rendered cells except for a handful of antialiased edge pixels (12-33 bytes of about 2,048,000 per case, maximum 41 of 255), the same tolerance the whole-page comparison already applies to tile seams. Six new component tests and three new `device_tiles` tests cover viewport grouping, grouping under 400% zoom, rotation-scoped grouping, prediction of both following pages, one-result slicing, failure marking for a whole region, the relaxed ceiling, rotated region extents, and the equivalence check at two device scales and four rotations. `cargo fmt --all -- --check`, the workspace suite and all-target Clippy with warnings denied pass.

Release measurements on the bundled sample: one 1195x1546 viewport region renders in 29.07 ms as a single job where the same viewport previously required 12 cell jobs, and pages 1 and 2 are already rendered (15.19 / 14.68 ms) before any turn. `crates/breeze-pdf/examples/region_probe.rs` records the region-versus-cell cost.

Native GUI inspection was **not** performed: the Computer Use path is unavailable in this session (`orca` is not installed), so the visual result rests on the application's own render trace, component tests and probes. Retained page preparation (display list) remains unimplemented, GPU residency is outside the 96 MiB budget, and the 350 MiB RSS target has not been re-measured. Details and open risks are in [work package 16](16-page-turn-presentation.md).

## October 13 automatic language detection and an English-source catalog

[Work package 19](19-language-detection.md) adds automatic detection above a divided
language list. `LanguagePreference` (Auto, English, 简体中文, 繁體中文) is the stored
choice and Select item; Auto resolves through `sys-locale` to Simplified Chinese for
`zh`/`zh-Hans`-style tags, Traditional Chinese for `zh-Hant`/`zh-TW`/`zh-HK`/`zh-MO`,
and English for everything else. The dropdown is one ungrouped Select:
`Auto detect`, a non-selectable `Languages` label row, then the three explicit
languages. The label is a row instead of a section header because a Kit Select
measures only its first section header and applies that height to every header, so
a header above detection would also reserve an empty band of the same height. The
label row is disabled and committing it keeps the current language, so it never
becomes the selected value. The catalog is now keyed on English per
AGENTS.md: `text_in` falls back to the English key, four near-synonym Chinese keys
that shared one English meaning were merged, and every `tr!`/`tf!` call site uses the
English literal.

State version 9 migrates the version 1–8 numeric language preference so an existing
choice is preserved (0/1/2 = Simplified Chinese/English/Traditional Chinese map onto
the new 2/1/3 indices); Auto is the default only for a fresh state. Tests resolve
detection to Simplified Chinese so their expected chrome does not depend on the
machine's locale, and the locale matcher is unit-tested directly. Formatting, the
workspace suite (136 app tests, three existing PDF probes ignored), all-target Clippy
with warnings denied, and the locked build pass. New tests cover locale matching,
preference round-tripping, the dropdown's rows and the label veto, the version 8
to 9 migration, catalog key uniqueness and slot parity, and live Select refresh.
Native macOS checks on the isolated review bundle under
`target/lang-detection-review/` confirmed Auto detection in Simplified Chinese on
this system, English under an `en-US` language preference, and the stored English
and Traditional Chinese choices (`Breeze · Local reading workspace`,
`Breeze · 本地閱讀工作台`) with English chrome labels. The session lacks the
screen-recording permission, so the open dropdown's label row was verified by
component tests rather than a pixel capture. Windows/Linux native localization
checks and the existing production fidelity and release gates remain open.

## October 13 chrome refinements: sidebar order, About layout, update label

The Home sidebar footer now stacks About (Primary+I) above Shortcuts (Primary+K),
with Settings (Primary+,) last. The titlebar Settings button is removed; the
sidebar entry and Primary+, remain the ways to open Settings, and the titlebar
keeps Home, the document tabs, Open, and the theme control. The About check action
is labelled "Check for updates" (`检查更新`), matching the Settings policy row, and
the retired "Check now" catalog entry is gone.

The About page's update status now wraps inside a bounded 360 px column and stays
centered below the check button, so a long failure message no longer shares the
button's line or pushes the release button around. Author and Source code render as
single rows: the muted label and its value share one line, and a fixed 64 px label
column left-aligns both values.

Validation: `cargo fmt --all -- --check`, the workspace suite (137 app tests plus
core, PDF, storage and packaging tests; three existing PDF probes ignored), and
`cargo clippy --workspace --all-targets --locked -- -D warnings` pass. Component
tests pin the new sidebar order, the removed titlebar entry (the shortcut still
selects Settings), the wrapped and centered update status, the single-line
left-aligned credits with a shared value column, and the new check label. Native
visual inspection of this pass was blocked: the session lacks the macOS Screen
Recording permission, so `screencapture` returned only the desktop wallpaper and
menu bar and the per-window capture failed with `could not create image from
window`. Windows/Linux runtime checks remain open.

## October 13 status text: remove announcements that carry no information

The interface no longer announces that nothing is wrong. About shows an update
result only after a check produced one, so the untouched "updates have not been
checked" line is gone along with the in-flight line that repeated the check
button's own label. The reader status bar and the annotation panel footer stay
empty while a document is healthy instead of showing "source unchanged" and
"annotations loaded · automatically saved to sidecar". A cancelled file picker,
recovery confirmation or discard no longer leaves a receipt, and the annotation
selection popover lost its "annotation selected" title, which only repeated the
click. Disclosures, error and blocking reasons, progress states, and the undo
hint after a deletion remain.

Validation: `cargo fmt --all -- --check`, `cargo test --workspace --locked` (141
app tests plus the core, PDF, storage and packaging suites; the same pre-existing
probes stay ignored) and `cargo clippy --workspace --all-targets --locked -- -D
warnings` pass. New component tests pin the silent update state, the empty
annotation footer while the sidecar is ready and saved, and the empty status bar
on a healthy document. Native visual inspection was not performed for this pass;
Windows/Linux checks remain open.

## October 13 manual annotation saving

Requested by the user on October 13, 2026: annotations must no longer save
themselves. The 500 ms debounce (`schedule_annotation_save` and
`DocumentAnnotations::autosave`) and every call site are gone — created, edited,
recolored, erased and deleted annotations, undo/redo, annotation-list removal,
and the follow-up write after a completed save. An edit now marks the document
unsaved, which the reader status area and the annotation panel footer report;
Save (Primary+S) and Save As are the only paths that write a sidecar, and a
cancelled Save As no longer starts a follow-up write. Reassociation and recovery
copies still write at once, because both are explicit actions on a file the user
chose. Close/quit, tab close and library removal remain refused while
annotations are unsaved, so pending work is not discarded silently; nothing is
written before the user saves, so an unexpected termination can cost that
session's annotations. Details are in [work package 21](21-manual-annotation-save.md).

Validation: `cargo fmt --all -- --check`, `cargo test --workspace --locked` (142
app tests plus the core, PDF, storage and packaging suites; the same pre-existing
probes stay ignored) and `cargo clippy --workspace --all-targets --locked -- -D
warnings` pass. `annotations::tests::edits_stay_unsaved_until_the_user_saves`
pins the new behavior: after an edit and an advanced clock the sidecar still
holds the previous snapshot, the document stays dirty and `allow_close` refuses,
and only an explicit save writes both annotations. Native visual inspection was
not performed for this pass; Windows/Linux checks remain open.

## October 14 panel toggles move into their panel headers

The reader toolbar now carries each panel toggle only while that panel is
collapsed. Opening the left navigation panel moves `panel-left` into the
navigation header's left edge, and opening the document sidebar moves
`panel-right` into that header's right edge, so each toggle keeps the window
edge it occupies while its panel is collapsed; collapsing a panel returns its
toggle to the toolbar. The Outline/Thumbnails tabs are now icon-only Kit
Segmented tabs (`table-of-contents` / `images`) with accessible labels and
action tooltips. Two equal-width header slots center the tab group regardless
of how wide the Outline-only search and fold controls become, and the same
right-edge toggle placement holds while document search occupies the sidebar
header.

Validation: in an isolated copy of the committed tree plus this change,
formatting, the complete app suite (144 passed, one existing probe ignored) and
all-target Clippy passed. At that initial check, the working checkout's MuPDF
adapter did not compile (`mupdf::PdfPage` was unresolved, `export.rs` still
called `pdf_oxide`, and `PdfDocument::open` rejected `PathBuf`), so locked
checks could not run there; those migration diagnostics were resolved later,
as recorded in the current status at the top of this file. The current local
Rust 1.99 Clippy exception is also recorded there. New and updated component
tests cover the toggle's panel-header
placement, its toolbar fallback, the centered icon-only tabs, and the search
toolbar sharing the right header row. Native visual inspection was not
performed because a Breeze instance was already running (immersive fullscreen)
during the pass; Windows/Linux native checks remain open.

## October 14 unified row hover and text-only category row actions

Document category rows and library rows/covers now share Kit's ghost-button hover
fill, the theme accent at 50% opacity in dark mode, instead of the list-active
tint; document tabs and sidebar navigation buttons already used it, and the
selected category keeps the list-active fill. The category row's edit and delete
buttons no longer paint a hover or active background: their icons rest at the
muted foreground and take the panel foreground (edit) or the theme danger color
(delete) only while that button itself is hovered. The delete action also uses a
2 logical pixel right inset instead of the full row padding, placing its icon
close to the panel edge near the left icon inset.

Validation: in an isolated copy of the committed tree plus this change,
formatting, the complete workspace suite (144 app tests with one ignored, 14
core, 21 PDF with three ignored, 6 PDF integration, 3 of 4 render-integrity with
one ignored, and 7 storage) and all-target Clippy pass, the latter with the
pre-existing `chunks_exact_to_as_chunks` lint in `breeze-pdf/src/lib.rs` allowed.
The working checkout's in-progress MuPDF migration does not compile
(`mupdf::PdfPage` is unresolved, `export.rs` still calls `pdf_oxide`, and
`PdfDocument::open` rejects `PathBuf`), so `--locked` checks cannot run there
yet. The changed component test also asserts the tightened delete inset against
the row's right edge. Native visual inspection was not performed because the
session lacks Screen Recording permission (`CGPreflightScreenCaptureAccess` is
false and `screencapture` returns only the desktop wallpaper); the isolated
app's window and startup were confirmed, but its hover states could not be
captured. Windows/Linux native checks remain open.

## October 14 category forms confirm with Enter and muted row actions

Category creation and rename now confirm with Enter. Kit's Popover binds `enter`
to its `Confirm` action and dispatches it before the input's own submit event,
so the editor handles `Confirm` on the popover content, saves, and stops
propagation; an invalid name keeps the editor open with its message instead of
dismissing an unsaved form. The category input's own `PressEnter` subscription
was removed because it could only ever arrive after the popover had closed.

The category row's edit and delete icons rest at the muted foreground and take
the panel foreground only while that button itself is hovered, with no hover
background. This withdraws the danger-colored delete hover described above.

Validation: formatting, the complete workspace suite (146 app tests with one
ignored, 14 core, 23 PDF with three ignored, 6 PDF integration, and 7 storage)
and all-target Clippy with warnings denied pass in the working checkout; the
MuPDF migration has since landed there, so the `--locked` checks run again.
`category_forms_confirm_with_enter` covers Enter creating a category, Enter
renaming it, and an invalid name keeping the editor open. Native visual
inspection of the hover colors was not performed because this session lacks
Screen Recording permission. Windows/Linux native checks remain open.

## October 15 reveal documents in the platform file manager

Library list rows and grid cards drop the explicit “阅读文档” action — the row
and card themselves still open the document — and gain a folder-open action that
reveals a file-backed document in the platform file manager. The label is
resolved per platform: “在访达中显示” on macOS, “在文件资源管理器中显示” on
Windows, and “在文件管理器中显示” elsewhere. macOS runs `open -R`, Windows runs
`explorer /select,<path>`, and other systems open the containing folder with
`xdg-open`; the launcher runs on a background thread and its stdout is not
consumed. The in-binary sample has no file to reveal, so the action is omitted
for it rather than shown disabled. A missing file or a failed launcher reports
“无法显示 …” in the library message area instead of failing silently.

Validation: `cargo fmt --all -- --check`, `cargo test --workspace --locked`
(passed; app suite 153 passed with the new suite) and `cargo clippy
--workspace --all-targets --locked -- -D warnings` pass. Component tests assert
that the old Open button is gone, that the reveal button appears only for
file-backed documents, that its accessible label is the platform term, and that
a document whose file disappeared reports the failure in the library message
area. Unit tests pin the per-platform command line and the catalog entries for
all three labels. A native macOS screenshot of an isolated data directory
confirmed the list row now carries favorite, folder-open, and remove only, with
no “阅读文档” action and no layout change to the category/location columns. The
Finder hand-off itself was not clicked from the running application, so the
launcher is verified by its command line and by the failure path rather than by
an observed Finder selection; Windows/Linux stay unverified.

## October 15 titlebar theme button removed

The right-hand titlebar group held one control, the theme-cycle button, and it
was the only reason that group existed. The button and its group are gone, so
the titlebar is now Home, the document tabs, and the divider/Plus control, and
the Settings dropdown is the single theme control in the interface. The
`CycleTheme` command, its Primary+Shift+T binding, and its entry on the
Shortcuts page are unchanged; only the button, its monitor/moon/sun icon, and
the retired tooltip string were removed from the catalog.

Validation: `cargo fmt --all -- --check`, `cargo test --workspace --locked`
(app suite 153 passed) and `cargo clippy --workspace --all-targets --locked --
-D warnings` pass. `theme_selection_and_settings_dropdown_stay_in_sync` now
cycles with the shortcut instead of clicking the button, and the tab-compression
test asserts the tab strip and Plus control stay inside `titlebar-content` with
no trailing action group. Native visual inspection of the resulting titlebar was
not performed in this session; Windows/Linux native checks remain open.

## October 15 theme preview while moving the dropdown cursor

Moving the Settings theme dropdown's cursor with the up/down arrow keys now
applies the highlighted theme immediately, so the effect can be judged before
committing to it. The preference is still written only when a row is confirmed
with Enter or a click; Escape, an outside click, focus loss, and leaving the
Settings page drop an unconfirmed preview and restore the committed theme, and
the trigger plus the row checkmark keep showing the committed choice throughout.

Kit's `SelectState` reports its cursor only as an `IndexPath`, and a search query
renumbers the rows before that index is read. `appearance::ThemeChoices` is
therefore a real `SelectDelegate` that keeps the filtered view — grouped like
Kit's own grouped filtering, so a matching group title still lists its rows — and
`theme_at(IndexPath)` resolves the highlighted row in the same coordinates the
cursor uses. Arrow keys schedule the preview through `Context::defer_in`, because
Kit's list handler is what moves the cursor and it runs after element listeners.

Validation: `cargo fmt --all -- --check`, `cargo test --workspace --locked`
(app suite 156 passed with two new tests) and `cargo clippy --workspace
--all-targets --locked -- -D warnings` pass. Component tests cover preview-then
-Escape, preview-then-Enter, reopening the dropdown on the committed row, a
confirmed theme staying applied, and a preview dropped when the Home command
unmounts the Settings page. A delegate test pins cursor resolution through a
filtered view. Native verification of the arrow-key preview itself was not
performed: this session lacks Screen Recording permission, and driving the
isolated window through macOS accessibility prompts raised a Developer Tools
password dialog, which was cancelled. Windows/Linux native checks remain open.

## October 15 Markdown scrolling no longer re-borrows the list

Opening a Markdown document and scrolling it aborted the process with
`RefCell already mutably borrowed` from `gpui-pre`'s list state. Kit dispatches a
virtual list's scroll handler while that list mutably borrows its own state, so
recording the reading position through `ListState::logical_scroll_top()`
re-entered the same `RefCell`. The handler now records the block range the event
reports and reads nothing back from the list; the exact list offset is only read
after commands that move the list, which run outside the scroll dispatch. The
per-block text observer also defers its remeasure and notifies the reader
instead of the observed text state, so a text layout pass cannot remeasure the
list while it is borrowed or re-enter its own observer.

Validation: `cargo fmt --all -- --check`, `cargo test --workspace --offline`
(app suite 165 passed with the new regression test, 16 core, 30 PDF with three
ignored, 1+1+8+5+1+3+4+7 in the remaining suites) and `cargo clippy --workspace
--all-targets --offline -- -D warnings` pass. The regression test
`markdown_reader_scrolls_and_records_its_position` opens a 200-block Markdown
document, scrolls, scrolls past the end, jumps through a heading and changes the
text size; reverting only the handler change reproduces the original panic at
`list.rs:561`. The app was also launched natively against an isolated
`BREEZE_DATA_DIR` with the same Markdown file as an argument and stayed running
without a panic, but the scroll gesture itself was not driven through the
native window: this session lacks Screen Recording permission. Windows/Linux
native checks remain open.

## October 15 reflow reader chrome matches the PDF reader

The EPUB and Markdown readers had put the navigation toggle at the right of the
content toolbar, kept search and favorite left of centre, moved text size into
the toolbar, and labelled the outline and information panel headers with words.
They now follow the PDF reader's placement: the toggle sits at the left edge of
its panel header and returns to the toolbar's left edge while collapsed, search
and favorite are right-aligned in the toolbar ahead of the document-panel
toggle, text size occupies the status bar's right end where PDF keeps zoom, and
the navigation header shows the outline icon tab while the document panel header
shows the information icon. A shared `flex_1` slot absorbs the middle of each
row, so the two formats keep the same control order at any window width.

Validation: `cargo fmt --all -- --check`, `cargo test --workspace --offline`
(app suite 166 passed with the new layout test, 16 core, 30 PDF with three
ignored, and the remaining suites) and `cargo clippy --workspace --all-targets
--offline -- -D warnings` pass. `reflow_chrome_keeps_the_reader_layout` asserts
the toggle's left edge in both the panel header and the toolbar, the toolbar's
search/favorite/toggle order and right alignment, that all three text-size
controls resolve inside `reflow-status` and below the toolbar, the outline icon
tab's accessible name, and the information icon at the left of the document
panel header. The app was launched against an isolated `BREEZE_DATA_DIR` with a
Markdown argument and stayed running for about a minute without a panic, but the
resulting chrome was not inspected visually: this session lacks Screen Recording
permission. Windows/Linux native checks remain open.

## October 15 reflow outline, Markdown measure and image references

The EPUB and Markdown navigation panel now uses the PDF outline's own row
geometry and controls instead of a reduced list: 24 px rows, 8 px plus 10 px per
depth of indentation, the active row highlighted, the title as its tooltip, a
nesting chevron that folds children, the outline title search on the header's
right, and the expand/collapse menu when the outline is nested. The reflow
outline list has its own element id, so a library that switches between a PDF and
a reflowable document cannot inherit the other list's cached measurements.

Markdown no longer offers page-turn buttons — it scrolls — and its status bar
carries a stateful reading-width toggle instead: `chevrons-left-right` switches
from the centred 760 px measure to the full window width and `chevrons-right-left`
switches back, per document and persisted with its reading state.

A Markdown image reference is a URI, not a path. Opening the literal text meant a
name written as `my%20shot.jpg` never found `my shot.jpg`, so the reader reported
"Image unavailable" for a picture that existed. References are now percent-decoded
and stripped of a query or fragment before the file is opened, and an image that
still cannot be shown no longer writes to the reader's status notice: it is
counted in the document panel as "Images not shown", cached per URI so it is not
retried every frame, and other images keep loading because the single-flight
`busy` flag was replaced with per-URI tracking.

Validation: `cargo fmt --all -- --check`, `cargo test --workspace --offline` (app
suite 170 passed with three new component tests and one new adapter test, 16
core, 30 PDF with three ignored, and the remaining suites) and `cargo clippy
--workspace --all-targets --offline -- -D warnings` pass.
`reflow_outline_matches_the_pdf_outline_style` renders a nested PDF outline and a
nested Markdown outline in the same window and asserts identical row origin,
height and chevron geometry plus the shared header controls and folding;
`markdown_has_no_paging_and_toggles_its_reading_width` asserts the status bar has
no page-turn control, that the toggle reports its state, and that the measured
column widens when enabled; `markdown_shows_images_named_with_spaces` decodes a
real JPEG named with a space; `markdown_image_failure_stays_out_of_the_status_bar`
asserts a missing or remote image leaves the status bar empty. Reverting only the
resolver reproduces `my%20shot.jpg: No such file or directory`, and reverting only
the scroll-handler change reproduces the earlier list borrow panic. The app was
also launched against an isolated `BREEZE_DATA_DIR` with a Markdown file holding
that image and stayed running without a panic; the resulting chrome and image were
not inspected visually because this session lacks Screen Recording permission.
Windows/Linux native checks remain open.

## October 14 category action hover follows the pointer directly

The category row's edit and delete icons take the panel foreground from the
pointer's own position instead of from a hover style. GPUI resolves an element's
hover against the previous frame's hit boxes, so a button that only exists while
its row is hovered never matched the pointer that revealed it: the icon stayed
muted until the pointer moved again, which is what the user reported. Each row's
action row now records its laid-out bounds during prepaint, and the render
compares the live pointer against them, so the first revealed frame already draws
the hovered icon in the panel foreground.

`category_action_hover` is the pure half of that decision, which end of the
action row the pointer occupies and nothing when the pointer is outside it, and
it is unit tested. The reported scenario was reproduced with a temporary probe
that mapped the decision onto the delete button's width: moving the pointer
straight onto the button in one event left it at its resting width before this
change and yields the hovered width on that first frame after it.

Validation: formatting, the complete workspace suite (172 app, 16 core, 30 PDF
with three ignored, and every PDF integration suite) and all-target Clippy with
warnings denied pass in the working checkout. Native visual inspection of the
colours was not performed because this session lacks Screen Recording permission.
Windows/Linux native checks remain open.

## October 15 EPUB chapter resources and page-turn latency

EPUB reading had two real defects behind the reported "sparse, broken layout"
and "slow page turns".

Layout: the adapter repackages each chapter as a temporary publication for
MuPDF, and resolved a reference by comparing its percent-encoded URL path against
the package manifest's decoded hrefs. Any stylesheet, image or font whose name
contained a space or a non-ASCII character — the ordinary case, because editors
write `my%20shot.jpg` — was therefore dropped. A resource the manifest did not
declare was dropped too, as were at-imported stylesheets and `url(...)` inside a
`style` attribute. The chapter consequently reached the layout engine with no CSS
at all, which is exactly what a sparse, disordered page looks like. Lookups now
decode first, fall back to the archive for undeclared files, infer a media type
from the extension, follow `@import` and `@font-face`, scan style attributes, and
carry the publication's language and page-progression-direction into the
generated package.

Latency: three separate costs. Every wheel event launched its own full-page
render, so a single trackpad swipe superseded itself dozens of times; scrolling
now accumulates until a third of a viewport has been crossed and requests one
turn (the magnitude is honoured, so a fast flick still crosses several pages).
Each delivered frame copied ten megabytes of pixels on the UI thread to build the
texture; the buffer is now owned and moved. And every turn waited for a fresh
rasterization of the whole page; the worker now keeps a four-page, 96 MiB raster
cache and prefetches the next page after a frame is shown.

Validation: `cargo fmt --all -- --check`, `cargo test --workspace --offline` (app
suite 173 passed, 16 core, 30 PDF with three ignored, 6 reflow unit, 5 reflow
integration, and the remaining suites) and `cargo clippy --workspace
--all-targets --offline -- -D warnings` pass.
`encoded_references_keep_their_stylesheets_images_and_fonts` and
`undeclared_but_present_assets_are_still_packaged` assert the packaged chapter
contains a percent-encoded stylesheet, its `@import`, a percent-encoded image
reachable from both a `link` and a `style` attribute, and an `@font-face` source;
`page_delta_moves_the_requested_number_of_pages` pins that a delta is a count, not
a direction; `epub_wheel_turns_pages_without_a_render_per_event` asserts a partial
scroll launches no render and a full page of scrolling advances exactly one page.
Release probe, Apple M1 Pro, macOS 15.8.1, 600×800 viewport at scale 2, font 18,
100-chapter authored fixture: first open 303 ms; warm page (cached layout,
re-rasterized) p50 60 ms, p95 69 ms; a chapter holds 7 pages at about 1035
characters per page; a prefetched page turn returns in p95 0.26 ms where an
unprefetched one waits p95 63 ms. The app was launched against an isolated
`BREEZE_DATA_DIR` with a six-chapter Chinese fixture whose stylesheet name
contains a space and stayed running; the rendered page was not inspected visually
because this session lacks Screen Recording permission. Windows/Linux native
checks remain open.

## October 15 CJK font: the real cause of sparse, slow EPUB reading

Reporting "the layout is disordered and sparse" and "page turns are slow" had one
root cause. `mupdf-sys` compiles no CJK font at all — its build script sets
`TOFU_CJK` and prunes Droid and Source Han to stay under the registry's package
limit — and the fallback belongs to `mupdf`'s `SystemFontLoader`, whose CJK hook
is implemented for Windows only. On macOS and Linux the workspace had
`base14-fonts` and `system-fonts` and nothing else, so every Han character became
a failed font lookup: the glyph was never drawn, and the lookup cost roughly a
millisecond, which is why a page of Chinese prose rendered as scattered Latin
words and digits and why each page took well over a second.

Enabling `bundled-fonts-droid` and `bundled-fonts-noto` installs MuPDF's runtime
font hooks for CJK and symbol coverage. Measured on the reported book (`从零构建大
模型`, 119 chapters, chapter 7 at 1200×900 logical, scale 2) through the adapter,
which produces exactly the texture the reader displays:

| | before | after |
|---|---|---|
| first page of chapter 7 | 4054 ms | 46 ms |
| rasterization ink coverage | 0.72 % (no Han glyphs) | 11.6 % |
| chapter page count | 1 | 2 at 1200×900, 1 at 1240×1160 |
| page turn inside the chapter | 1490 ms | 19 ms (p95 21 ms) |

At the reported window's own content size (1240×1160 logical, 2480×2320 device
pixels) the first page is 46 ms and a turn is p50 19 ms; jumping to a distant
chapter is p50 96 ms, p95 142 ms. A rendering defect in the same path was fixed
along the way: the page was requested with alpha, giving premultiplied
transparency that nothing needed, and the opaque variant returns three channels
where the old code assumed four, so the conversion now builds BGRA from RGB and
adds an opaque alpha byte.

Validation: `cargo fmt --all -- --check`, `cargo test --workspace --offline` (app
suite 173 passed, 16 core, 30 PDF with three ignored, 6 reflow unit, 6 reflow
integration, and the remaining suites) and `cargo clippy --workspace
--all-targets --offline -- -D warnings` pass.
`chinese_prose_is_drawn_with_its_own_stylesheet` renders a Chinese chapter through
the real adapter and asserts the Han characters reach the text layer, that ink
coverage exceeds 3 %, that the raster is opaque, and that both the first page and
a page turn stay under 500 ms. With the two font providers removed the same test
fails at 0.05 % ink, which reproduces the reported symptom exactly. The chapter in
the report was rendered to PNG and inspected: Chinese text now lays out with the
publication's own paragraph indentation, line spacing and justified wrapping.
Driving the native window itself was not possible — this session lacks Screen
Recording permission — and Windows/Linux remain unverified, although the CJK hook
gap this fixed is macOS/Linux-specific.

## October 14 category selection uses the navigation entry fill

A selected category row now paints the same fill as the other navigation
entries. It was the only sidebar entry that mixed the list-active colour, a blue
tint, into its selected state, because that colour was passed to a Custom button
variant; the sibling entries are Kit ghost buttons, whose selected fill is the
secondary-active token. The category row is now the same ghost control, so
selection, hover, and their text colours all resolve from the tokens the other
entries use, the selected fill is no longer overridden by the row's own group
hover, and `Palette` loses its now-unused `tint` field.

Validation: formatting, the complete workspace suite (173 app, 16 core, 30 PDF
with three ignored, and every PDF integration suite) and all-target Clippy with
warnings denied pass in the working checkout. Native visual inspection was not
performed because this session lacks Screen Recording permission. Windows/Linux
native checks remain open.

## October 15 native window verification of the EPUB reader

The reader was driven through its real window, not only through the adapter: a
release build in a throwaway `.app` bundle with its own bundle identifier and an
isolated `BREEZE_DATA_DIR`, opened on the book that was reported broken (`从零构建
大模型`, 119 chapters, 24.6 MB).

What the window showed: the cover page renders its Chinese title, the author line
and the pipeline figure; chapter 7 renders the Chinese prose with the
publication's paragraph indentation, line spacing and justified wrapping; a
figure page renders its JPEG and the caption beneath it; the chapter arrows move
through the book and the status bar reports the chapter ("7 / 119"); the
information panel reports the title, author, chapter count and reading progress.
Ten scroll steps completed in 477 ms including the harness round trip. None of
this is what a sparse page looked like before: the same chapter previously drew
its Latin words and digits over 0.72 % ink with no Han glyph at all.

Driving the window also found a defect the adapter tests could not see. Clicking
the chapter arrows faster than a chapter renders dropped clicks: five rapid
clicks advanced one chapter. The arrows are absolute jumps, and each click
recomputed its target from the last frame that had landed, so every click after
the first produced the request already in flight and was discarded. The arrows
now anchor on the chapter the reader last asked for, and the status-bar/page-turn
path accumulates its delta the same way.
`rapid_chapter_clicks_all_land` clicks the real toolbar arrow five times without
waiting and requires chapter 5; reverting only the anchor change reproduces
"five rapid clicks landed on chapter Some(1), not 5".

Validation: `cargo fmt --all -- --check`, `cargo test --workspace --offline` (app
suite 174 passed, 16 core, 30 PDF with three ignored, 6 reflow unit, 6 reflow
integration, and the remaining suites) and `cargo clippy --workspace
--all-targets --offline -- -D warnings` pass. Interactive verification stopped
when the display locked; the remaining chapters, Windows and Linux are unverified.

## October 16 EPUB paging lives in the status bar

The EPUB reader had two sets of page controls and reported its position by
chapter. The toolbar's chapter arrows are gone; the status bar's Previous and
Next page buttons are the only ones, they carry the `Previous page` and `Next
page` labels so their tooltips show the Page Up and Page Down bindings beside the
action, they disable at the ends of the book, and the position between them reads
as `page / pages` with no chapter name. Markdown keeps its reading percentage and
its width toggle, since it has no pages.

Pressing Next faster than a page renders also dropped clicks: five rapid presses
advanced one page. A turn is resolved against a landed frame, so the second press
rebuilt the request already in flight and was discarded. The reader now queues
the steps and issues the next one when a frame lands, so every press lands on a
page instead of relying on the engine to cross chapters in one delta.

Validation: `cargo fmt --all -- --check`, `cargo test --workspace --offline` (app
suite passed, 16 core, 30 PDF with three ignored, and the remaining suites) and
`cargo clippy --workspace --all-targets --offline -- -D warnings` pass.
`epub_page_controls_live_in_the_status_bar` asserts the toolbar keeps only its
global commands, that both page buttons sit inside the status bar with the
previous/next-page labels and a resolvable binding each, that the position parses
as `page / pages` and never names a chapter, and that asking to go back from the
first page leaves the reader there. `rapid_page_clicks_all_land` presses the real
status-bar button five times without waiting and requires the fifth page;
reverting only the queue reproduces "five rapid clicks landed on chapter Some(1),
not 5". The window was also driven directly: the tooltip reads "下一页 Page
Down", the position reads `1 / 2`, and six rapid presses advanced six chapters.

## October 16 EPUB two-page reading

The EPUB reader can show a facing pair. The status bar's `columns-2` button is a
view state — pressed while the spread is on, with a tooltip that names the
opposite action — and the choice is stored per document in `ReflowState::spread`
beside the text size, so it survives a reload and older snapshots still load
with single-page reading.

The first implementation combined the pair into one wide raster and handed it to
the reader as a single image. GPUI's image element then measured that wide buffer
as one page and letterboxed it into the content area, so the second page never
appeared at the expected size and the text was scaled and resampled, which is why
it read as a single, larger, blurrier page. The frame now carries one raster per
visible page and the native layout places them side by side as two children, each
`flex_1` of the row; an odd final spread keeps an empty second cell so its single
page stays at half width. Every page is rastered at half the content width times
the device scale, so it maps one to one onto its half of the area instead of
being rescaled by the image element.

`frame.pages` replaces `Frame::bgra`, and the worker's cache accounts for the sum
of the page buffers. A turn still advances a spread rather than a page: `turn`
counts steps, and a page location is snapped to the start of its pair before the
delta applies, so a location saved in single-page mode does not open half way
through a spread. The status bar reports the pair (`3–4 / 12`) and Next is
disabled on the last spread, where the visible pages reach the end of the
chapter. `two_page_reading_returns_a_facing_pair` asserts the two rasters, the
even left page and a two-page turn in the adapter.
`epub_two_page_reading_fills_the_content_area` drives the real window: it asserts
the button is inside the status bar, checks the toggled state through
accessibility metadata, requires two page textures, and requires the settled
double-width raster to match the content width.

Validation: `cargo fmt --all -- --check`, `cargo test --workspace --locked` (app
178 passed, 16 core, 30 PDF with three ignored, 6 reflow unit, 7 reflow
integration, 7 storage, and the remaining suites) and `cargo clippy --workspace
--all-targets --locked -- -D warnings` pass. Native verification with an isolated
`BREEZE_DATA_DIR` on the reported 23 MB Chinese EPUB confirmed the pair renders
side by side, the status bar reads `1–2 / 5`, the toggle is visibly pressed, and
the type size and sharpness match single-page reading. Windows and Linux remain
unverified.

## October 14 unassigned entry uses the ban icon

The sidebar's Unassigned entry now leads with Lucide's `ban` icon instead of the
tag icon, so the entry that means "no category" no longer reads as a category
tag. Per-category rows and the editor's colour swatches keep the tag icon.

Validation: the complete workspace suite passes in the working checkout (194 app,
19 core, 39 PDF with three ignored, and every PDF integration suite), and the
category component tests covering that entry pass. At the time of this entry the
checkout's in-progress markdown editor is unformatted and has two unused items,
so a whole-workspace `cargo fmt -- --check` and all-target Clippy report those
files independently of this change; `categories.rs` itself is rustfmt-clean.
Native visual inspection was not performed because this session lacks Screen
Recording permission. Windows/Linux native checks remain open.
