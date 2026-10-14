# Compact UI Design

## Accepted direction

The user accepted the compact prototype's general layout on September 29, 2026. Preserve its content-first direction rather than the superseded promotional layout.

- No application branding block, welcome poster, decorative sidebar footer, or redundant document-information card.
- Document tabs occupy the native GPUI Kit TitleBar; do not stack a second title bar beneath OS chrome.
- Home, Open, Settings, and other global commands use icon buttons with accessibility labels and tooltips.
- Home owns every global page. Library, About, Shortcuts, and Settings share one compact sidebar and one content area.
- Keep necessary text on document tabs, navigation tabs, and Reading/Annotate mode tabs.
- Use restrained separators, small radii, a 4-pixel spacing system, and no decorative glass/gradient effects.

## Layout and density

```text
Native title bar: window controls | Home | document tabs | divider | Plus
+---------------------------+---------------------------------------------------+
| Outline / Thumbnails tabs | Reading / Annotate + contextual tools              |
|                           +---------------------------------------------------+
| Embedded outline or       | Document viewport              | optional         |
| scrollable thumbnails         |                                | properties       |
|                           +---------------------------------------------------+
|                           | Page navigation | layout | fit mode | zoom        |
+---------------------------+---------------------------------------------------+
```

Current logical dimensions:

| Element | Size |
| --- | --- |
| Title bar | 34 px, with Kit-managed native controls |
| Reader toolbar | 38 px |
| Status bar | 32 px |
| Page margin | 12 px |
| Navigation panel | Default 208 px; resize range 192–360 px |
| Main reader split | Minimum 430 px |
| Root UI text / Kit base font | 13 px / 15 px |

Use Kit's resizable panels and centered, icon-only segmented Outline/Thumbnails tabs (`table-of-contents` / `images`) with accessible labels. The 38 px navigation header pins the navigation toggle to its left edge, centers the tabs between two equal-width slots, and keeps the Outline-only search and fold controls at its right edge. The navigation toggle leaves the reader toolbar while this panel is open and returns to the toolbar's left edge when it is collapsed. Remember the sidebar's session width when collapsed. Fit calculations follow the measured panel width, not a hard-coded value. Center thumbnails horizontally; constrain long outline labels to their available width. Existing saved widths below 192 px are raised to the new minimum without rejecting the saved session.

Let the Kit splitter draw the navigation boundary as a single 1 px line in the same color as the other chrome borders; the navigation panel must not add a second border. Center its 4 px drag indicator on that line, retaining the Kit hover/press/drag transitions and hit area.

Show the Outline tab only when the current document has outline entries. Otherwise, show Thumbnails automatically while retaining the user's navigation-view preference for documents with an outline. Keep known document outlines available during page loading. Thumbnails use a responsive grid of compact cards: one column at 192/208 px, two at 280 px, and three at 360 px, with a maximum card width of 144 px. Preserve page aspect ratios, page numbers, the current-page border, and empty slots in the final row. Virtualize rows and map them to page ranges for loading and navigation; retain the top visible page when the sidebar is resized.

In Outline view, the header's right edge contains title search when entries exist. Nested outlines also show an `ellipsis-vertical` dropdown with Expand All (`list-chevrons-up-down`) and Collapse All (`list-chevrons-down-up`); these actions do not navigate. A flat outline reserves no disclosure-icon column. In a nested outline, titles navigate and chevrons independently fold descendants. The active section uses the nearest resolved preceding destination, with deeper headings preferred on page ties, and promotes its highlight to a visible collapsed ancestor.

Title search opens a separate 34 px row below the unchanged 38 px header. It shares the document search row's bottom border, horizontal padding, control gap, and vertically centered Kit Small input/close button. The header supplies the top divider; both rows keep equal vertical breathing room around their controls. Match titles case-insensitively, retain matching ancestors, and reveal matches inside previously collapsed branches. Search uses temporary folding choices; clearing the query or closing search restores the original folds, and closing also restores the outline scroll position. Escape closes search after the input's native handling. Search and fold controls are absent from Thumbnails view; search state resets with the loaded source/fingerprint. Show an explicit empty result when no title matches.

The page itself retains the document's font and colors. UI density changes do not edit PDF content.

## Home and reader behavior

Home is the single global navigation surface. Its fixed-width sidebar keeps the
library filters and categories in a scrollable region and stacks three full-width
entries at the bottom edge in this order: Shortcuts (`keyboard`, Primary+K), About
(`info`, Primary+I), and Settings (`settings`, Primary+,). Each entry is a real
command, so its tooltip shows the bound shortcut, with Control replacing Command on
Windows and Linux. Exactly one of the four Home pages is visible at a time and the
active entry is highlighted. Selecting a library filter or category returns to the
library page, so filters stay usable from any page. The titlebar Home button selects
the library page; the titlebar Settings button selects the Settings page. Nothing
outside Home duplicates these pages.

Shortcuts, About, and Settings share one bounded reading column: at most 720 logical
pixels wide, centered once the window is wider, with a small inset before the scroll
clip so an active control border is not cut off. About is vertically centered as
well. Its logo, name, version, update row, and credits are center-aligned rather than
left-aligned; each credit stacks its muted label above its value so the two share a
center line. The author is shown as `Pone Ding<poneding@gmail.com>`.

About shows the bundled Breeze logo, the application name, the installed version,
the update check with its localized status, the author, and the public repository
address. The brand tile is decoded once from `assets/brand/breeze-icon.png` into a
bounded BGRA texture; it is never fetched over the network. An available release
still offers its GitHub page, and the window-level update banner is suppressed while
a non-library Home page already shows that state.

Shortcuts lists the registered bindings through `Kbd::binding_for_action` and
`Kbd::global_binding_for_action`, so the page cannot invent or drift from a real
binding. Rows whose action has no registered binding are omitted, and one muted line
states that custom shortcuts are not supported yet. Settings keeps a title and one
control per row; explanatory subtitles are not added for their own sake. The update
*policy* stays in Settings, now limited to Manual and Automatic, while the *check*
action and its result live in About. Version-8 state maps the retired "Never" policy
to Manual on read; older snapshots carrying it still validate.

Home provides compact list and cover-grid views with recent/favorite/category filters. Library entries refer to original files; do not silently import copies. New libraries are empty. Legacy sample entries remain ordinary removable entries to preserve reading state and annotations. Both views use real first-page thumbnails, loaded only for visible entries, and share the same document actions. Grid metadata is centered and action icons appear only on card hover. Empty results use a minimal Kit Empty with an icon and “暂无文档”, without descriptions or extra actions.

The reader has independent one/two-page and continuous-scrolling choices, defaulting to continuous single-page reading. Its status menu places Single (`file`) and Two pages (`columns-2`) above a separator, with Continuous (`gallery-vertical`) below it. The trigger shows an icon and 单页 / 双页 / 连续单页 / 连续双页. Continuous two-page reading virtualizes spreads and retains a lone final page for odd page counts. Outline and thumbnail navigation are virtualized across the document; raster requests cover visible ranges, not every page. Cancellable search and rectangular glyph selection/copy are implemented. Every displayed surface carries its page identity, including annotation gestures. State must never label an old rendered page as a newly requested page.

Fit Page, Fit Width, Actual Size, and Custom Ratio share one status dropdown that displays only the mode icon/label. A separate percentage between Zoom Out/In always shows the calculated display scale, updating when the viewport, page geometry, or fit mode changes. Actual Size and Custom Ratio at 100% remain distinct modes. Primary+0 selects Actual Size; modified-wheel events zoom without also scrolling the document. Percentage zoom remains bounded to 25–200%. Buttons, keyboard zoom, and discrete mouse-wheel steps switch to Custom Ratio at the next 25% boundary in the requested direction: 167% goes to 175% or 150%, while 175% goes to 200% or 150%. Smooth trackpad zoom remains continuous and also switches to Custom Ratio. Selecting Custom Ratio captures the current rounded percentage. Keyboard zoom does not consume Input editing keys.

View rotation is per document and view-only: Rotate Left / Rotate Right sit immediately left of the layout menu and advance the displayed orientation by one clockwise or counterclockwise quarter turn. Rotated pages keep their own `/Rotate` and are never rewritten in the source file; annotation geometry stays in unrotated PDF user space and is mapped through the combined rotation for display and gestures. Raster tiles and navigation thumbnails are rendered in the rotated space, so a quarter turn exchanges the displayed width and height while text selection remains valid. Page text, outlines, and annotation coordinates are unaffected.

Fullscreen reading is the rightmost status-bar control. It requests native window fullscreen and renders only the document: the title bar, navigation panel, document sidebar, reader toolbar, and status bar are absent, and the page area uses the entire window. The per-document layout, zoom, and rotation are preserved, and fit modes refit to the larger area. Escape leaves fullscreen when no text annotation is being edited; F11 toggles it in both directions. This is an immersive reader mode, not hidden document content: nothing is erased, rasterized, or replaced.

Layout and zoom persist per document independently of the reading-position switch. State version 5 remembers the mode and custom percentage per document and as defaults for new documents. Legacy 100% values migrate to Actual Size, matching their previous mode label; version-5 custom 100% remains Custom Ratio. Other explicit document layout/zoom choices survive migration, and missing new-document preferences default to continuous single-page / Fit Page.

The prototype raster is fixed at 144 DPI; zoom currently changes display size. Do not claim adaptive high-DPI rendering. Actual Size uses 72 document points to 96 logical pixels, not physical-centimeter calibration.

## Appearance

Use one GPUI Kit Select labeled Theme, replacing the segmented mode control and separate light/dark palette rows:

- Follow System is the default application preference. It subscribes to window appearance events rather than polling and resolves to Kit's default light/dark theme.
- Bundle the complete official GPUI Kit gallery from the same source commit as the pinned 0.7.0 dependency; see [theme provenance](../../assets/themes/README.md). Core Kit initialization provides two default themes, and Breeze explicitly registers the other 36 upstream theme definitions from compiled-in assets. No runtime network request or theme-directory watcher is used.
- Populate a searchable Select from the registry, grouped as System / Light / Dark (Chinese section labels). There are 39 choices: Follow System, 12 light themes, and 26 dark themes. Keep the default theme first within each appearance group, followed by case-insensitive name order; retain upstream theme names.
- Selecting a named theme uses its declared appearance and ignores OS appearance changes. Persist its registry name, not its dropdown index.
- Apply the selected `ThemeConfig` through `Theme::update` / `apply_config`. Application chrome reads the active Kit theme colors; Breeze no longer overrides them with its own palettes. Preserve compact typography and control radii.
- Primary+Shift+T cycles the same choices and keeps the settings selection synchronized.
- Moving the dropdown cursor with the up/down arrow keys previews the highlighted theme immediately, including rows that a search query has filtered to. The theme is committed only by confirming the row (Enter or a click), which is also when the preference is persisted; Escape, an outside click, or focus loss restores the committed theme. The trigger and the row checkmark keep showing the committed choice while a preview is on screen.

PDF pages are not inverted, tinted, or rerendered by theme selection. Theme preference, library state, and restoration switches persist locally. Legacy states retain Follow System or migrate explicit Light/Dark to the corresponding built-in name; retired palette fields are accepted on read but omitted from new snapshots. The language dropdown applies English, Simplified Chinese, or Traditional Chinese to application chrome immediately and persists the choice; user names, paths, document content, and official theme names are preserved. Update checks use the user-designated `poneding/breeze` stable GitHub Releases source. Manual is the default, Automatic checks once per startup, and Never prevents background checks. Explicit checks remain available in every policy. A newer release offers its GitHub page; automatic download/install is unavailable. Opening-window policies are implemented separately from page zoom and applied once for new/restored active documents.

## Annotation modes

All twelve requested annotation tools operate on the independent sidecar layer. The reader toolbar ends with Search and Favorite; the document-sidebar toggle stays at the toolbar's right edge only while the sidebar is closed. Search is available in every mode. The optional 256 px right sidebar starts closed and is independent of Reading/Annotate mode. It is a sibling of the reader content below the title bar, so its 38 px header aligns with the reader toolbar instead of starting below it. Its header uses separate Information and Annotations icon buttons with the same spacing on the left and pins the selected document-sidebar toggle to its right edge; the active header button is selected for the current view. While document search is open, its input and close action replace those two header buttons in the same row, and the toggle remains at the right edge.

Information shows the current document's name, format, page count, current page dimensions, categories, annotation count, and file/sidecar locations. Annotations lists the current document's sidecar items across all pages in page order, with an all-types/type filter and an optional current-page filter. Rows show type, color, page, and note text when present; clicking navigates to the page. Deletion affects the chosen sidecar item and supports undo/redo. The list is virtualized. Existing embedded PDF annotations are not imported into editable history. Document text search opens the same right sidebar automatically and shows its results below the toolbar; it never replaces the left Outline/Thumbnails navigation. Opening search records whether the right sidebar was already open. Closing search with its close button, Search command, or Escape restores that state, so a temporarily opened sidebar closes while an already open sidebar remains visible.

Annotation tools open Kit hover cards after a short pointer dwell. The pointer can enter the card and adjust its controls; leaving both trigger and card closes it after a short grace period. Keyboard activation opens a focusable Kit popover dismissed by Escape. Hovering alone never selects a different tool. Colors, line widths, and opacity are remembered independently for each annotation tool for the current application session. Line width uses a decimal Kit NumberInput (0.25–100 pt, 0.25 pt steps); opacity uses a continuous 0–100% Kit Slider. The eraser explains whole-stroke removal and has no irrelevant color/width controls.

The Type text-annotation tool opens an empty native Input at the clicked page position after mouse release. Enter commits one annotation; Escape cancels. IME composition confirmation does not commit partial text. Text is displayed directly with the annotation's foreground and opacity, without a sticky-note background. Double-click an existing text annotation or use its selected Edit Text control to revise it. Unfinished input is retained and blocks navigation, closing, saving, and export until completed or cancelled. Existing PDF-copy export still discloses and emits standard sticky notes with exact Unicode contents.

Click sidecar annotations in Reading or Annotate mode to select them; Annotate also offers a selection pointer. A selection outline and nearby controls allow color, width, and opacity changes, text editing, and deletion. Delete/Backspace removes the selection outside native inputs. Non-text-markup annotations can be dragged within their page; text markup remains tied to its glyph geometry. A slider drag, line-width confirmation, text confirmation, or completed move forms one undoable edit. Source fingerprint, history revision, index, and the original item protect edits from stale or shifted targets. Changing another annotation, undo/redo, and document navigation invalidate old selections. Original PDF content is outside these hit targets.

Use the same eight fixed colors for category tags and annotation swatches: red `#EF4444`, orange `#F97316`, amber `#F59E0B`, green `#22C55E`, teal `#14B8A6`, blue `#3B82F6`, violet `#8B5CF6`, and pink `#EC4899`. These colors are independent of application appearance. Annotation opacity is exactly the displayed percentage; new text/area highlights default to 40% amber, while strokes and notes default to 100%. Existing stored annotations retain their color and opacity.

Show annotation load/save/conflict state, undo/redo, Save, Save As, and export controls in the Annotations sidebar. Surface annotation errors in the reader status as well. Never imply successful file changes.

## Tooltips, accessibility, and shortcuts

Use Kit's native `Tooltip`, `Tooltip::action`, and `Button::tooltip_with_action`. Shortcut labels come from actual Action bindings. Icon-only controls require accessible names, meaningful tooltips, and keyboard focus.

`Primary` means Command on macOS and Control on Windows/Linux:

| Command | Shortcut |
| --- | --- |
| Open / Settings | Primary+O / Primary+, |
| Home | Primary+Shift+H |
| Close current document tab | Primary+W |
| Next / previous tab | Ctrl+Tab / Ctrl+Shift+Tab |
| Toggle navigation panel | Primary+B |
| Focus page input / submit | Primary+G, then Enter |
| Previous / next page | PageUp / PageDown |
| Reading / Annotate | Primary+1 / 2 |
| Outline / Thumbnails | Primary+Shift+O / Primary+Shift+M |
| Favorite | Primary+D |
| Zoom in / out | Primary++ (also Primary+=) / Primary+-; Primary+wheel over the document |
| Actual size / fit page / fit width | Primary+0 / 9 / 8 |
| Cycle theme | Primary+Shift+T |
| Export annotated PDF copy | Primary+Shift+E |

Do not occupy macOS Command+H with Home. Page and zoom bindings exclude the Input context. Mode shortcuts switch UI modes. Primary+F opens search; Primary+C copies the selected glyph text; Primary+S saves annotations; Primary+Shift+S chooses a new sidecar destination; Primary+Z / Primary+Shift+Z undo/redo annotations outside Input. Primary+Shift+E exports annotations to a new PDF. The source PDF is never overwritten by these commands.

## Required native checks

In addition to component tests, inspect titlebar dragging/double-click/window controls, narrow-window reachability, long tab labels, theme changes, tooltip placement, IME, and keyboard navigation. Windows and Linux require their own native validation. Never record unavailable or interrupted checks as passed.

## September 30 refinement

Use Kit Small controls instead of XSmall: the default 15 px rem gives global icon buttons a 22.5 px hit box. Keep the title bar, toolbar, navigation header, and status bar heights unchanged. Left alignment must be applied to a full-width child row inside Kit Button; outer `justify_start` alone does not override the component's centered content container.

Document tabs have one four-corner rounded surface, a theme-background active fill with a small shadow, and no inter-tab separators. Inactive tabs receive the same shadow on hover, including over their Close overlay. Home and the tab group's trailing divider/Plus control retain their control widths; Kit's Windows/Linux window controls occupy their own sibling region. Each tab fits its filename, icon, and normal padding up to 228 logical pixels; short names do not stretch to the maximum. Tabs shrink from their preferred widths as more documents open or the window narrows. At the 40 px minimum, excess tabs scroll within that bounded region. Preserve 4 px of leading space and 6 px of trailing space inside the tab scroller so edge shadows remain visible. A single 1 × 14 px divider and Plus button follow the visible tab region, including when it is empty. Plus opens the document picker, uses muted foreground at rest and ordinary button foreground/background on hover, and is never part of the scrolling content. The old right-side Open button is removed.

Each tab's icon/title content has equal 0.5 rem left and right insets (7.5 logical pixels at the current 15 px rem). Preferred widths include both sides, keeping the right edge of short titles as comfortably inset as the left document icon. Close uses a separate 2 px right inset; its overlay does not inherit the title's larger inset.

Close is an inset, hover-only overlay rather than a reserved layout slot. Text uses the full available width and does not reflow on hover; a 12 px transparent-to-surface gradient fades it into the solid area around the 18.5 px Close button. Close retains independent feedback and never activates its document. The parent tab keeps its hover surface over the overlay. Primary+W remains available without hovering.

Named library categories are user-owned. A fresh library has only the Unassigned filter, not preset named categories. The category group uses `tags`; each category uses `tag` with one of eight persistent colors (red, orange, amber, green, teal, blue, violet, pink). Create/rename Kit Popovers allow choosing the name and color without reflowing the sidebar. The entire category row is one interactive surface. Rename/delete icon buttons occupy that same row and have independent hover/click feedback. They appear on hover and remain visible while their editor is open. Assignment menus independently check multiple categories; deleting one preserves other memberships.

## October 15 library actions

Library rows and covers keep one action set: favorite and remove, plus a folder-open action that reveals a file-backed document in the platform file manager — “在访达中显示” on macOS, “在文件资源管理器中显示” on Windows — and stays absent for the bundled in-binary sample. Reveal never opens, moves, or edits the document; a missing file or failed launcher is reported instead of silently doing nothing. The document row itself remains the only Open control, so no separate “阅读文档” action is shown.
