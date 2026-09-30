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
| Navigation panel | Default 208 px; resize range 168–360 px |
| Main reader split | Minimum 430 px |
| Root UI text / Kit base font | 13 px / 15 px |

Use Kit's resizable panels and centered segmented Outline/Thumbnails tabs. Remember the sidebar's session width when collapsed. Fit calculations follow the measured panel width, not a hard-coded value. Center thumbnails horizontally; constrain long outline labels to their available width.

The page itself retains the document's font and colors. UI density changes do not edit PDF content.

## Home and reader behavior

Home provides compact list and cover-grid views with recent/favorite/category filters. Library entries refer to original files; do not silently import copies. New libraries are empty. Legacy sample entries remain ordinary removable entries to preserve reading state and annotations. Both views use real first-page thumbnails, loaded only for visible entries, and share the same document actions.

The reader has independent one/two-page and continuous-scrolling choices, defaulting to continuous single-page reading. Its status menu places Single (`file`) and Two pages (`columns-2`) above a separator, with Continuous (`gallery-vertical`) below it. The trigger shows an icon and 单页 / 双页 / 连续单页 / 连续双页. Continuous two-page reading virtualizes spreads and retains a lone final page for odd page counts. Outline and thumbnail navigation are virtualized across the document; raster requests cover visible ranges, not every page. Cancellable search and rectangular glyph selection/copy are implemented. Every displayed surface carries its page identity, including annotation gestures. State must never label an old rendered page as a newly requested page.

Fit Page, Fit Width, Actual Size, and Custom Ratio share one status dropdown that displays only the mode icon/label. A separate percentage between Zoom Out/In always shows the calculated display scale, updating when the viewport, page geometry, or fit mode changes. Actual Size and Custom Ratio at 100% remain distinct modes. Primary+0 selects Actual Size; modified-wheel events zoom without also scrolling the document. Percentage zoom remains bounded to 25–200%. Buttons, keyboard zoom, and discrete mouse-wheel steps switch to Custom Ratio at the next 25% boundary in the requested direction: 167% goes to 175% or 150%, while 175% goes to 200% or 150%. Smooth trackpad zoom remains continuous and also switches to Custom Ratio. Selecting Custom Ratio captures the current rounded percentage. Keyboard zoom does not consume Input editing keys.

Layout and zoom persist per document independently of the reading-position switch. State version 5 remembers the mode and custom percentage per document and as defaults for new documents. Legacy 100% values migrate to Actual Size, matching their previous mode label; version-5 custom 100% remains Custom Ratio. Other explicit document layout/zoom choices survive migration, and missing new-document preferences default to continuous single-page / Fit Page.

The prototype raster is fixed at 144 DPI; zoom currently changes display size. Do not claim adaptive high-DPI rendering. Actual Size uses 72 document points to 96 logical pixels, not physical-centimeter calibration.

## Appearance

Separate `ThemePreference` from `ColorScheme`:

- Follow System, Light, and Dark are the three modes; Follow System is the default.
- Light and dark palettes are selected independently and survive mode changes within the session.
- Follow System subscribes to window appearance events rather than polling.
- Explicit Light/Dark preferences ignore OS appearance changes.

| Light palette | Dark palette |
|---|---|
| Warm white — default | Dark gray — default |
| Sand | Warm charcoal |
| Mist blue | Midnight blue |
| Light forest | Dark forest |
| Pale violet gray | Dusk violet |

Apply palette changes to both application chrome and Kit components. PDF pages are not inverted or tinted. Theme preference, palettes, library state, and restoration switches persist locally. Language/update selectors remain behavior previews. Opening-window policies are implemented separately from page zoom and applied once for new/restored active documents.

## Annotation and editing modes

All twelve requested annotation tools now operate on the independent sidecar layer. Contextual controls belong near the selected tool or in the optional properties panel. Do not give unused properties permanent viewport space.

Editing offers text, image, insert-text, and insert-image entry points. The independent style preview is explicitly not a PDF overlay editor. Applying original-content edits stays unavailable until structural editing is validated. Annotation export is a separate enabled, restricted workflow with explicit destination and sticky-note semantics.

Show annotation load/save/conflict state in the properties panel. Use concise capability tooltips for unavailable PDF editing/export paths. Never imply successful file changes.

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
| Cycle theme mode | Primary+Shift+T |
| Export annotated PDF copy | Primary+Shift+E |

Do not occupy macOS Command+H with Home. Page and zoom bindings exclude the Input context. Mode shortcuts switch UI modes. Primary+F opens search; Primary+C copies the selected glyph text; Primary+S saves annotations; Primary+Shift+S chooses a new sidecar destination; Primary+Z / Primary+Shift+Z undo/redo annotations outside Input. Primary+Shift+E exports annotations to a new PDF. The source PDF is never overwritten by these commands.

## Required native checks

In addition to component tests, inspect titlebar dragging/double-click/window controls, narrow-window reachability, long tab labels, theme changes, tooltip placement, IME, and keyboard navigation. Windows and Linux require their own native validation. Never record unavailable or interrupted checks as passed.

## September 30 refinement

Use Kit Small controls instead of XSmall: the default 15 px rem gives global icon buttons a 22.5 px hit box. Keep the title bar, toolbar, navigation header, and status bar heights unchanged. Left alignment must be applied to a full-width child row inside Kit Button; outer `justify_start` alone does not override the component's centered content container.

Document tabs have one four-corner rounded surface, an elevated active background, separators only between adjacent inactive open documents, and an inset close button revealed on hover. Reserve close-button space to prevent text reflow. Primary+W remains available without hovering.

Named library categories are user-owned. A fresh library has only the Unassigned filter, not preset named categories. The category group uses `tags`; each category uses `tag` with one of eight persistent colors (red, orange, amber, green, teal, blue, violet, pink). Create/rename forms allow choosing the color. Rename/delete icon buttons occupy the same sidebar row and appear only on hover. Assignment menus independently check multiple categories; deleting one preserves other memberships.
