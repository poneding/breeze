# Draggable tabs and two reader panes

Requested October 16, 2026. Implementation branch: `feat/draggable-split-tabs`.

## Scope

Drag a document tab onto the left or right half of the reading area to move it
into a second pane. Require at least two open documents. The workspace permits
one horizontal split, never a nested split or more than two panes. Tabs can also
be reordered in a group or moved between the existing groups. An empty group
collapses back to one reader without closing the other group's documents.

Each pane owns its navigation sidebar, document sidebar, reading content,
toolbar, status bar, focus, search, rendering worker, and transient gestures.
The native title bar contains the two corresponding tab groups. The divider is
a Kit Resizable; narrow panes scroll controls instead of hiding them.

## Implementation boundary

`breeze-app/src/split.rs` coordinates the outer Breeze view and one optional peer
reader entity. The peer reuses the existing native PDF, EPUB and Markdown reader
components. It never opens a second application-state store. The outer view
retains library records for peer-owned documents, while the full `Book` —
including its annotation history, save identity, and Markdown editor entity —
has one owner and moves intact. In-flight annotation inspection is reissued by
its new owner. Save/export jobs and unfinished text-note input block a transfer.

Library actions reach the owning pane. Peer notifications update the outer
library's reading metadata and the existing state writer. Both panes participate
in the window-close guard. PDF fit and raster geometry use each pane's measured
content viewport, rather than the full native window dimensions.

The split arrangement, divider position, and tab order are session UI state.
Existing persisted document positions, zoom, favorites, memberships, opened
files, and sidecar paths remain supported; restarting restores the open files
into one tab group. No storage schema migration or new document engine is needed.

## Verification

One implementation phase covers native drag/drop, ownership and focus routing,
layout, data-integrity regressions, and documentation. Verify before its local
commit:

- Real pointer events create left/right splits, reorder tabs, move tabs between
  groups, and never create a third pane.
- Pane-local navigation, zoom, sidebars, search and focus remain independent;
  fit geometry and tab widths match the actual split layout.
- Moving and collapsing preserve live annotation/draft ownership, and save and
  close guards reach both panes. The library and snapshot retain both documents.
- Run formatting, the locked workspace suite and all-target Clippy; start the
  native application with `BREEZE_DATA_DIR` beneath `target/` and inspect it.

Executed checks and platform limitations belong in [STATUS](STATUS.md).
