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
| nearby thumbnails         |                                | properties       |
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
| Root UI text / Kit base font | 12 px / 14 px |

Use Kit's resizable panels and centered segmented Outline/Thumbnails tabs. Remember the sidebar's session width when collapsed. Fit calculations follow the measured panel width, not a hard-coded value. Center thumbnails horizontally; constrain long outline labels to their available width.

The page itself retains the document's font and colors. UI density changes do not edit PDF content.

## Home and reader behavior

Home is a compact document list with recent/favorite/category filters. Library entries refer to original files; do not silently import copies. The built-in sample is clearly identified and is not fabricated user history.

The current reader is single-page. It displays real embedded outline data, at most 300 outline entries, and up to four nearby thumbnails. Text selection/search and continuous/spread layouts are not implemented. State must never label an old rendered page as a newly requested page.

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

Apply palette changes to both application chrome and Kit components. PDF pages are not inverted or tinted. Current settings are session-only; persistence is not implemented.

## Annotation and editing modes

All twelve requested annotation tools must remain discoverable. Contextual controls belong near the selected tool or in the optional properties panel. Do not give unused properties permanent viewport space.

Editing offers text, image, insert-text, and insert-image entry points. The independent style preview is explicitly not a PDF overlay editor. Applying edits/export stays unavailable until true structural editing is implemented and validated.

Use concise capability tooltips instead of a permanent banner where the prototype boundary can still be understood clearly. Never imply successful file changes.

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
| Zoom in / out | Primary+= / Primary+- |
| Actual size / fit page / fit width | Primary+0 / 9 / 8 |
| Cycle theme mode | Primary+Shift+T |

Do not occupy macOS Command+H with Home. Page and zoom bindings exclude the Input context. Mode shortcuts only change the current UI mode; they do not imply document-writing capability.

## Required native checks

In addition to component tests, inspect titlebar dragging/double-click/window controls, narrow-window reachability, long tab labels, theme changes, tooltip placement, IME, and keyboard navigation. Windows and Linux require their own native validation. Never record unavailable or interrupted checks as passed.
