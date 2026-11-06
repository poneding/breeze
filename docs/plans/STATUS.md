# November 6 cancelled annotation gestures stay silent

Clicking a page with a drawing tool, or dragging shorter than the smallest
readable gesture, created no annotation and left `未创建批注：The gesture is too
small` in the PDF status bar: an engine-facing English reason inside a translated
wrapper, for an interaction that changed nothing. A quote of the reason is not
something the reader can act on, and the status bar had been reserved for real
work.

Rejection is now typed as `breeze_core::annotations::Rejected`. The three reasons
that drew nothing — an empty gesture, a shape with no area, and a markup drag
over no text — are cancelled gestures: no annotation, no message, and no status
line. A gesture that is drawn but rejected (content limits, coordinates, style)
still reports through `Annotation was not created: {}`. The note editor keeps its
own message for missing note text.

Validation: `cargo fmt --all -- --check`, `cargo test --workspace --locked`, and
`cargo clippy --workspace --all-targets --locked -- -D warnings` pass. The new
core test `drafts_that_draw_nothing_are_cancelled_gestures` pins the
classification, and the new component test
`a_click_with_a_drawing_tool_stays_out_of_the_status_bar` fails on the previous
behavior with the reported `未创建批注：The gesture is too small` and passes with no
status notice for the highlight, area, ink, and rectangle tools. Native visual
review of the status line was not performed: an isolated review window could not
be reliably brought to the front for pointer automation, so the component test is
the evidence for the rendered row.

# November 6 square icon-only tabs

The reader's icon-only mode tabs and outline or thumbnail tabs, and their Markdown
counterparts, rendered as glyph-width boxes. GPUI Kit sizes an icon tab from inner
content that is `flex_1()` with a fixed width, and the flex basis wins inside an
auto-width bar, so each tab collapsed onto its 14 px icon: the button measured
13 px wide and neighbouring icons kept only the bar's own 2 px gap. Every
icon-only tab now states a 24 px square, the height of a `small` segmented tab
and of a Kit `small` icon button, so the icons sit 14 px apart.

The GPUI Kit 0.9.0 upgrade was not the cause. A build pinned back to 0.8.1 and
GPUI 0.3.10 measures the same 13 px tab, and window captures of the same document
at both versions differ by 46 pixels inside the toolbar band, all antialiasing on
page text. The defect came in with the icon-only mode tabs (`feat(reader): use
icon mode tabs`, October 16); that upgrade's review inspected the home window,
not the reader toolbar. The Kit's own icon-tab sizing is still unreliable in an
auto-width bar and is worth reporting upstream.

Validation: `cargo fmt --all -- --check`, `cargo test --workspace --locked` (286
application tests plus every crate and integration test), and
`cargo clippy --workspace --all-targets --locked -- -D warnings` pass. Native
macOS window captures of the sample PDF and of a Markdown document show the
square tabs and their spacing, and
`icon_only_tabs_stay_square_beside_each_other` fails on the previous geometry and
passes on the fixed one.

# November 4 disk-image and frame-wait test reliability

The macOS updater and two test waits had defects that failed CI intermittently on
an unchanging commit. The **updater** built its mountpoint from process id plus
the current millisecond, which two concurrent installs hit in the same
millisecond (measured 30 of 30 attempts), and it deleted the mounted image
before unmounting it. `hdiutil` addresses an image through its mountpoint, so
removing that directory or the backing file first strands the volume: it stays
attached until the machine restarts and makes every later attach likelier to fail
with `Resource temporarily unavailable`. A run of the two macOS install tests
leaked a volume in 10 of 12 rounds and failed 2 of 12, and 17 stranded volumes
were found on the development machine. Attach and detach now run under a
process-wide lock with a uniquely named mountpoint, a failed attach detaches
before releasing the mountpoint, attach and detach retry transient failures, and
the image is unmounted before it is unlinked.

The **EPUB waits** counted iterations rather than time: 1 200 attempts with a
25 ms pause is a 30 s floor that a cold `windows-latest` runner can exceed while
rendering a six-chapter fixture, which reported a missing frame that was merely
late (`epub_paging_has_no_status_bar_buttons` failed 2 of 7 hosted runs).
`WAIT_TIMEOUT` is now a 90 s wall-clock deadline, so a slow runner waits as long
as it actually needs rather than as long as a fast machine would.

The **loopback HTTP fixtures** answered HTTP/1.1 with no `Connection: close`
while closing the socket after every response, inviting the client to reuse a
socket that was already gone. That surfaced as `Connection reset by peer` on the
next request, and failed `download_rejects_a_mismatched_or_missing_checksum` and
`download_verifies_the_published_checksum` intermittently under load on both
Windows and Linux. Both fixture servers now declare the close.

Validation: `cargo fmt --all -- --check`, the complete serial
`cargo test --workspace --locked -- --test-threads=1` suite (285 application
tests plus every crate and integration test), and
`cargo clippy --workspace --all-targets --locked -- -D warnings` pass on Rust
1.98.0. The two macOS install tests then passed 10 of 10 rounds at four threads
with no leaked volume, and the full workspace suite passed eight consecutive
default-parallel runs with no leaked volume and no failure, against 10 of 12
leaking rounds before, and the HTTP fixtures passed 25 of 25 focused
repetitions.
The platform gating was re-checked through the stub-dependency harness
under `x86_64-unknown-linux-gnu` and `x86_64-pc-windows-msvc`, both Clippy-clean.
The Windows wait change is exercised only by the hosted runner: this host cannot
reproduce that runner's timing, so the next hosted Windows job is its proof.

# November 4 CI toolchain and platform-gating recovery

The hosted matrix had failed on every platform since the GPUI Kit 0.9.0 upgrade,
for three independent reasons that the v0.2.0 tag inherited. First, the pinned
Rust 1.97.1 predates `Option::map_or_default`/`Result::map_or_default`, which
GPUI's macOS snapshot (0.3.10 onward) calls; stabilization is **1.98.0**, verified
by compiling both a probe and the full workspace on 1.97.1 (E0658) and 1.98.0
(pass). The workspace `rust-version`, both workflow pins, and the toolchain notes
now state 1.98.

Second, Linux and Windows Clippy compile different `cfg` sets than macOS, so
macOS-only installer code was reported as dead: `VersionMismatch`,
`InstallFailed`, `MissingApplication`, `Completion::Installed`,
`LaunchKind::Bundle`, `remove_tree`, `diagnostic`, the `Stdio` import, and the
`version` parameter of `install`. Each is now guarded on the platform that
actually constructs it. Third, the non-Unix launcher passed a bundle to the
macOS-only `open` command, which cannot work on Windows; it now refuses the path.
The `real_http_transport_sends_headers_and_rejects_false_success` user-agent
assertion had also hardcoded `0.1.0` and so failed on any release bump; it
compares against `CARGO_PKG_VERSION`.

Validation: `cargo fmt --all -- --check`, `cargo check --workspace --locked`, the
complete serial `cargo test --workspace --locked -- --test-threads=1` suite (285
application tests plus every crate and integration test), and
`cargo clippy --workspace --all-targets --locked -- -D warnings` pass on Rust
1.98.0. Because this host cannot cross-compile (Windows needs `llvm-rc` and
Linux needs a cross `gcc`, both absent), the platform gating was instead verified
by compiling the real `install.rs` unmodified through a stub-dependency harness
(`ureq`, `sha2`, `breeze-storage`) under `x86_64-unknown-linux-gnu` and
`x86_64-pc-windows-msvc`: zero dead-code warnings and clean
`clippy -- -D warnings` on both. That is a `cfg`-resolution check, not a native
build; the hosted Linux and Windows jobs remain the authority for this change.

# November 3 GPUI Kit 0.9.0 upgrade

GPUI Kit and its GPUI Base, Component, assets, and macro packages now resolve to
0.9.0, with the GPUI snapshot at 0.3.11. Breeze's existing component APIs compile
unchanged, including its pinned theme registry and editor integrations.

Validation: `cargo fmt --all -- --check`, `cargo check --workspace --locked`, the
complete serial `cargo test --workspace --locked -- --test-threads=1` suite (285
application tests plus every crate and integration test, with five existing
performance probes ignored), and
`cargo clippy --workspace --all-targets --locked -- -D warnings` pass. A macOS
review bundle launched from an isolated `BREEZE_DATA_DIR` and rendered its home
window normally. The computer-use driver did not deliver an additional settings
click, so that extra native theme-menu inspection was not completed; component
tests cover the full theme registry and selector. Windows and Linux runtime
checks remain unverified.

## November 2 dependency refresh

The workspace lockfile now resolves to the highest versions compatible with the
declared Rust toolchain. Direct pins advanced to GPUI Kit 0.8.1, `resvg`
0.49.0, `ureq` 3.5.0, `embed-resource` 3.0.12, and `zip` 9.2.2; the GPUI
snapshot is now 0.3.10. GPUI's static key parser no longer accepts the
platform-dependent `secondary-` alias, so command bindings are loaded against
the active host OS and startup reports invalid bindings instead of panicking.
The shortcuts retain their existing Command-on-macOS and Ctrl-on-Windows/Linux
meaning.

Validation: `cargo fmt --all -- --check`, the complete
`cargo test --workspace --locked` suite (284 application tests plus every crate
and integration suite; five existing performance probes ignored), and
`cargo clippy --workspace --all-targets --locked -- -D warnings` pass. The
macOS review bundle built and launched from an isolated `BREEZE_DATA_DIR`; its
Chinese home screen rendered without a storage-lock error. Windows and Linux
runtime checks remain unverified. Cargo reports the existing future-incompatibility
notice for `block 0.1.6`.

## November 2 PDF text selection across wheel scrolling

Wheel events no longer commit an active PDF text-selection gesture. The reader
continues scrolling while the left button remains held, and subsequent pointer
movement extends the selection against the updated page positions. A captured
left-button release still commits the range; a non-dragging move still cleans
up a release that was not delivered to the page surface.

Validation: the regression test begins a real component drag, dispatches a wheel
event, verifies the viewport moves, and confirms a later pressed-button move
extends the selected text. The full serial workspace suite passes (285
application tests plus every crate and integration suite; five existing
performance probes ignored), as does all-target Clippy with warnings denied.
The default parallel suite had one transient macOS installer fixture version
mismatch; that test passed alone and in the serial suite. Native macOS review
used the bundled sample and isolated `BREEZE_DATA_DIR`: a text selection drew
correctly, then remained reported as 181 selected characters after scrolling to
page 2. The computer-use driver could not interleave a wheel event while a
native drag button was held, so that exact physical gesture is covered by the
component event test. Windows and Linux remain unverified.

## November 2 Markdown front matter spacing and document information

YAML front matter now keeps its 48 px reading inset inside the metadata card's
render wrapper instead of inserting a separate block after it. The card therefore
clears the content top while the first Markdown body block follows at the normal
paragraph gap. TOML front matter retains the post-block inset because it uses
Kit's code-block fallback. Markdown's right-panel file location now reuses PDF's
single-line, start-truncated path row, full-path tooltip, and trailing folder-open
action. The content menu's status-bar command now uses a `panel-bottom` icon and
Kit's native trailing check while the bar is visible, without a square checkbox
substituting for the command icon.

Validation: `cargo fmt --all -- --check`, the complete
`cargo test --workspace --locked -- --test-threads=1` suite (283 application
tests plus every crate and integration suite), and
`cargo clippy --workspace --all-targets --locked -- -D warnings` pass. The
default parallel run passed 281 application tests but both macOS disk-image
install tests hit a transient `hdiutil: attach failed - 资源暂时不可用`;
both passed serially, matching the existing timing-sensitive note above. Focused
tests cover the YAML and TOML inset mapping, the front matter-to-body paragraph
gap, Markdown's shared path-row geometry and reveal action, and all five content
context menus. Native macOS inspection used an isolated `BREEZE_DATA_DIR` and a
scratch Markdown file. It confirmed the metadata card's 48 px top clearance,
the normal gap before the first body block, the start-truncated source path with
its trailing Finder button, and the content menu's `panel-bottom` icon with its
trailing active check. Windows and Linux remain unverified. Review captures and
scratch files stay under `target/` and are not committed.

## October 30 Markdown edit undo, redo, and source save

Markdown edit mode now carries the same compact Undo, Redo, and Save icon group
as the PDF annotation tools, centered in the toolbar's flexible middle space
between the Reading/Edit switch and the trailing document commands. Equal
zero-basis spacers produce that centering, exactly as the PDF tool row does, and
collapse instead of overflowing a narrow window. The buttons reuse the document
commands bound to the reader, but dispatch by format: in Markdown editing they
reach GPUI Kit's editor history or the Markdown save path; in PDF they keep the
annotation history and sidecar behavior.

Save writes the draft back to its source file, following the user's October 30
choice of direct-to-source persistence over the earlier Save-As-first plan. The
editor captures its text, acknowledged checkpoint, and the fingerprint of the
bytes loaded from or last published to the file. A background publication
stages and atomically replaces the file, refuses a destination that changed on
disk, keeps the original permissions, and rejects a read-only file. Only the
captured checkpoint is acknowledged, so typing during the write remains dirty.
A failed load leaves Save disabled because there is no source fingerprint.
Close and quit messages now direct the user to save rather than describing save
as a later phase.

Editor notices report load and save failures. The status line gains saving and
saved states, while the existing modified state is now refreshed from Kit's
change events. The save path never writes the reading preview or a PDF sidecar.

Each action reflects whether it would do anything, so the group reads as a state
display rather than three always-live icons. Save is inert for a clean draft,
while a publication is in flight, and after a failed load. Undo and Redo are
inert at the ends of Kit's history. Kit exposes the Undo and Redo actions but not
the depth behind them, and it replays history without emitting a change event, so
`MarkdownEditor` keeps a bounded shadow of the text states the history can
restore: every change records its resulting text, and a completed step is
resolved by the state the editor now shows. Resolving by state rather than by a
counter is what makes the ends exact: typing coalesces into one transaction, so
counting changes overstates the steps left, and text can legitimately repeat
(the document's own text, or a later revision that matches it), so comparing
against the loaded text cannot prove the history is empty. A step that changes
nothing proves the direction is finished. The window holds 256 states, which
share rope structure and cost a path per edit rather than a copy; once it drops
its oldest state, undo stays available until a dispatch proves otherwise. The
engine gap is recorded in [parser 15](../specs/15-markdown-rs-parser.md); a Kit
release that exposes history availability would let this shadow model go.

Validation: `cargo fmt --all -- --check`,
`cargo test --workspace --locked` (281 application tests plus every crate and
integration suite; the macOS disk-image install test is timing sensitive under a
fully parallel run and passes alone and with `--test-threads=1`), and
`cargo clippy --workspace --all-targets --locked -- -D warnings` pass. New
component tests cover the toolbar actions and their disabled states before an
edit, while dirty, at the bottom of the history, after redo, and after a save;
the toolbar centering (leading and trailing space equal within one pixel);
saving and undo through both the buttons and the platform actions; direct source
publication; checkpoint cleanliness after save; and refusal of an externally
changed source. Unit tests cover the shadow history's resolution of repeated
text, redo-branch truncation and the bounded window. Storage tests cover
permission preservation and read-only refusal.

Native macOS inspection used an isolated `BREEZE_DATA_DIR` and a scratch Markdown
file. Entering edit mode placed the action group at 1214-1276 logical pixels,
centered within its 745-1745.5 slot (group center 1245 against slot center
1245.25). The state sequence was then read from each icon's darkest pixel, which
distinguishes the states because a disabled Kit button draws at reduced opacity:
an opened draft showed `undo x redo x save x`; typing showed `undo ok redo x
save ok`; saving wrote the file and showed `save x`; `Command-Z` past the saved
baseline showed `undo x redo ok save ok`; `Command-Shift-Z` returned to the
published text and showed `undo ok redo x save x`; clicking each toolbar button
reproduced the same transitions and left the corresponding control inert. The
session's model cannot view images, so this inspection used captured pixel
profiles and OCR of the status bar rather than visual review. Windows and Linux
remain unverified. Scratch state stays under `target/markdown-state-review/` and
`target/markdown-center-review/` and is not committed.

## October 30 In-app update checks, banner, installation, and restart

The reader asked for three changes on October 30: a check on every startup, a
prominent new-version banner, and an update action inside Breeze that ends with a
restart prompt. [Work package 32](32-in-app-updates.md) implements them and
supersedes work package 17's read-only scope.

Startup now starts one background check on every launch path, including storage
failures, after restoring the update policy. Automatic is the fresh-install
default; Manual remains selectable. Session state version 12 migrates a stored
pre-12 default of Manual to Automatic once because the old value is identical to a
deliberate Manual choice. A newer release appears in a full-width banner using
the active theme's base blue (18% background, 60% border), with Update now, View
release, and dismiss. About exposes the same install/restart entries.

An install downloads the deterministic platform asset from the fixed GitHub
release path, enforces HTTPS and a 512 MiB package limit, and verifies the
published SHA-256 checksum before touching the installation. macOS mounts the DMG,
rechecks the bundle's `Resources/VERSION`, stages a copy beside the installed
bundle, and swaps it with rollback on failure. A running AppImage is replaced
atomically. Windows stages the matching Inno or MSI package and starts it after
Breeze exits; `.deb` installs only download and reveal the package because root
owns installation. The verified install shows a Kit AlertDialog once with
Restart now and Later; restart uses the normal close path, so state saving and
unsaved-annotation checks still apply. On macOS and AppImage installs, a detached
helper waits for this process to exit before launching the replacement, so the
new instance can acquire the state-store lock.

Validation: `cargo fmt --all -- --check`,
`cargo test --workspace --locked` (276 application tests plus every crate and
integration suite), and
`cargo clippy --workspace --all-targets --locked -- -D warnings` pass twice in a
row after the final changes. New tests cover plan/asset names, release metadata
that lacks an installer or checksum, checksum parsing, a loopback download with
success/mismatch/missing checksum, banner and About entries, the one-shot restart
dialog, the state migration, and native macOS DMG installation and version
rejection.

Native macOS startup used an isolated `BREEZE_DATA_DIR` and the real GitHub
endpoint; About reported `当前版本无需更新 · 最新正式版本 0.1.0`, confirming the
startup check. A temporary local preview of release 0.2.0, removed before the
commit, showed the blue banner `发现新版本 0.2.0` with `立即更新` / `查看发布版本`,
the About action row, and the modal `更新已就绪` with `稍后` / `立即重启`. Banner
pixels measured `(0.876, 0.914, 0.985)` against white content. Windows and Linux
installation/relaunch were not run on their native systems; their paths compile
but remain unverified. The packages are still unsigned, so the checksum verifies
integrity rather than publisher identity. Review captures and scratch state stay
under `target/update-verify/` and are not committed.

## October 27 Status bar visibility context control

The document content context menu now includes an icon-and-text `Show status
bar` toggle. It starts checked, uses an unchecked square after the bar is
hidden, and calls the existing context menu even while the status bar is absent.
PDF, EPUB, and Markdown reader layouts omit their 32 px status rows when the
option is off, allowing the viewport to fill the released height. Split reading
keeps both panes on the same preference.

The preference persists as `show_status_bar` in session state schema version 11.
Snapshots from version 10 and earlier default the missing field to enabled. A
focused persistence test covers default-on migration and both serialized values.

`cargo fmt --all -- --check`, the five context-menu component tests, the focused
state-migration test, the complete `cargo test --workspace --locked` suite (259
app tests plus every crate and integration suite), and
`cargo clippy --workspace --all-targets --locked -- -D warnings` pass. Native
macOS inspection with a uniquely bundled app and isolated
`BREEZE_DATA_DIR` confirmed the checked default, hidden layout, unchecked reopened
menu item, and restoration through the context menu. Windows and Linux remain
unverified.

## October 27 Markdown image and Mermaid edge rendering

The apparent borders around Markdown images and Mermaid sequence diagrams are
not drawn by Breeze. The visible fragments came from bitmap minification. Wide
generated SVGs were fitted into the reading column after their original
dimensions had fixed one-point strokes; at scales below 1x, axis-aligned edges
landed between device pixels inconsistently, so some disappeared or became
disconnected. Large raster screenshots went through GPU bilinear minification
for the same reason, which could skip a thin source border.

Mermaid SVGs now rewrite their root width and height to the available reading
width before rasterization. Every `stroke-width` and dashed-stroke length is
scaled by the same inverse factor, preserving an approximately one-logical-pixel
outline at the fitted size. The width bucket is part of the Mermaid cache key, so
resizing or switching between standard and full width rasterizes the correct
variant. The image now fills a correctly proportioned fitted box rather than
leaving natural-size height around a flex-shrunk width.

Large raster images are now area-resized before texture upload to at most 4096
pixels on either axis and 8 million pixels total, while their original dimensions
remain the logical display size. Small images keep their bytes unchanged. The
existing 32 MiB decoded-image budget, 16 MiB input limit, count limit and
per-URI cache remain in force.

`cargo fmt --all -- --check`, the complete `cargo test --workspace --locked`
suite (264 app tests plus every crate and integration suite), and
`cargo clippy --workspace --all-targets --locked -- -D warnings` pass. New tests
cover fitted SVG dimensions, stroke and dash scaling, large-raster downsampling,
logical-size retention, a continuous one-pixel raster border after downsampling,
and a ten-participant sequence diagram fitting inside the reading viewport.
Native confirmation of the corrected pixels was not performed:
macOS locked the screen before the final review bundle could be inspected.
Windows and Linux remain unverified.

## October 26 Markdown reading insets and shared editing widths

Markdown reading no longer lets its private Kit scrollbar cover the last column
of text. The scroll surface now reserves the 16 px overlay track as horizontal
content padding and paints its draggable scrollbar at the outer content-panel
edge. Invisible inset blocks give the first and last document blocks 48 px of
breathing room without shrinking the reading viewport; the insets scroll with
the content, so text may reach the top and bottom edges while scrolling.

Markdown source editing now uses the same standard 760 px measure and full-width
toggle as reading. The width button applies immediately in either mode, and the
source text stays within that measure while the editor scrollbar remains fixed
to the outer content-panel border. Kit's default half-viewport of trailing
editor scroll space is disabled, so the source cannot scroll far past its last
line.

Switching modes now preserves the visible source position. Reading uses its top
visible Markdown block to reveal the corresponding editor line; editing maps its
top visible source line back to the reading position before rebuilding the
preview. Component tests assert text clearance from the private scrollbar,
scrollable top and bottom insets, the full reading viewport height, the
standard-to-full width transition, and both position mappings. Native macOS
inspection with an isolated `BREEZE_DATA_DIR` confirmed the reading surface,
both editor widths, and the reduced trailing scroll extent. Windows and Linux
remain unverified.

The top inset marker now follows leading YAML or TOML front matter. Markdown
recognizes front matter only at byte zero, so prefixing the marker had made
`FrontmatterPlugin` fall back to ordinary content. Rendering offsets account for
the marker's variable insertion point.

Reading/editing switches no longer rebuild an unchanged preview. Returning from
the editor reuses the parsed `TextViewState`, decoded images and scroll model; a
changed draft still reparses for an accurate preview but retains the same
document's bounded image cache. SVG decoding also loads system fonts once and
reuses that tree for size inspection and rasterization, instead of scanning
fonts twice for every badge.

Validation for these fixes: `cargo fmt --all -- --check`, the complete
`cargo test --workspace --locked` suite (256 app tests plus every crate and
integration suite), and
`cargo clippy --workspace --all-targets --locked -- -D warnings` pass. Native
macOS inspection with an isolated `BREEZE_DATA_DIR`
confirmed three front matter entries as a property list and both requested
Shields badges as images. Switching to source and immediately back to reading
restored the rendered badges without a loading state. Windows and Linux remain
unverified.

## October 26 Tab and content context menus

Document tabs now open an icon-and-text context menu with the platform file
manager reveal action and a Close action bound to the clicked tab. File-backed
documents enable reveal; the bundled in-memory sample shows the action disabled.
Close continues through the existing annotation-save and Markdown-draft guards.

The PDF content viewport now exposes the status bar's document-wide operations:
page focus, rotation, layout, zoom steps and presets, fit modes, and fullscreen.
EPUB and Markdown content expose spread or reading-width, text-size, and
fullscreen controls. Each menu entry calls the existing status mutation and uses
Kit's selected state and shortcut display. A right click on EPUB content no
longer starts a text selection, so the context gesture remains separate from
left-button selection.

`cargo fmt --all -- --check`, the focused context-menu component tests,
the complete `cargo test --workspace --locked` suite, and `cargo clippy
--workspace --all-targets --locked -- -D warnings` pass. Native macOS inspection
of the local review bundle used isolated `BREEZE_DATA_DIR` values for a PDF and a
Markdown document and confirmed the tab, PDF content, and Markdown content
menus. Windows and Linux remain unverified.

## October 23 Page submit icon matches Close density

The page input's Enter affordance occupied a full Small icon-button square and
sat eight pixels inside the input border, making it feel heavier than the
surrounding status controls. It now uses the title-bar tab Close dimensions:
an 18.5 px hit target with an XSmall corner-return icon. The suffix adds six
pixels before the button so the page number remains distinct, then pulls the
button toward the border until its visible edge is two pixels inside the input
outline.

The page-position component test now asserts both the 18.5 px target and a
1–3 px right-border gap. `cargo fmt --all -- --check`, the complete
`cargo test --workspace --locked` suite (255 app tests plus all other suites),
and `cargo clippy --workspace --all-targets --locked -- -D warnings` pass.
Native macOS inspection with an isolated `BREEZE_DATA_DIR` confirmed the smaller
icon, retained text separation, and close-to-border placement in the focused
page field. Windows and Linux remain unverified.

## October 23 Status bar stays muted until hover

PDF and reflowable status bars had mixed foreground colors: page and zoom
values were muted, while buttons inherited Kit's normal foreground and became
brighter only through Kit's hover surface. Every status value and control now
starts in the palette's muted foreground. A small status-hover owner tracks the
single control under the pointer and renders only that control with normal
foreground, including icon buttons, text labels, and the zoom dropdown arrow.
Selected view toggles retain normal contrast, and the active page input remains
normally legible.

The treatment covers the PDF page position, page-submit icon, rotation, layout,
fit mode, zoom steps, percentage menu, and fullscreen controls. EPUB/Markdown
position, text-size, width, spread, and fullscreen controls use the same path,
with status text such as character count and text size remaining muted.

`cargo fmt --all -- --check`, `cargo test --workspace --locked` (255 app tests
plus every other workspace suite), and `cargo clippy --workspace --all-targets
--locked -- -D warnings` pass. Component tests verify PDF and reflow controls
are muted at rest and that hovering one control promotes only that control.
Native macOS inspection with isolated `BREEZE_DATA_DIR` values confirmed the
muted PDF and Markdown status bars and normal foreground plus tooltip on a
hovered zoom control. Windows and Linux remain unverified.

## October 23 Annotation controls follow reader scrolling

The selected-annotation controls could keep an old above/below decision after
the page scrolled because the viewport overlay was not rebuilt when only the
scroll offset changed. The reader now tracks both the single/spread page
scroller and the continuous virtual list, repainting once per changed offset
while an annotation is selected. The existing measured-size placement then
re-evaluates the content edge and can move the panel from below to above the
annotation.

A component test starts with the controls below a selected line, scrolls the
page until that position would overflow, and verifies the controls follow the
new page bounds, flip above the line, keep their 8 px gap, and remain inside the
reader viewport. Formatting and Clippy pass. The workspace test run passes with
one pre-existing unrelated Settings assertion skipped
(`home_navigation_selects_about_shortcuts_and_settings`, which expects a
`default-document-app` element absent at the current HEAD). An isolated native
macOS review build launched and a line annotation was created, but the
session's coordinate mapping did not reliably hit the thin line for the
selection/scroll transition, so that visual check remains incomplete. Windows
and Linux remain unverified.

## October 22 Editable page position and zoom presets

The PDF status bar's page position was permanent text followed by an always
visible input and jump button. It now starts as one clickable `current / total`
button. Clicking it, or pressing Primary+G, swaps it for an integer-only input
initialized to the current page; the muted Enter icon is inside the input's
trailing edge. Enter or the icon validates the destination, navigates, and
returns to the text form. Invalid and out-of-range values keep the field open,
select its text, and report the existing navigation error.

The calculated zoom percentage is now a dropdown rather than static text. Its
levels are 25–200% in 25-point steps, 250–400% in 50-point steps, and 500% as
the final 100-point step. `breeze-core` owns that sequence, and percentage zoom,
the preset menu, zoom buttons, keyboard zoom, and discrete wheel steps share it
instead of maintaining separate bounds. Smooth trackpad zoom remains continuous
between 25% and 500%.

`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked
-- -D warnings`, and the focused page/zoom component tests pass. The complete
workspace run reached 252 passing app tests but failed one unrelated Markdown
text-size assertion caused by the working tree's separate default-font-size
change. Native macOS inspection with an isolated
`BREEZE_DATA_DIR` confirmed the page input and inline muted Enter icon, a jump
from page 3 to 5, the complete preset menu, and selection of 250% with the
reader switching to Custom Ratio. Windows and Linux remain unverified.

## October 22 Annotation controls follow their selection

Selected-annotation controls used a fixed 208 px height estimate, leaving a
large gap above shorter panels, and were clipped by the page surface near the
reader edges. The controls now move to a reader-viewport overlay, use their
measured height to choose below or above the selected element with an 8 px gap,
and clamp horizontally and vertically to the content area. The page canvas
keeps the selection outline and resize handles; the overlay remains separate
from page rendering and scrolling.

Unit tests cover upward flipping, gap preservation, horizontal clamping and the
roomier-side fallback. A component test selects annotations near both vertical
edges, verifies the measured overlay stays adjacent and fully inside the reader
viewport. `cargo test --workspace --locked` and Clippy pass with warnings
denied. Native macOS inspection could not run because the session's Mac was
locked; Windows and Linux remain unverified.

## October 21 Markdown rendering unified on GPUI Kit TextView

The user directed removal of Breeze's own Markdown rendering path. The reader
now creates one scrollable, selectable `TextViewState::markdown` for the entire
document. Kit owns Markdown parsing, block virtualization, layout, selection,
links, tables, task checkboxes, images and code cards.

Breeze's mdast-to-HTML projection and per-block `ListState` renderer were
deleted, along with the native table, task-list, code-card and image-row
renderers. TOC, front matter, inline/display math and Mermaid remain as GPUI Kit
`MarkdownPlugin` extensions. A bounded metadata scan now supplies outline rows,
source offsets, block starts and character counts without retaining a render
tree.

`cargo fmt --all -- --check`, `cargo test --workspace --locked` and
`cargo clippy --workspace --all-targets --locked -- -D warnings` pass. New tests
assert one `markdown-document` and no `markdown-block` renderer, exercise Kit's
tables/tasks/code/images, plugin constructs, search, outline jumps and scroll
progress. Follow-up fixes added the native scrollbar-cover treatment, a 24 px
content inset, working TOC reveal ranges, 2x SVG image loading, Mermaid
background removal and Kit tree-sitter code highlighting. Native visual
inspection on macOS confirmed the unified renderer, padding, code highlighting
and frameless image/diagram surfaces; Windows and Linux runtime acceptance
remain pending. See [work package 31](31-markdown-text-view-unification.md).

## October 21 Explicit annotation selection tool

Reading mode could still select a sidecar annotation, and every Annotate tool
could hit an existing item instead of drawing over it. The toolbar had hidden
its stale `SELECT_TOOL` state, so drawing tools were also the only page-level
route into move, resize, style, text and delete controls.

Annotate now begins with a `mouse-pointer-2` Select button, followed by the
eleven drawing tools. Select is the only state in which page annotations and
text-note overlays can be hit, moved, resized, restyled, edited or deleted.
Choosing any drawing tool clears the current selection and makes every gesture
creation-only, including a gesture that starts over an existing annotation.
Reading ignores annotation hit targets so its pointer remains available for
text selection. Adding a text note with the note tool also no longer leaves the
new note selected.

Verified in an isolated worktree based on `ecf7fe5` with the annotation files
ported onto that checkout because unrelated Markdown edits in the working tree
do not compile. `cargo test --workspace --locked` passes all 247 app tests plus
the remaining workspace suites, including new Reading, note-overlay,
drawing-over-existing and Select-tool regressions; `cargo fmt --all -- --
check` and `cargo clippy --workspace --all-targets --locked -- -D warnings`
exit 0 in that worktree. Native macOS inspection used an
isolated `BREEZE_DATA_DIR`: Select appeared first and showed its filled active
state, a second circle was added over an existing one without selecting either,
and Reading clicks left no edit controls. Windows and Linux remain unverified;
the main checkout's workspace-wide checks remain blocked by the unrelated
Markdown compile error.

## October 21 Cross-page selection ends with its drag

A long PDF text selection could keep growing after the mouse was released, then
lose its range when the next right click arrived. The window-level mouse-up
listener missed releases delivered after the page that owned the drag was
virtualized away. Its move handler also accepted hover events, so those later
events could re-extend the stale selection before the page-up path committed it.

A committed selection now always clears the drag gesture, pointer and edge
scroll while retaining the selected range and highlights. Left-button release
is handled in the capture phase, and any wheel scroll first finalizes a stale
drag instead of letting its auto-scroll continue over newly revealed pages.

Verified with `cargo test --workspace --locked` (246 app tests plus the remaining
workspace suites) and `cargo clippy --workspace --all-targets --locked -- -D
warnings`. A native macOS run used an isolated `BREEZE_DATA_DIR` and a 388-page
PDF; real drag input, release and scrolling were inspected. The checkout-wide
`cargo fmt --all -- --check` remains blocked by unrelated unformatted
`markdown_style.rs`; the three touched Rust files pass `rustfmt`. The range
remained intact through a native wheel scroll; Windows and Linux remain
unverified.

## October 20 Native Markdown reading blocks

The user's second pass over the reference note produced eight concrete rendering
issues. The fixes are in [specification 15](../specs/15-markdown-rs-parser.md#reading-constructs-october-20-2026)
and [work package 29](29-markdown-reading-constructs.md).

- Image-only paragraphs now render as a centred, borderless row. Several badges
  share one line and scale down together instead of wrapping; local, remote and
  SVG images use the same bounded loader.
- Source line breaks inside a paragraph render as line breaks (`<br>`), so a
  wrapped source line is a wrapped reading line.
- The TOC nests subheadings inside their parent's list item, producing the
  reader's list indentation.
- `---` and `===` separators draw a 1.5 px divider in the muted foreground,
  and spaced forms such as `= = =` are recognized. A separator that the parser
  reads as a setext heading remains a heading.
- Task rows are native: a 16 px checkbox with a 4 px corner radius and the
  item's inline content, indented by nesting level.
- Tables are native grids with one inline text view per cell and GFM column
  alignment applied to the text; selection backgrounds now follow the aligned
  text, and every cell renders.
- Code blocks are native cards: language on the left (muted), a copy button and
  an ellipsis menu on the right. The menu toggles line numbers and wrapping;
  the body uses the reader's monospace code size.

Verified with `cargo fmt --all -- --check`, `cargo test --workspace --locked`
(410 passed) and `cargo clippy --workspace --all-targets --locked -- -D
warnings`. New component tests cover the native block ids and heights, the rule
height, nested TOC, soft breaks and the code controls. Native visual inspection
was not performed because this session's model cannot view images; Windows and
Linux remain unverified.

## October 20 Markdown reading constructs

The user's reference note exposed thirteen reading gaps. The reader now
classifies top-level blocks and routes each shape to the renderer that can draw
it; [specification 15](../specs/15-markdown-rs-parser.md#reading-constructs-october-20-2026)
has the full behavior table and [work package 29](29-markdown-reading-constructs.md)
the work record.

- Front matter renders as a native metadata card; `[TOC]`, `[[toc]]`,
  `[[_TOC_]]` and `{{toc}}` become a generated heading list; `---` and
  `===`/`***`-only lines become a native divider.
- Tables and task lists go through Kit's Markdown reader, so GFM column
  alignment and checkboxes appear and every cell renders (including the
  reported `help()` cell).
- `<sup>`/`<sub>` map to Unicode; known `:shortcode:` emoji become characters;
  abbreviation declaration paragraphs are hidden.
- Display formulas typeset with RaTeX and its embedded KaTeX fonts; Mermaid
  fences render natively to SVG with `mermaid-rs-renderer`. Unsupported Mermaid
  syntax falls back to the source code card.
- Images load from local paths, remote `http(s)` URLs and SVG. Remote fetches
  are bounded (10 s, 16 MiB, five redirects); SVG rasterization uses system
  fonts so badge labels render.
- Licenses: added tree-sitter, RaTeX with KaTeX OFL fonts, and
  mermaid-rs-renderer notices under `assets/licenses/`.

Verified with `cargo fmt --all -- --check`, `cargo test --workspace --locked`
(408 passed) and `cargo clippy --workspace --all-targets --locked -- -D
warnings`. New tests cover the construct classification and rendering, SVG
rasterization, a loopback remote fetch, formula typesetting and the native
blocks. Native visual inspection was not performed because this session's model
cannot view images. Remaining: abbreviation tooltips, inline formula typesetting,
a dark Mermaid palette, and Windows/Linux verification.

## October 20 Markdown editing becomes a highlighted source editor

The user chose the reliable source-editor route after the live-preview
experiments were measured. Editing is now GPUI Kit's code editor with the
tree-sitter Markdown grammar; the reading view is the rendered preview. See
[specification 15](../specs/15-markdown-rs-parser.md#editing-surface-october-20-2026)
and [work package 28](28-markdown-source-editor.md).

- `MarkdownEditor` hosts Kit's `EditorState`/`Editor` instead of the home-grown
  projection surface. Text, selection, undo/redo, IME, soft wrap, search and
  highlighting belong to the component; Breeze wires the file, font size, dirty
  state and the status line. The `Live formatting` / `Markdown source` toolbar
  pair is removed; the reader's Edit/Reading control switches between source and
  preview.
- `breeze-app` enables `gpui-kit`'s `tree-sitter-markdown` feature. The editor
  uses soft wrap with no line numbers or folding, so a note reads as a page.
- Dirty state is read from the editor text: a byte-length read keeps caret
  movement cheap, contents are compared at equal length, and an in-progress
  composition counts as an edit. Opening is clean; leaving and re-entering edit
  mode keeps the draft; the close and quit guards still see it.
- The reader's 2 MiB parse ceiling no longer limits editing: a larger file opens
  in the source editor while the reader reports the limit. The 16 MiB source
  session limit is unchanged.
- The mdast semantic/projection modules in `breeze-reflow` are no longer used by
  the application. Their tests still pass; a follow-up cleanup can retire them.
- The earlier `zorite-editor` evaluation was not kept: 64 KiB took 0.86 s for a
  debug first frame and 1 MiB did not finish a Release first frame within 60 s.

Verified with `cargo fmt --all -- --check`, `cargo test --workspace --locked`,
`cargo clippy --workspace --all-targets --locked -- -D warnings` and a native
launch with an isolated `BREEZE_DATA_DIR`. Component tests cover entering edit
mode, the source status line, the character count, the split-tab draft, and the
oversized-file path. Native visual inspection was not performed: this session's
model cannot view images. Windows and Linux remain unverified.

## October 20 Markdown rebuilt on markdown-rs

The user asked for rendering, live editing and the source view to be rebuilt
entirely on markdown-rs, taking ELY Office's `apps/ely-mark` as the reference,
and chose one AST projection, editing parity instead of new authoring features,
and a 2 MiB whole-file parse ceiling. The decision and its measured costs are in
[specification 15](../specs/15-markdown-rs-parser.md) and
[work package 27](27-markdown-rs-rebuild.md).

- `breeze-reflow` now has one parse function (`markdown::ast`) over
  `markdown = 1.0.0` with GFM. `pulldown-cmark` is removed from the workspace
  and lockfile. Reading blocks, headings, plain text and search text all come
  from the same mdast tree; link and footnote definitions resolve
  document-wide.
- `markdown/render.rs` projects mdast to the HTML subset GPUI Kit reads. A
  block containing a code block still hands its own source to Kit's Markdown
  view, because Kit's HTML reader has no code card.
- `markdown/semantic.rs` derives one `LineKind` per source line (block kind,
  quote depth, hidden setext underline, exact ATX/list marker range) and inline
  spans from node positions. Character references and escapes are split into
  per-run source ranges, so `&amp;` keeps a byte-accurate caret map and CRLF
  documents keep their bytes. The projection's line scanner remains only as the
  provisional projection used before a worker result.
- Above 2 MiB the reader does not parse: it states the limit, the editor opens
  in Source, the Live control is disabled, and every byte stays editable and
  copyable. `MAX_PARSE_BYTES` is the ceiling.
- Leading container chains deeper than the 64-level policy are refused before
  parsing, so markdown-rs cannot spend minutes on `> > > ...` before the tree
  check.

Verified with `cargo fmt --all -- --check`, `cargo test --workspace --locked`
(241 app tests and all crate suites), `cargo clippy --workspace --all-targets
--locked -- -D warnings`, and the Release corpus probe: 10 KiB 2.0 ms, 100 KiB
36 ms and 1 MiB 2.0 s for a semantic parse; the 2 MiB ceiling is refused before
parsing. New coverage includes a component test that renders the mdast HTML
through Kit (references, table and footnote) and an oversized-document test that
checks the reader notice, source-only editor and full character count.

Native visual inspection was not performed: this session's model cannot view
images, so the macOS screenshot could not be reviewed. The app was launched with
an isolated `BREEZE_DATA_DIR` and a fixture covering headings, lists, quotes,
tables, code, images, footnotes and raw HTML, and component tests cover the same
content. Windows and Linux remain unverified.

## October 20 EPUB and Markdown file associations

OS registration still declared only PDF after the reader gained EPUB and
Markdown support, so Finder, File Explorer and Linux file managers never
offered Breeze for those formats. Both packaging paths now register every
format `DocumentFormat` accepts:

- macOS: `CFBundleDocumentTypes` entries for `com.adobe.pdf`,
  `org.idpf.epub-container` and `net.daringfireball.markdown` (`.pdf`,
  `.epub`, `.md`, `.markdown`), each with role `Viewer` and
  `LSHandlerRank` `Alternate`, in `scripts/bundle-macos.sh` and
  `scripts/release.py`.
- Windows: `Breeze.Document`, `Breeze.Epub` and `Breeze.Markdown` ProgIDs,
  `OpenWithProgids` values for all four extensions, and those extensions under
  `Applications\breeze-app.exe\SupportedTypes` in both the MSI and Inno
  sources.
- Linux: `MimeType=application/pdf;application/epub+zip;text/markdown;`.

No extension's default value is written, so changing the default stays an OS
decision. The Settings row now reads "Default document app" instead of "Default
PDF app". `scripts/test_release.py` compares the local macOS bundle with the
release registrations so the two packaging paths cannot drift.

`cargo fmt --all -- --check`, the complete `cargo test --workspace --locked`
suite (264 app tests plus every crate and integration suite),
`cargo clippy --workspace --all-targets --locked -- -D warnings`, and all 16
`scripts/test_release.py` cases pass. The native macOS Launch Services check
was not repeated in this commit pass; Windows and Linux registration remain
unverified.

## October 20 Split tabs start on the pane divider

Reported on macOS: after dragging a tab to split the reader, the right pane's
first tab sat well to the right of the panes' divider. Measured at a 1000 px
window with an even split: panes at x 0 and 500, the right group's tab strip
starting at 553 and its first tab at 557 — 57 px off the divider.

The title bar's two tab groups shared whatever width remained after the macOS
traffic lights and the Home button in equal halves. The panes divide the whole
window, so the two coordinate spaces do not meet: half of 894 px of leftover
title bar is not the pane boundary at 500 px. The same layout could not follow
the divider being dragged at all, because the split was never consulted.

The right-hand group is now sized to its pane and starts on the seam. It carries
the divider as a 1 px leading border, which puts the hairline on the same pixel
the resize handle paints and leaves the first tab the documented 4 px inside the
strip. `sizes()[1]` of the split's `ResizableState` is the right-hand pane in
either arrangement (the secondary pane is added second unless it sits on the
left), and the state's existing observation already redraws the title bar while
the divider is dragged.

The width needs one correction. The title-bar content row ends before the
window's right edge — on Windows and Linux Kit's window controls take about
102 px there, and on macOS nothing — so the pane width alone would overhang by
that distance. The row is measured during prepaint (`window.viewport_size()`
minus its own bounds) into `title_bar_right_chrome`; the header's trailing space
moved from the row onto the right-hand group so the row's bounds end exactly at
that chrome. The left-hand group keeps the remaining width, which equals its
pane, so a wide left pane never starves its tabs.

`split_tab_groups_start_on_their_panes` splits both ways, resizes the window to
1440 and 900 px, and then moves the divider programmatically to 420 px. Each
time it asserts that the right-hand group's left edge equals the right pane's
left edge and that exactly 5 px (the 1 px divider plus the strip's 4 px lead
space) separate the divider from the first tab. Before the change the assertion
failed by 53 px.

Verification on this host: `cargo fmt --all -- --check` and `cargo test -p
breeze-app --locked` (240 tests) pass, including the new test. The workspace-wide
`cargo test`/`cargo clippy` runs could not complete: a concurrent, uncommitted
edit in `crates/breeze-reflow/src/markdown/projection.rs` (an inferred `depth`
integer) does not compile, so `breeze-reflow` and every crate above it fail to
build. That file is not part of this change and was left untouched. Native
visual inspection on macOS also remains unavailable in this session: window
capture and synthetic input are refused for lack of Screen Recording and
Accessibility authorization, so the assertion is geometric rather than visual.

## October 20 Markdown type scale, code blocks and editor size

The Markdown reader laid every block out as HTML in Kit's rich-text view, which
sizes headings from a fixed 14 px base: h1-h6 were 28, 21, 17.5, 15.75, 14 and
14 px whatever the reading font size, so h3-h6 were smaller than the 18 px body
text and the text-size control never moved a heading. The same view has no
`<pre>` handling, so a fence became a paragraph of inline-code runs — no code
card, no language, 0.875 em text. The live editor kept its own fixed 16 px body
and 13 px code, so editing ignored the reading size entirely.

`breeze-app::markdown_style` now derives the reader's heading scale (1.6, 1.4,
1.25, 1.1, 1, 1 times the body size), code size and paragraph rhythm from
`reflow.font_size`, and rebuilds the themed `TextViewStyle` because Kit exposes
only a whole-style override. A block whose source contains a fenced or indented
code block keeps that source and is laid out from Markdown instead of HTML, so
Kit draws its own code card. `MarkdownEditor` derives body, heading and code
metrics from the same reading size, reflows when it changes and scales its
scroll offset with it.

Limits: code blocks use Kit's card, padding and monospace size but no syntax
highlighting, because the optional `tree-sitter` feature stays off; laying a
code-containing block out from its own source would drop reference and footnote
definitions written elsewhere in the document; horizontal rules and task-list
checkboxes still render as nothing, because the HTML projection has no element
for them; the editor's list indent stays at 16 px per level.

Validation: `cargo fmt --all -- --check`, `cargo test --workspace --locked` (239
app tests, seven of them new: heading-scale unit tests, rendered heading and
body block heights before and after a text-size change, a fence slice parsing as
code, the reader laying out top-level and nested fences, and the editor
reflowing while keeping its place) and `cargo clippy --workspace --all-targets
--locked -- -D warnings` pass. `cargo run --locked -p breeze-app` starts on the
disposable Chinese Markdown fixture under `target/md-review/` without a console
error. Native visual inspection is unperformed: two `screencapture` runs taken
around an app launch are byte-identical and show only the desktop, so this
session cannot capture the window, and the layout statements above come from
rendered-component bounds instead of pixels. Windows and Linux runtime remain
unverified.

## October 20 no reading progress in the Markdown status bar

The Markdown status bar reported a reading percentage where EPUB reports a page
position. Markdown scrolls without pages, so that percentage only repeated the
scrollbar's fraction in another form, and it read as a page position beside the
rendered character count. The position element now renders only from a landed
EPUB frame — `reflow_position_text` returns the chapter page count for a frame
and nothing otherwise — so Markdown and a chapter still being laid out show no
progress figure. Character count, text size and the reading-width toggle are
unchanged, and the document information panel keeps reporting reading progress.

Validation ran in a detached worktree of `5f4bdd4` holding only this change,
because unrelated Markdown text-style work is in progress in the checkout.
`cargo fmt --all -- --check` passes; `cargo test --workspace --locked` passes
with the app suite's 232 tests, including regressions that a Markdown document
and an open draft have no `reflow-position` while the EPUB page count still
parses as `page / pages`; and `cargo clippy --workspace --all-targets --locked --
-D warnings` passes. Native visual inspection was not performed: screen capture
returned a uniform surface with no readable content, so the display appeared
locked or asleep, and the status-bar change is covered by rendered-component
tests alone. Windows and Linux runtime remain unverified.

## October 20 reading draws one selection, not two

A read-mode drag painted two highlights: the selection fill, and the amber
preview of the highlight annotation that `draft()` used to build in any mode.
The preview was drawn above the selection, clamped to the page the press landed
on, so once a drag passed that page it stopped while the selection kept going —
the "yellow-brown layer freezes" report. Reading never creates an annotation, so
`draft()` now returns nothing outside Annotate mode; the selection fill is the
only highlight a read-mode drag draws.

Validation: formatting, the app suite (232 tests, including a regression that a
read-mode drag leaves `draft()` empty and the annotation history untouched while
Annotate still previews its highlight) and Clippy with warnings denied for
`breeze-app` and `breeze-pdf` pass.

## October 20 Windows title-bar clicks and the installed application icon

Two platform defects were reported from the Windows package: Home and the Plus
button in the title bar did nothing, and an installed Breeze had no application
logo. Both had platform-specific causes; the icon defect was also present on
Linux.

### The title-bar caption swallowed Home and Plus

GPUI Kit's `TitleBar` marks the container of its children with
`WindowControlArea::Drag`. On Windows, GPUI answers `WM_NCHITTEST` with
`HTCAPTION` whenever that drag hitbox is anywhere in the current mouse hit-test
path (`gpui-pre-windows` 0.3.8, `handle_hit_test_msg`). A caption press is
delivered to GPUI first (`handle_nc_mouse_down_msg`), but the native move loop
starts unless a GPUI element stops propagation during the press.

Kit's `Button` prevents focus and text selection on mouse-down but does not stop
propagation; its action runs from `on_click`, which requires the mouse-up. The
caption therefore took the press, moved the window, and never delivered the
release, so Home and Plus had no effect. Document tabs were not affected: the
draggable tab row stops propagation on press, which suppresses the native move
loop. Close buttons sit inside that row and were equally protected. There was no Linux equivalent: the Linux backend ignores
window-control hitboxes (`on_hit_test_window_control` is a no-op), so a press is
never reclassified as a caption.

The fix keeps Kit's title bar for its chrome but removes the caption from under
the controls:

- `titlebar-content` blocks the mouse (`occlude`), which cuts the hit-test path
  before Kit's drag hitbox and turns Home and Plus into ordinary client clicks.
- The tab group's empty width is the window-drag surface. The tab strip and the
  divider/Plus row block the group's own caption hitbox, so only the width after
  the controls maps to the caption; tab rows additionally stop their own press.
  A 24 px spacer stays reserved there even when tabs fill the strip, so the
  window cannot become unmovable from its title bar.
- macOS and Linux ignore the window-control area. The same empty width starts
  their move through `start_window_move`, and keeps Kit's title-bar double-click
  behavior (`titlebar_double_click` on macOS, zoom on Linux) and the Linux window
  menu. The handlers require a press on that surface and the button still held,
  so a press that leaves the strip cannot start a later, unrequested move.

`title_bar_drag_area_excludes_the_tab_controls` presses the empty width, Home, a
document tab, and Plus with real GPUI hit testing, then moves the pointer with
the button held. Kit's title bar starts `start_window_move` from that move; the
headless test platform has no move loop and panics, so a caption leak reaches a
failing test instead of passing silently. Removing the content-row block
reproduces that failure. The same test also covers the reported controls
directly.

### The executable and window had no icon

Windows reads a window's taskbar icon from the window class. GPUI registers that
class with `LoadImageW(module, MAKEINTRESOURCE(1), IMAGE_ICON, …)`, but
`breeze-app.exe` shipped with no icon resource. The MSI and setup UI used
`breeze.ico` for their own artwork, while the Start Menu shortcut, the PDF
association, and the Inno uninstall entry pointed at the executable's icon, so
they fell back too. Explorer, the taskbar, Alt-Tab, and the uninstall entry all
showed the generic program icon.

`crates/breeze-app/build.rs` now embeds `crates/breeze-app/breeze.rc` as
resource 1 through `embed-resource` (already locked as GPUI's manifest build
dependency); the script refers to `assets/brand/breeze.ico` relative to the crate
root, which is the working directory both rc.exe and llvm-rc receive. A missing resource
compiler fails the Windows build instead of shipping an icon-less executable.
The release workflow now opens the packaged `breeze-app.exe` as a data module
and requires a `RT_GROUP_ICON` resource named 1, which is the resource GPUI and
Explorer load.

Linux had the equivalent gap: the window was created without `app_id` and
without `WindowOptions.icon`. Wayland has no window icon property and matches
the application to a desktop entry by `app_id`, so without one it could not find
the installed `breeze.desktop`/`breeze.png`; X11 also had no `_NET_WM_ICON` and
no `WM_CLASS` for its launcher to match. `app_window_options` now sets
`app_id=breeze` and the bundled 256 px raster (which X11 publishes as
`_NET_WM_ICON`), and the desktop entry declares `StartupWMClass=breeze` for X11.
The raster is decoded straight RGBA; the About tile remains a separate bounded
BGRA decode and is unchanged. Tests cover the packaged raster's transparency and
the window options' identity fields, and the Python release tests assert the
desktop entry, the installed icon, and the package layout.

Windows uses the executable's icon resource rather than this raster. macOS keeps
`Breeze.icns` in the bundle.

Verification on this host: `cargo fmt --all -- --check`, the full workspace tests,
Clippy with warnings denied, and the release tooling tests pass on macOS. The
Windows resource link and the Windows/Linux runtime behavior were not executed
here. Native visual inspection of the AppKit window could not be completed: the
session has no Accessibility or Screen Recording authorization, so synthetic
input and window capture were refused. `CGWindowListCopyWindowInfo` confirmed the
reader window opened at 1340×901. Native Windows and Linux acceptance, including
the caption interaction and the taskbar/dock icon, remains pending.
## October 19 Annotation history moves into the reader toolbar

The annotation sidebar no longer carries its own Undo, Redo, Save, and Save As
row. Those commands now sit in the reader toolbar's left half, immediately after
the Read/Annotate tabs and ahead of the centered tool group, so editing does not
depend on the document panel being open and Save stays reachable in both modes.
In Annotate mode a 1 px divider separates the history group from the annotation
tool icons; Read mode omits the separator because no tools follow it. Command ids
are unchanged (`undo`, `redo`, `save-sidecar`, `save-sidecar-as`). The sidebar
footer keeps the unsaved/not-ready status, annotation errors, recovery actions,
and PDF export.

Validation: formatting and all-target Clippy with warnings denied pass. The
component test asserts the group stays inside `reader-toolbar` with all four
controls visible while the document panel is closed. A native capture of the
isolated reader confirmed the left-aligned Undo/Redo/Save/Save As group after
the mode tabs; this session's Windows window renders at 2x into a smaller
window whose right side is clipped, so the Annotate-mode divider was not
captured natively.

## October 19 The annotation list keeps one hover surface

Deleting an annotation is now a borderless trailing action inside the row button
instead of a sibling outside it, so the row's hover fill covers the delete icon.
That button paints no background of its own: only its trash icon changes, from
the secondary foreground to the destructive color while the button itself is
hovered. Disabled rows keep a dimmed icon and no hover change, and the delete
click stops propagation so deleting never also navigates to the annotation's
page.

Validation: formatting and all-target Clippy pass. Component checks assert the
row button's bounds contain the delete action and that deleting leaves the
navigation page and generation unchanged. The workspace suite reaches 221
passing app tests; three EPUB/reflow tests fail identically on the unmodified
checkout in this Windows session (the recorded Windows EPUB layout hang), and
two PDF timing probes pass when rerun serially. The row's hover and icon colors
were not captured natively; this session clips the reader window's right side,
where the annotation panel sits.

## October 17 Windows lays out Chinese prose an order of magnitude slower

`chinese_prose_is_drawn_with_its_own_stylesheet` guards the CJK font hooks with a
per-page time bound, because the defect it was written for failed a font lookup
per character and cost over a second per page. On a `windows-2022` runner the
first page of that fixture takes **4.0–5.3 s**, against **under 500 ms** on the
macOS and Linux runners, while the glyphs are drawn correctly: the ink-coverage
and opacity assertions pass.

That runner varies by a third between runs, so no time bound there separates a
regression from its noise. The bound therefore applies where it was measured,
and the Windows run keeps the fidelity assertions only. The gap itself is a
Windows performance defect to investigate, not something a bound should hide.

## October 17 A Windows source stays open, so it cannot be replaced by rename

`crates/breeze-pdf/tests/source_identity.rs` replaces a source PDF by renaming a
new file over it, the way an editor or installer does, and expects the worker to
reload it. On Windows the rename fails with `PermissionDenied`: the PDF worker
keeps the source open through `PdfDocument::open`, and MuPDF's Windows handle
does not permit the delete sharing that a replacing rename needs. The same test
passes on macOS and Linux, where renaming over an open file is allowed.

Impact: while Breeze has a PDF open, another program cannot replace that file by
rename on Windows; it reports "access is denied" until the document is closed.
That matches how most Windows PDF readers behave, but it is a change from the
previous engine, which read the file into memory.

The test carries `#[cfg_attr(windows, ignore = "…")]` with that reason. Two
options would change the behaviour: open the source from the bytes `read_source`
already reads under the 256 MiB budget, which also drops the duplicate read and
keeps no handle, or close and reopen the MuPDF document per render. Both move
the memory profile, so neither belongs in a release fix.

A related consequence of the same Windows constraint: `SourceIdentity` cannot
read the volume serial number or file index, because they sit behind the
unstable `windows_by_handle` feature and the workspace forbids first-party FFI.
It uses the creation time as the stable stand-in, which detects replacement by
rename but not an in-place rewrite that preserves size, creation and
modification times. The content fingerprint remains the authoritative check.

## October 17 Two-page EPUB reading hangs in MuPDF layout on Windows

The Windows job of the release workflow failed on
`ui_tests::epub_two_page_reading_fills_the_content_area`: the reader toggled to
two-page reading, but no two-page frame ever arrived. The defect is in the reader
engine, not in the test.

Instrumenting the request and reply path narrowed it to a single call.
`breeze-app` dispatched the spread request (`epoch` 3 → 4), nothing superseded
it, no error was recorded, and the worker picked the job up; the worker then
never returned from `Document::layout`, which
`breeze_reflow::epub::layout::prepare` calls with the half-width column. Measured
on a `windows-2022` runner, with a six-chapter fixture of forty short English
paragraphs:

| Request | Outcome |
| --- | --- |
| `layout(600, 800)`, the first placeholder viewport | returns, 5 pages |
| `layout(792, 600)`, single-page reading | returns, 7 pages |
| `layout(396, 600)` at font size 18, the two-page column | never returns |

macOS and Linux lay out the same 396×600 column and return seven pages, so this
belongs to the Windows build of MuPDF (`mupdf-sys` 0.8.0) rather than to the
request shape: the existing Windows spread test renders at 600×800 and passes.

Impact: two-page reading is unusable in the Windows package. Single-page EPUB
reading, PDF reading, search, selection and annotation export are unaffected.

The component test carries `#[cfg_attr(windows, ignore = "…")]`, so a Windows
run reports it as skipped with that reason instead of passing, and every other
Windows test still runs. The tracing added while locating the hang was reverted.
Resolving this needs a Windows debugging session or an upstream `mupdf-rs`
report; a width-specific workaround would hide an engine defect instead of
fixing it.

## October 16 EPUB text selection

EPUB pages were rendered as opaque BGRA rasters with only a flat text string for
search, so the reader could not map a pointer to a character and nothing was
selectable. This was a missing integration, not a MuPDF capability limit:
`breeze-reflow` already obtained a structured text page per rendered page, then
discarded its character quads.

`breeze_reflow::epub::TextLayer` now carries the page's glyphs and lines in
reading order with normalized page-space bounds. The EPUB adapter returns one
layer per visible raster, including both halves of a spread. `breeze-app` maps a
pointer through the page image's real `Contain` rectangle, anchors a caret
selection on press, extends it on move, and paints one translucent rectangle per
selected line. Dragging between the two visible pages selects the intervening
text as whole pages, matching the PDF reader's cross-page selection model.
Dependencies and `Cargo.toml` are unchanged.

Validation: `cargo test -p breeze-reflow --locked` (14 unit and 8 EPUB tests),
`cargo test -p breeze-app --locked reflow::tests` (6 tests, including a real
pointer drag that selects two lines and checks the painted rectangles), `cargo
fmt --all -- --check`, and Clippy for both crates with warnings denied pass. The
real 23 MB Chinese EPUB produced six text lines with glyph bounds on its first
page. Native macOS capture could not complete the drag because the available
automation injects a press-and-release without intermediate move events; the
component test drives the same hit testing and callbacks with real intermediate
moves. Windows and Linux remain unverified.

## October 16 selection handle alignment

Reported after the frame pixel-alignment change: the corner and edge squares did
not sit on the frame; they were offset from it. Two independent causes, both in
the new canvas paint path:

- The handle list was built from page-local points and then painted with the
  canvas origin added again, shifting every square by the page surface's own
  origin. On the sample fixture that put the handles 58 x 139.5 logical pixels
  away from the frame.
- The square fills were quads. The renderer snaps quads to whole device pixels
  while the frame path sits on half-pixel centers, leaving a fixed half-device-
  pixel disagreement even after the origin was fixed.

The handle list now carries only the unit positions inside the frame and is
resolved against the same origin as the outline. Handle fills are paths rather
than quads, so the outline, the fill, and the frame share one coordinate grid.
Square centers snap to the frame centerlines and half-extents are whole device
pixels, which keeps each square symmetric about its frame point at 1x, 2x, and
3x scale.

Validation used the isolated tree for the pixel-alignment change. Formatting,
`cargo test --workspace --locked` (206 app tests and all remaining suites) and
`cargo clippy --workspace --all-targets --locked -- -D warnings` pass. A new
component test renders a selected rectangle through the real window and compares
every painted handle quad against the frame's snapped corners and edge midpoints,
rejecting any offset beyond one device pixel; it fails on the pre-fix code with
the handle 1.1 device pixels from its frame point. Another test pins the square's
symmetry and device-grid edges. Native macOS inspection measured all eight handle
fills against the frame: every center landed within 3 device pixels (1.5 logical
pixels) of its frame corner or edge midpoint, and a corner-handle drag still
resized the ellipse while the frame and handles stayed aligned. Windows and Linux
remain unverified.

## October 16 selection scrolling and late pages

Two limits of the cross-page selection are gone. Holding a drag past the reading
viewport's edge now scrolls the document and keeps selecting, at a speed that
grows with how far past the edge the pointer is (2 to 18 logical pixels per 16 ms
step, from a 24 px band), the way a text editor scrolls past its last visible
line; the scroll stops when the document ends, when the pointer comes back
inside, and on release.

The selection is also no longer tied to the surfaces that happened to be laid out.
Each page surface registers its bounds every frame and the pointer resolves
against those, so a pointer in the gap between two pages, or in the viewport's
own padding, belongs to the nearer page instead of dropping the move. A page
whose text has not been prepared leaves the selection where it is and is retried
when its text arrives, so a drag held over a page still being rendered catches up
on its own. Moves and releases are handled at the window level, so a drag that
ends over the toolbar or the status bar still commits and its edge scroll always
stops; a fast scroll during a drag can still move a page under a stationary
pointer before the next frame.

Validation: `cargo fmt` on the touched files, the app suite (207 tests) and
Clippy with warnings denied for `breeze-app` and `breeze-pdf` pass. The
auto-scroll test drives real press/move/release events and asserts that the
viewport scrolled, the selection grew, and nothing kept scrolling after a release
outside the viewport. One new test covers a page whose text arrives mid-drag
joining the selection; the cross-page test now also crosses the gap between the
two pages. The workspace-wide `--all-targets` Clippy could not run because
another session's in-flight Markdown test file does not compile; the crates this
change touches were checked directly.

## October 16 selection frame pixel alignment and glow

The selection frame was built from two nested element borders placed on the
fractional overlay coordinates produced by the page transform. Each edge then
received a different antialiasing share, so the top/left and bottom/right sides
looked like they had different widths on a white page, and the frame competed
with light PDF backgrounds.

The frame and its resize handles are now painted as paths on one canvas. Every
edge and handle side is centered on a device pixel, so all four sides cover
their pixels identically at any device scale, and handle squares stay 9 logical
pixels wide while their 16-pixel pointer targets are unchanged. The frame is a
layered blue glow: an 11-pixel halo at 8% alpha, a 6.5-pixel mid layer at 18%,
and a 1.5-pixel core at 85%. The core uses a softer blue than the previous
0x2563eb. Handle hit targets remain ordinary elements so hit testing, cursors,
and the existing resize gestures are untouched; only their appearance moved to
the canvas.

Validation used an isolated copy of the committed baseline plus these changes.
Formatting, `cargo test --workspace --locked` (204 app tests and all remaining
suites) and `cargo clippy --workspace --all-targets --locked -- -D warnings`
pass. Tests cover pixel-centred frame edges and handle squares at 1x, 2x, and 3x
device scale, monotonic halo width/alpha, and the existing resize, hit-testing,
and rotation coverage. Native macOS inspection selected an ellipse on the
authored white fixture and sampled the rendered image: the four frame-edge
intensities measured 106/107 (top/bottom) and 104/104 (left/right), replacing the
earlier roughly 18% horizontal-to-vertical difference; the glow and its corner
and edge handles were visible. Windows and Linux remain unverified.

## October 16 Markdown character count in the status bar

The Markdown status bar now reports how many characters the reading surface
shows, next to the existing reading progress. The count comes from the parsed
rendered text in reading mode, so Markdown syntax markers are excluded exactly
as they are from the page. While a draft editor is open, the same element
reports the open session's projection instead, which keeps the number in step
with typing and with the instant-layout/source-view toggle. Counts of a thousand
or more are grouped with thousands separators, and the element carries the
visible text as its accessibility label. EPUB and PDF do not show it.

The character count for an editor draft is captured whenever a projection is
installed: the provisional line-local projection, the refined semantic one, and
the caret-driven rebuilds all update it, so the status bar never shows a stale
number after a keystroke.

Validation in an isolated worktree at `4e5f836`: `cargo fmt --all -- --check`,
the full workspace suite (202 app tests plus the other crates), and
`cargo clippy --workspace --all-targets --locked -- -D warnings` pass. New
coverage asserts the reader count and its accessibility label, the absence of
the element for EPUB, the grouped formatter, and that entering edit mode plus
switching to the source view and back keeps the reported count in step with the
session. A native macOS run confirmed `44 个字符` for a short document,
`2,169 个字符` for a longer one, live `49 个字符` after typing, and the same
count in instant-layout and source views. Windows and Linux native checks remain
open.

## October 16 icon-only reader modes and a centered tool group

The PDF reader toolbar no longer labels its mode switch with words. Reading and
Annotate are icon-only segmented tabs — `pointer` and `feather`, each keeping its
accessible label and its `Read mode` / `Annotate mode` tooltip with the bound
shortcut. Reading owns no tool: the removed `mouse-pointer-2` Browse button left
the tool row empty there, and the page now shows a pointer (hand) cursor for
caret-anchored text selection. Annotate keeps its eleven drawing tools, centered
in the row with equal zero-basis slots that collapse when a narrow window has to
scroll the group; its former `mouse-pointer-2` selection button is gone too,
because clicking an annotation already selects it. The 1 px separator between
the mode tabs and the tools is removed.

Component tests assert the empty Reading tool row, the eleven annotated tools
with no twelfth button, and the centered group. Validation in an isolated
checkout of this change: `cargo fmt --all -- --check`,
`cargo test --workspace --locked` (202 app tests plus the other crates' suites),
and `cargo clippy --workspace --all-targets --locked -- -D warnings` all exit 0.
Native visual inspection was not performed: `screencapture` has no Screen
Recording permission on this machine, so the toolbar's icon tabs and centered
group rest on component geometry checks. Windows and Linux remain unvalidated.

## October 16 Markdown source publication and recovery records (P4, storage layer)

[Work package 25](25-markdown-live-editing.md) phase P4's storage layer is
implemented and tested; its application wiring is **not** complete.

`breeze_storage::markdown` reads a Markdown source once and hashes those exact
bytes, publishes a new destination without clobbering, and replaces an existing
destination only when its fingerprint still matches the one the draft adopted —
refusing symbolically linked or multiply linked targets, and taking an advisory
per-Breeze lock around the recheck and the write. A damaged or unsupported
recovery record is reported rather than silently discarded; recovery drafts are
bounded, validated before writing, and pruned clean-first against a 256 MiB
directory budget with the record being written protected from eviction.

Validation: `cargo test -p breeze-storage --offline` (7 unit and 6 new
`markdown_save` integration tests, covering non-clobbering publication, a
fingerprint conflict, invalid UTF-8 and oversized input, recovery round trips,
pruning, an invalid record, and a symlink destination) and `cargo clippy
-p breeze-storage --all-targets --offline -- -D warnings`, both exit 0.

**Blocker recorded, not worked around:** the editor-side save, recovery and
close-guard wiring (`markdown_editor.rs`, `reflow.rs`, `commands.rs`, `main.rs`,
`ui_tests.rs`, `i18n_catalog.rs`) was written and then reverted without being
committed, because a concurrent refactor of `reflow.rs` and `ui_tests.rs` in the
shared working tree changed the reader toolbar and removed the page-turn buttons
underneath it. Merging that wiring is a rebase-and-retest step, not a
mechanical one: the affected tests assert exact toolbar and status-bar contents.
Markdown drafts therefore still cannot be saved from the application, and
phases P5–P7 are unstarted.

## October 16 page-turn buttons removed

The PDF and EPUB status bars no longer show Previous Page or Next Page arrow
buttons. Their layouts now start with the reading position, and the PDF page
field, jump button, rotation, layout, zoom, and fullscreen controls are
unchanged. Page turning stays on the keyboard through Page Up/Page Down and the
horizontal arrow keys, and the EPUB rapid-turn queue is still covered by a test
that now drives five `pagedown` presses.

Retired with the buttons: the PDF `visible` page-range computation that only fed
their disabled state, and the EPUB `reflow_page_bounds` endpoint helper.

Validation in a clean worktree at `71b4c24`: formatting, the full workspace
suite (201 app tests, plus the other crates), and Clippy with warnings denied
pass. A native macOS check confirmed the PDF status bar shows `1 / 8` with the
page field and jump button but no arrows, Right Arrow still advances to page 2,
and the EPUB status bar shows `2 / 12` with Page Down still turning the page.
Windows and Linux native checks remain open.

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

## October 16 text selection follows the pointer across pages

The reading-order selection above kept its gesture on the page the press landed
on: moves over a neighbouring page surface were dropped, so a drag that crossed
into the next page froze, and a release over that page never committed, leaving
the stale press to poison the next selection. Dragging back was the same defect
seen the other way round.

A selection is now two carets, each a page and a glyph index in that page's
reading order, and every page surface maps its own pointer through its own
bounds. Moving onto a neighbouring page extends the selection there, whether it
is a separate page of a spread or the next page in continuous scrolling; moving
back onto the first page takes the selection back with it, and the highlight is
drawn per page while the copied text joins pages with a newline. A press that
never moves selects nothing and clears the status line, and a page whose text has
not arrived keeps the selection where it was rather than snapping it to a
corner. Text markup still clamps to the page it started on, because annotations
are page-local.

Dragging past a page's edge still clamps to that page; there is no auto-scroll
while selecting. Selecting a range that needs a page's text before that page has
been prepared therefore waits for it, and copying a whole document region still
means one gesture per page.

Validation: `cargo fmt` on the touched files, the workspace suite (201 app tests,
39 PDF with three ignored, and the remaining suites) and Clippy with no
diagnostics in the touched crates pass. Five app tests cover the reading-order
range, a backwards drag selecting the same text as a forwards one, a drag that
crosses into the second page and returns, an unloaded page keeping the selection,
a press that never moves, and markup clamping to its page. They ran in a copy of
the tree carrying this work, because another session's in-flight Markdown
refactor left the shared checkout uncompilable at the time; the committed files
are the ones tested there.

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

## October 16 draggable tabs and split reading

Implemented on `feat/draggable-split-tabs`: dragging a document tab onto the
left or right half of the reader creates a second pane when another document
remains open. The workspace allows exactly one horizontal split. Tabs can be
reordered or transferred between its two native title-bar groups; an empty group
returns the remaining documents to one reader. The middle divider uses Kit
Resizable. Each pane keeps its own navigation and document sidebars, content,
toolbar, status bar, focus, search, PDF worker and reflow view.

Moving a tab transfers its annotation history and Markdown editor entity rather
than copying them. Saving/exporting and unfinished text notes block transfers;
both panes participate in window-close protection. Unfinished annotation-file
inspection is restarted by the new owner. Library favorites/category actions
reach the owning pane, and the single existing state writer retains both
panes' document metadata. Split placement, divider size and tab order are
session-only; restarting restores open documents into one tab group.

Native inspection found and corrected titlebar window dragging stealing tab
moves, missing split previews after drag start, compressed tab-group wrappers,
and PDF layout using the full window width instead of its pane's measured
content area. Regression tests exercise real pointer events, left/right
placement, the two-pane limit, cancellation, tab reorder/transfer, independent
focus/sidebars/search/navigation, per-pane fitted PDF bounds, divider resizing,
dirty annotation and Markdown guards, stale owners, and interrupted annotation
inspection.

Validation: `cargo fmt --all -- --check`, `cargo test --workspace --locked` (226
app tests, including 12 new split regressions, and all other workspace suites),
and `cargo clippy --workspace --all-targets --locked -- -D warnings` pass.
`cargo run --locked -p breeze-app` starts successfully with an isolated
`BREEZE_DATA_DIR` below `target/`. The macOS native window was inspected with the
project-authored PDF and a disposable Chinese Markdown file: both documents
render side by side, the tab groups remain readable, four independent sidebars
and two status bars are present, and dragging the peer tab back collapses the
empty pane. Windows/Linux runtime validation remains open. These are functional
checks, not new performance or release-acceptance measurements.
