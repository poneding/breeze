# Library presentation and categories

User-requested work, September 30, 2026. Complete and commit each item separately.

1. Put All Documents first with the Lucide `library-big` icon.
2. Make the entire document row open the document; secondary controls remain independent.
3. Align list columns and remove the built-in sample from the new-library experience.
4. Add first-page covers and a grid/list view switch with bounded background work.
5. Show the content-header Open command only in All Documents; use `file-plus-corner` for Open.
6. Allow multiple category memberships per document and migrate saved single-category assignments.
7. Use `tags`/`tag` icons, at least seven category colors, and inline hover actions.

The project-authored PDF remains a test fixture. Existing user data and annotations must survive migrations.

## Verification

Each item requires formatting, workspace tests, Clippy, and native macOS review with `BREEZE_DATA_DIR` under `target/`. Windows/Linux runtime acceptance remains outstanding. Generated review state and images are disposable.

## Completed items

- Item 1: All Documents now precedes Recent and Favorites and uses `LibraryBig`. Formatting, 81 workspace tests (three probes ignored), Clippy, and native macOS navigation/icon inspection passed.
- Item 2: The row is a keyboard-accessible Kit Button; favorites, category menus, and removal stop event propagation. Formatting, 82 workspace tests, and Clippy passed. Native inspection confirmed row layout, favorite isolation, and category popup; Open updated isolated state, but the UI capture tool retained the old window after opening, so the post-open screenshot is unverified.
- Item 3: Header and rows share fixed category/position/action widths, the sample label is removed, and new libraries start empty. Existing sample entries remain ordinary removable entries to preserve their state and annotations. Empty-state restoration and column geometry tests pass. Formatting, 83 workspace tests, Clippy, and native macOS list inspection passed.
- Item 4: Virtualized list/grid views now display first-page covers and persist the view choice. The existing bounded PDF worker handles one awaited cover job; the UI cache is limited to 16 MiB/64 entries and each visible request to 32 covers. Unavailable covers have an error icon and explanatory tooltip. Formatting, 85 workspace tests, Clippy, and native macOS list/grid cover inspection passed. A native window resize also refreshed the capture tool and confirmed the earlier whole-row Open transition.
- Item 5: Only All Documents has a content-header Open command; Recent and Favorites use the titlebar command. Both Open icons use `FilePlusCorner`. Formatting, 85 workspace tests, Clippy, and native macOS checks of all three filters passed.
