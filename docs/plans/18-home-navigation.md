# Home Navigation, About, Shortcuts, and a Leaner Settings Page

## Request

The user asked for a simpler composition of the application chrome:

- Move Settings into the Home screen instead of a screen of its own.
- Add an Info entry at the bottom of the Home sidebar that shows application name,
  logo, version, update check, author, and source address.
- Add a keyboard-shortcut entry that lists existing shortcuts. Custom shortcut
  editing stays unavailable.
- Add a Settings entry that is activated when the titlebar Settings button is used.
- Remove filler copy from Settings (`外观即时生效`, the data-directory row, the
  GPUI Kit explanation, and similar subtitles). Guiding principle: 大道至简、少即是多.

## Design

Home becomes the single navigation surface. Its left sidebar keeps the library
entries (All documents / Recent / Favorites / categories) in a scrollable region
and gains a compact bottom row of three icon buttons: Info (`info`), Shortcuts
(`keyboard`), and Settings (`settings`). The selected entry is highlighted.

The content area renders exactly one page:

| Selection | Content |
| --- | --- |
| Library entry or category | Current library list/grid, unchanged |
| Info | Breeze logo, name, version, update check and status, author, source address |
| Shortcuts | Existing bound shortcuts grouped by area, with runtime `Kbd` chips |
| Settings | Theme, language, default-PDF-app availability, opening window, restoration switches |

`Screen::Settings` is removed; `Screen::{Home, Reader}` stays, and `HomePage`
selects the Home content. The titlebar Settings button and Primary+, open Home and
select Settings. Primary+Shift+H returns to the library page.

Settings rows keep a title and control only. Explanations are dropped unless they
carry a distinct constraint; the update *policy* selector stays in Settings while
the *check* action and its status live in Info. The update banner is hidden while
a non-library Home page is open because that page already shows the state.

Shortcuts are read from the registered key bindings with
`Kbd::binding_for_action`, so the page never invents or drift from real bindings.
The Quit binding is global and uses `Kbd::global_binding_for_action`. A single
muted line states that custom shortcuts are not supported yet.

## Constraints

- No PDF content, annotation, or persistence behavior changes.
- Sidebar identity, accessibility labels, and focus behavior stay intact.
- The brand mark is decoded from the bundled `assets/brand/breeze-icon.png`,
  downscaled once, and never loaded from the network.
- Cross-platform shortcut labels come from Kit's platform-aware `Kbd` formatting.

## Refinements after the first review, October 13, 2026

- The three global entries are stacked full-width at the bottom of the sidebar in
  the order Shortcuts, About, Settings, each with a tooltip that shows its real
  binding: Primary+K, Primary+I, and Primary+, (Control on Windows/Linux).
- Shortcuts, About, and Settings render in one capped, centered column (720 logical
  pixels) with a small inset before the scroll clip, so an active Select or
  segmented control is not clipped at the right edge.
- About is centered on both axes, and its credits are center-aligned with the muted
  label stacked above the value. The author reads
  `Pone Ding<poneding@gmail.com>`.
- Settings no longer offers the "Never" update policy. State version 8 accepts only
  Manual and Automatic and maps the retired value to Manual on read; older snapshots
  that still carry it continue to validate.

## Verification

- Formatting, workspace tests, all-target Clippy, and isolated startup.
- Component tests: sidebar entries switch pages and mark selection; the titlebar
  Settings button selects the Settings page; the library page remains intact;
  the shortcuts page renders real bindings for a representative action; the Info
  page shows the version and the update check.
- Native visual inspection of the three pages in a controlled data directory.
- Component tests cover the stacked order, the shortcut commands, the capped and
  centered columns, About's two-axis centering, and the exact tooltip keys
  (⌘I/⌘K/⌘, on macOS, Ctrl equivalents on Windows/Linux).
- Native inspection after the screen was unlocked confirmed the stacked sidebar, the
  bounded and centered Settings column, and the two-axis-centered About page.
