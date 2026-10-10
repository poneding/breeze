# Compact UI Design

## Accepted direction

The user accepted the compact prototype's general layout on September 29, 2026. Preserve its content-first direction rather than the superseded promotional layout.

- No application branding block, welcome poster, decorative sidebar footer, or redundant document-information card.
- Document tabs occupy the native GPUI Kit TitleBar; do not stack a second title bar beneath OS chrome.
- Home, Open, Settings, theme, and other global commands use icon buttons with accessibility labels and tooltips.
- Keep necessary text on document tabs, navigation tabs, and Reading/Annotate/Edit mode tabs.
- Use restrained separators, small radii, a 4-pixel spacing system, and no decorative glass/gradient effects.

## Layout and density

```text
Native title bar: window controls | Home | document tabs | Open | theme | Settings
+---------------------------+---------------------------------------------------+
| Outline / Thumbnails tabs | Reading / Annotate / Edit + contextual tools       |
|                           +---------------------------------------------------+
| Embedded outline or       | Document viewport              | optional         |
| scrollable thumbnails         |                                | properties       |
|                           +---------------------------------------------------+
|                           | Page navigation | layout | fit mode | zoom        |
+---------------------------+---------------------------------------------------+
```

Current logical dimensions:

| Element | Size |
|---|---|
| Title bar | 34 px, with Kit-managed native controls |
| Reader toolbar | 38 px |
| Status bar | 32 px |
| Page margin | 12 px |
| Navigation panel | Default 208 px; resize range 192–360 px |
| Main reader split | Minimum 430 px |
| Root UI text / Kit base font | 13 px / 15 px |

Use Kit's resizable panels and left-aligned segmented Outline/Thumbnails tabs. Remember the sidebar's session width when collapsed. Fit calculations follow the measured panel width, not a hard-coded value. Center thumbnails horizontally; constrain long outline labels to their available width. Existing saved widths below 192 px are raised to the new minimum without rejecting the saved session.

Let the Kit splitter draw the navigation boundary as a single 1 px line in the same color as the other chrome borders; the navigation panel must not add a second border. Center its 4 px drag indicator on that line, retaining the Kit hover/press/drag transitions and hit area.

Show the Outline tab only when the current document has outline entries. Otherwise, show Thumbnails automatically while retaining the user's navigation-view preference for documents with an outline. Keep known document outlines available during page loading. Thumbnails use a responsive grid of compact cards: one column at 192/208 px, two at 280 px, and three at 360 px, with a maximum card width of 144 px. Preserve page aspect ratios, page numbers, the current-page border, and empty slots in the final row. Virtualize rows and map them to page ranges for loading and navigation; retain the top visible page when the sidebar is resized.

In Outline view, the header's right edge contains title search when entries exist. Nested outlines also show an `ellipsis-vertical` dropdown with Expand All (`list-chevrons-up-down`) and Collapse All (`list-chevrons-down-up`); these actions do not navigate. A flat outline reserves no disclosure-icon column. In a nested outline, titles navigate and chevrons independently fold descendants. The active section uses the nearest resolved preceding destination, with deeper headings preferred on page ties, and promotes its highlight to a visible collapsed ancestor.

Title search opens a separate 34 px row below the unchanged 38 px header. It shares the document search row's bottom border, horizontal padding, control gap, and vertically centered Kit Small input/close button. The header supplies the top divider; both rows keep equal vertical breathing room around their controls. Match titles case-insensitively, retain matching ancestors, and reveal matches inside previously collapsed branches. Search uses temporary folding choices; clearing the query or closing search restores the original folds, and closing also restores the outline scroll position. Escape closes search after the input's native handling. Search and fold controls are absent from Thumbnails view; search state resets with the loaded source/fingerprint. Show an explicit empty result when no title matches.

The page itself retains the document's font and colors. UI density changes do not edit PDF content.

## Home and reader behavior

Home provides compact list and cover-grid views with recent/favorite/category filters. Library entries refer to original files; do not silently import copies. New libraries are empty. Legacy sample entries remain ordinary removable entries to preserve reading state and annotations. Both views use real first-page thumbnails, loaded only for visible entries, and share the same document actions. Grid metadata is centered and action icons appear only on card hover. Empty results use a minimal Kit Empty with an icon and “暂无文档”, without descriptions or extra actions.

The reader has independent one/two-page and continuous-scrolling choices, defaulting to continuous single-page reading. Its status menu places Single (`file`) and Two pages (`columns-2`) above a separator, with Continuous (`gallery-vertical`) below it. The trigger shows an icon and 单页 / 双页 / 连续单页 / 连续双页. Continuous two-page reading virtualizes spreads and retains a lone final page for odd page counts. Outline and thumbnail navigation are virtualized across the document; raster requests cover visible ranges, not every page. Cancellable search and rectangular glyph selection/copy are implemented. Every displayed surface carries its page identity, including annotation gestures. State must never label an old rendered page as a newly requested page.

Fit Page, Fit Width, Actual Size, and Custom Ratio share one status dropdown that displays only the mode icon/label. A separate percentage between Zoom Out/In always shows the calculated display scale, updating when the viewport, page geometry, or fit mode changes. Actual Size and Custom Ratio at 100% remain distinct modes. Primary+0 selects Actual Size; modified-wheel events zoom without also scrolling the document. Percentage zoom remains bounded to 25–200%. Buttons, keyboard zoom, and discrete mouse-wheel steps switch to Custom Ratio at the next 25% boundary in the requested direction: 167% goes to 175% or 150%, while 175% goes to 200% or 150%. Smooth trackpad zoom remains continuous and also switches to Custom Ratio. Selecting Custom Ratio captures the current rounded percentage. Keyboard zoom does not consume Input editing keys.

Layout and zoom persist per document independently of the reading-position switch. State version 5 remembers the mode and custom percentage per document and as defaults for new documents. Legacy 100% values migrate to Actual Size, matching their previous mode label; version-5 custom 100% remains Custom Ratio. Other explicit document layout/zoom choices survive migration, and missing new-document preferences default to continuous single-page / Fit Page.

The prototype raster is fixed at 144 DPI; zoom currently changes display size. Do not claim adaptive high-DPI rendering. Actual Size uses 72 document points to 96 logical pixels, not physical-centimeter calibration.

## Appearance

Separate `ThemePreference` from `ColorScheme`:

- Follow System is the default application preference. It subscribes to window appearance events rather than polling and resolves to Kit's default light/dark theme.
- Bundle the complete official GPUI Kit gallery from the same source commit as the pinned 0.7.0 dependency; see [theme provenance](../../assets/themes/README.md). Core Kit initialization provides two default themes, and Breeze explicitly registers the other 36 upstream theme definitions from compiled-in assets. No runtime network request or theme-directory watcher is used.
- Populate a searchable Select from the registry, grouped as System / Light / Dark (Chinese section labels). There are 39 choices: Follow System, 12 light themes, and 26 dark themes. Keep the default theme first within each appearance group, followed by case-insensitive name order; retain upstream theme names.
- Selecting a named theme uses its declared appearance and ignores OS appearance changes. Persist its registry name, not its dropdown index.
- Apply the selected `ThemeConfig` through `Theme::update` / `apply_config`. Application chrome reads the active Kit theme colors; Breeze no longer overrides them with its own palettes. Preserve compact typography and control radii.
- The titlebar button and Primary+Shift+T cycle the same choices and keep the settings selection synchronized.

PDF pages are not inverted, tinted, or rerendered by theme selection. Theme preference, library state, and restoration switches persist locally. Legacy states retain Follow System or migrate explicit Light/Dark to the corresponding built-in name; retired palette fields are accepted on read but omitted from new snapshots. Language/update selectors remain behavior previews. Opening-window policies are implemented separately from page zoom and applied once for new/restored active documents.

## Annotation and editing modes

All twelve requested annotation tools operate on the independent sidecar layer. The reader toolbar ends with Search, Favorite, and the document-sidebar toggle in that order. Search is available in every mode. The optional 256 px right sidebar starts closed and is independent of Reading/Annotate/Edit mode. It is a sibling of the reader content below the title bar, so its 38 px header aligns with the reader toolbar instead of starting below it. Its header uses separate Information and Annotations icon buttons; the document-sidebar toggle remains selected while the sidebar is open, and the active header button is selected for the current view.

Information shows the current document's name, format, page count, current page dimensions, categories, annotation count, and file/sidecar locations. Annotations lists the current document's sidecar items across all pages in page order, with an all-types/type filter and an optional current-page filter. Rows show type, color, page, and note text when present; clicking navigates to the page. Deletion affects the chosen sidecar item and supports undo/redo. The list is virtualized. Existing embedded PDF annotations are not imported into editable history. Document text search opens the same right sidebar automatically and replaces its body with the search input and result list; it never replaces the left Outline/Thumbnails navigation.

Annotation and editing tools open Kit hover cards after a short pointer dwell. The pointer can enter the card and adjust its controls; leaving both trigger and card closes it after a short grace period. Keyboard activation opens a focusable Kit popover dismissed by Escape. Hovering alone never selects a different tool. Colors, line widths, and opacity are remembered independently for each annotation tool for the current application session. Line width uses a decimal Kit NumberInput (0.25–100 pt, 0.25 pt steps); opacity uses a continuous 0–100% Kit Slider. The eraser explains whole-stroke removal and has no irrelevant color/width controls. Editing text and insert-text keep separate preview styles.

The Type text-annotation tool opens an empty native Input at the clicked page position after mouse release. Enter commits one annotation; Escape cancels. IME composition confirmation does not commit partial text. Text is displayed directly with the annotation's foreground and opacity, without a sticky-note background. Double-click an existing text annotation or use its selected Edit Text control to revise it. Unfinished input is retained and blocks navigation, closing, saving, and export until completed or cancelled. Existing PDF-copy export still discloses and emits standard sticky notes with exact Unicode contents.

Click sidecar annotations in Reading or Annotate mode to select them; Annotate also offers a selection pointer. A selection outline and nearby controls allow color, width, and opacity changes, text editing, and deletion. Delete/Backspace removes the selection outside native inputs. Non-text-markup annotations can be dragged within their page; text markup remains tied to its glyph geometry. A slider drag, line-width confirmation, text confirmation, or completed move forms one undoable edit. Source fingerprint, history revision, index, and the original item protect edits from stale or shifted targets. Changing another annotation, undo/redo, and document navigation invalidate old selections. Original PDF content is outside these hit targets.

Use the same eight fixed colors for category tags and annotation/edit swatches: red `#EF4444`, orange `#F97316`, amber `#F59E0B`, green `#22C55E`, teal `#14B8A6`, blue `#3B82F6`, violet `#8B5CF6`, and pink `#EC4899`. These colors are independent of application appearance. Annotation opacity is exactly the displayed percentage; new text/area highlights default to 40% amber, while strokes and notes default to 100%. Existing stored annotations retain their color and opacity.

Editing offers text, image, insert-text, and insert-image entry points. The independent style preview is explicitly not a PDF overlay editor. Applying original-content edits stays unavailable until structural editing is validated. Annotation export is a separate enabled, restricted workflow with explicit destination and sticky-note semantics.

Show annotation load/save/conflict state, undo/redo, Save, Save As, and export controls in the Annotations sidebar. Surface annotation errors in the reader status as well. Use concise capability messages for unavailable original-content editing. Never imply successful file changes.

## Tooltips, accessibility, and shortcuts

Use Kit's native `Tooltip`, `Tooltip::action`, and `Button::tooltip_with_action`. Shortcut labels come from actual Action bindings. Icon-only controls require accessible names, meaningful tooltips, and keyboard focus.

`Primary` means Command on macOS and Control on Windows/Linux:

| Command | Shortcut |
|---|---|
| Open / Settings | Primary+O / Primary+, |
| Home | Primary+Shift+H |
| Close current document tab | Primary+W |
| Next / previous tab | Ctrl+Tab / Ctrl+Shift+Tab |
| Toggle navigation panel | Primary+B |
| Focus page input / submit | Primary+G, then Enter |
| Previous / next page | PageUp / PageDown |
| Reading / Annotate / Edit | Primary+1 / 2 / 3 |
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

Document tabs have one four-corner rounded surface, an elevated active background, separators only between adjacent inactive open documents, and an inset close button revealed on hover. Reserve close-button space to prevent text reflow. Primary+W remains available without hovering.

Named library categories are user-owned. A fresh library has only the Unassigned filter, not preset named categories. The category group uses `tags`; each category uses `tag` with one of eight persistent colors (red, orange, amber, green, teal, blue, violet, pink). Create/rename Kit Popovers allow choosing the name and color without reflowing the sidebar. The entire category row is one interactive surface. Rename/delete icon buttons occupy that same row and have independent hover/click feedback. They appear on hover and remain visible while their editor is open. Assignment menus independently check multiple categories; deleting one preserves other memberships.
