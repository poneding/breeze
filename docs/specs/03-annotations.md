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
| Text note | Anchor, text, and style | Text or FreeText, distinguishing a sticky note from a text box |
| Circle/rectangle | Bounds, stroke, fill | Circle / Square |
| Line/arrow | Endpoints, width, ending styles | Line / LE |
| Freehand wavy line | Path | Ink or explicit appearance |

Do not flatten all annotations merely to make screenshots look correct. Flattening may be offered only as an explicit export-to-copy choice. Validate exported subtype/appearance behavior in independent readers.

## Persistence and integrity

- Save annotation state to a sidecar only on an explicit user action; never rewrite the entire PDF to persist a stroke.
- Keep unsaved annotation state in the session and refuse close, quit, tab close, or library removal until the user saves, keeps a recovery copy, or explicitly discards it.
- Preserve annotation undo/redo semantics independently of the source document.
- Fingerprint the source before applying saved sidecar state. Stop on mismatches or unsupported objects.
- Prefer Save As. Saving failures must not damage the original document.
- A full rewrite can leave historical/orphaned information. Do not advertise sidecar or export behavior as secure erasure.

## Implemented sidecar workflow

Completed gestures create native overlays in PDF user-space coordinates. Selection-based deletion, annotation removal from the document-wide filtered sidebar, and bounded undo/redo modify only annotation state. Tool settings use fixed document colors and explicit opacity, independent of the application theme. Sidecars are written only when the user saves (Save, Save As, or Primary+S); an edit stays in the session and is reported as unsaved, and every write still carries a source SHA-256 and optimistic sidecar revision checks. Read/write/conflict errors remain visible and disable overwrite. Save As selects a new non-clobbering destination and persists its association. Close/quit is refused while annotations are dirty or a save is pending; normal closing also waits for application-state persistence. Because nothing is written before the user saves, an unexpected termination can lose that session's annotations; the refusal paths above are the mitigation, not background writes.

Text selection follows reading order: the run between the two carets nearest the gesture, as in a text editor, not every glyph inside a dragged rectangle. Lines sharing a baseline form one row; successive rows of a paragraph share one highlight edge, which closes the leading a translucent fill would otherwise show as a seam or a double band on tall CJK font boxes, while a paragraph break stays open. Copy inserts a newline per row. A selection may also span the pages on screen: each page contributes its own rows, a page between the two carets is selected whole, and copy joins pages with a newline. A pointer past a page's edge clamps to that page, and a pointer in a gap between pages belongs to the nearer one. Holding a drag past the reading viewport's edge scrolls the document and keeps selecting. A page whose text has not been prepared keeps the selection where it is, then joins it when its text arrives, without another pointer move. Runs too steep to read along a page axis, and unavailable extraction, are not guessed. Text annotations have an anchor and plain text entered on the page, not full rich-text editing. The sidecar layer supports direct selection, style changes, text revision, deletion, and movement of non-text-markup annotations; numeric and pointer drafts commit as single history transactions. Source identity and a transient history revision guard stale selections. Wavy lines are free paths. Restricted standard annotation export is implemented. Text annotations are explicitly exported as Unicode sticky notes with an icon, not as guessed-font inline FreeText. Existing PDF annotations are preserved, not imported into editable history. Forms/widgets, signatures, encryption, unsupported UserUnit, and existing destinations are rejected. See [work package 05](../plans/05-annotation-export.md).

## Stroke appearance

Native stroked annotations use round caps and joins. Free waves use continuous
cubic curves with a wavelength proportional to the line width, reduced amplitude
for short gestures, and endpoints on the stored gesture anchors. Ellipses also
use cubic curves. Curve geometry is shared by display, selection, and explicit
PDF export; an exported wave has a cubic appearance and a sampled editable
InkList. These appearance improvements do not modify the persisted gesture points
or source page content.

## Selection and resizing

Selected annotations use a soft blue glow under a narrow core line, independent
of the application theme so selection stays visible on light and dark page
content. The frame and its handles paint as one canvas, with every edge centered
on a device pixel; element borders on the page transform's fractional
coordinates gave the four sides visibly different weights.
The Select annotation button uses Kit's filled primary variant while active.
Selected ellipses, rectangles, and area highlights expose eight resize handles:
side handles change one dimension; corner handles change both. Handle targets
remain 16 logical pixels wide regardless of document zoom. Resizing follows the
displayed page axes, including native and reader rotation, keeps the opposite
edge fixed, and clamps to the page bounds and a 0.5-point minimum dimension.
A completed drag is one undoable change to the sidecar annotation. Escape cancels
the draft, and source/history identity guards still reject stale edits.

The eraser button, settings, and whole-stroke erasure path are removed. Delete
and the annotation list remain the explicit ways to remove annotations.
