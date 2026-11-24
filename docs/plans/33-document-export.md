# Reader menus and document conversion

Requested on November 13, 2026. This work adds a favorite toggle to the content
context menu and a compact More button immediately after the toolbar favorite.
Both menus share an Export submenu: Markdown offers PDF and Word (.docx), and
PDF offers Markdown and Word. The existing annotated PDF export stays available.
EPUB conversion is outside this request.

## Implementation contract

- Use Kit DropdownMenu/PopupMenu and the existing favorite mutation. Keep the
  38 px toolbar, clipboard commands, narrow-window scrolling and pane ownership.
- Snapshot the document when export is requested, including an open Markdown
  draft. Export does not save the draft or acknowledge a sidecar save.
- Run conversion and file I/O on a background executor. Allow one outstanding
  export per pane, including the destination picker. Publish a temporary file
  only to a new destination, checking the source fingerprint before publication.
- Reuse markdown-rs through the existing bounded AST parser. Generate Word with
  pinned native Rust bindings and PDF with the existing MuPDF HTML backend.
  Preserve headings, paragraphs, inline emphasis, lists, tables and code.
- The user selected preservation of text, images and layout. PDF conversion
  retains styled editable text at its original page coordinates in Word,
  composed images at source density and vector regions at 288 DPI. Markdown
  reflows styled text and links sibling PNG assets; its syntax cannot reproduce
  absolute PDF page layout. Scanned pages retain their image without OCR.
  Installed Word fonts and complex PDF layouts still require fidelity checks.
- Preserve source files, existing destinations and unsaved state on success,
  cancellation and errors. Report unsupported content explicitly.
- On November 23, 2026, the user requested export outcomes in a popup. Show
  conversion and annotated-PDF success or failure in a centered Kit AlertDialog
  in the originating window, including the destination and conversion notes.
  Clear export progress when work ends; do not retain receipts in the reading
  status bar. Cancelling the file picker produces no outcome dialog.

## Verification

Exercise the real PDF and Markdown menus, favorite state and format options.
Check exported PDF text and Word package structure, Unicode, lists/tables,
source conflicts, existing destinations and drafts. Run formatting, workspace
tests and Clippy, then launch with isolated application data under `target/`
for native Windows inspection. Record completed checks and remaining fidelity
or platform limits in STATUS; commit the verified phase locally.
