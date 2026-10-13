# Annotations

Annotations are an independent sidecar layer. They are not edits to the PDF page content, and Breeze does not expose PDF content editing.

## Annotation model and export

Store geometry in document coordinates, not viewport pixels. Prefer standard, editable PDF annotations with valid appearance streams on explicit export.

| Tool | Sidecar geometry | PDF export candidate |
|---|---|---|
| Text highlight | Glyph quadrilaterals | Highlight / QuadPoints |
| Underline, strikeout, text wavy line | Glyph quadrilaterals | Underline / StrikeOut / Squiggly |
| Area highlight | Rectangle and opacity | Square or quadrilateral Highlight, subject to viewer testing |
| Pen | Stroke paths | Ink |
| Eraser | Delete/split editable ink commands | Update corresponding Ink; never erase original content |
| Text note | Anchor, text, and style | Text or FreeText, distinguishing a sticky note from a text box |
| Circle/rectangle | Bounds, stroke, fill | Circle / Square |
| Line/arrow | Endpoints, width, ending styles | Line / LE |
| Freehand wavy line | Path | Ink or explicit appearance |

Do not flatten all annotations merely to make screenshots look correct. Flattening may be offered only as an explicit export-to-copy choice. Validate exported subtype/appearance behavior in independent readers.

## Persistence and integrity

- Autosave annotation state to a sidecar, not the entire PDF after every stroke.
- Preserve annotation undo/redo semantics independently of the source document.
- Fingerprint the source before applying saved sidecar state. Stop on mismatches or unsupported objects.
- Prefer Save As. Saving failures must not damage the original document.
- A full rewrite can leave historical/orphaned information. Do not advertise sidecar or export behavior as secure erasure.

## Implemented sidecar workflow

Completed gestures create native overlays in PDF user-space coordinates. Ink erasure, annotation removal from the document-wide filtered sidebar, and bounded undo/redo modify only annotation state. Tool settings use fixed document colors and explicit opacity, independent of the application theme. A 500 ms debounce writes sidecars with a source SHA-256 and optimistic sidecar revision checks. Read/write/conflict errors remain visible and disable automatic overwrite. Save As selects a new non-clobbering destination and persists its association. Close/quit is refused while annotations are dirty or a save is pending; normal closing also waits for application-state persistence.

Current text selection is a glyph-center rectangular selection; rotated glyph runs and unavailable extraction are not guessed. Text annotations have an anchor and plain text entered on the page, not full rich-text editing. The sidecar layer supports direct selection, style changes, text revision, deletion, and movement of non-text-markup annotations; numeric and pointer drafts commit as single history transactions. Source identity and a transient history revision guard stale selections. Wavy lines are free paths. Restricted standard annotation export is implemented. Text annotations are explicitly exported as Unicode sticky notes with an icon, not as guessed-font inline FreeText. Existing PDF annotations are preserved, not imported into editable history. Forms/widgets, signatures, encryption, unsupported UserUnit, and existing destinations are rejected. See [work package 05](../plans/05-annotation-export.md).
