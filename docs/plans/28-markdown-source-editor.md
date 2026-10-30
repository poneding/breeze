# Work package 28: Markdown source editor

Planning baseline: October 20, 2026. The user chose the reliable source-editor
route after the live-preview experiments were measured: the only published GPUI
WYSIWYG component (`zorite-editor`) cannot render a 1 MiB document within a
minute, and Velotype is an application rather than a reusable crate. Editing is
now GPUI Kit's code editor with the tree-sitter Markdown grammar; the reading
view is the rendered preview.

Specification 15 owns the parser and projection design. This plan replaces the
editing surface only; it does not add save/recovery, table or image editing, or
a second Markdown renderer.

## Phases

| Phase | Deliverable | Exit evidence | Status |
|---|---|---|---|
| S1 Surface | Host Kit's `EditorState`/`Editor` for Markdown with the `tree-sitter-markdown` feature, file loading, font size, dirty/character tracking and the source status line; remove the Live/Source toolbar pair | `markdown_editor` unit tests and the app's Markdown component tests | Complete (October 20) |
| S2 Verification | Workspace checks, the oversized-document fixture, split/close behaviour and a native smoke run; record the removed surface and the unused mdast projection modules | `fmt`, `test --workspace`, `clippy -D warnings`, STATUS record | Complete except native visual inspection (October 20) |

## S1 decisions

- `breeze-app` enables `gpui-kit`'s `tree-sitter-markdown` feature; the editor
  names its language `markdown` and the highlighter comes from Kit's registry.
- Soft wrap on, line numbers/folding off: a note reads as a page, not a code
  buffer.
- Source and preview are the existing reader Edit/Reading controls. There is no
  separate Live/Source pair; the reader renders the preview.
- Dirty state is derived from the editor text rather than a Change event, so an
  in-progress IME composition and a committed keystroke both count. A byte-length
  read keeps caret movement cheap; contents are compared when the length is
  unchanged.
- The reader's 2 MiB parse ceiling still applies to rendering; editing does not
  need a whole-document parse and opens any file the 16 MiB source session
  admits. Above the ceiling the reader reports the limit and keeps the Edit
  control.
- The reader previews the **draft**, not the file: leaving edit mode re-parses
  the draft text on a worker. The file is never written by the preview.

## Out of scope and follow-ups

Explicit source Save, Undo and Redo were added to the source toolbar on October
30, 2026 (direct save to the source file, as chosen by the user). Save As,
crash recovery, and the remaining [work package 25](25-markdown-live-editing.md)
P4–P7 authoring work remain open. The mdast semantic/projection modules in
`breeze-reflow` are no longer on the application path; a cleanup package may
retire them once the reading model no longer needs the shared syntax helpers.
Native visual inspection of the new surface was not performed in this session
because the model cannot view images; Windows and Linux remain unverified.
