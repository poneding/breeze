# PDF Editing and Annotations

## Existing-content editing contract

Existing-content editing must target the original content stream, operator, or selected resource usage. The following are prohibited substitutes:

- Covering old text with a rectangle or replacement text.
- Erasing a geometric region and appending replacement content.
- Rebuilding an entire page from extracted text while discarding unrelated graphics.
- Rasterizing a page to simulate editable content.

Insertion is a separate operation. A background rectangle explicitly requested for a text block is valid only when the text itself is genuinely edited; it must not conceal unchanged source text.

The reader writes independent annotation sidecars and restricted annotated PDF copies, but exposes no original-content editing API. Historical operator-edit experiments demonstrate only limited feasibility, not a production editor. See [engine assessment](01-validation.md).

## Proposed structural-editing pipeline

1. Parse page content streams and nested Form XObjects, retaining stream IDs, operator indices, resource scopes, graphics state, transformation matrices, and text state.
2. Map selected glyphs/Unicode back to the source encoding, font/CMap/ToUnicode data, and original operators. Mark unreliable mappings as unsupported.
3. Identify a particular text run or drawing instance, not every object intersecting a screen rectangle.
4. Modify only the target operations. Preserve `TJ` spacing and advances where applicable. Copy shared resources/forms before editing one usage when necessary.
5. Serialize to a new temporary output, reopen it, validate semantics and rendering, and commit the save only after validation.
6. Reject unsupported encodings, scopes, or shared-resource relationships explicitly. Never fall back to cover-up editing.

## Text and image rules

| Operation | Required semantics |
|---|---|
| Font/size | Modify text state and font resources with correct Unicode-to-code/CID mapping and permitted font embedding/subsetting |
| Bold/italic | Select the appropriate font face; do not silently fake an unavailable face |
| Color/opacity | Isolate text graphics state/ExtGState so unrelated later objects are not changed |
| Alignment | Reposition an explicitly selected line/block; do not assume PDFs contain paragraph semantics |
| CJK/complex scripts | Validate embedded/subset fonts, ToUnicode, ligatures, shaping, and bidirectional layout |
| Scanned or outlined text | Report no editable text object; do not automatically OCR |
| Image scale/rotate | Change the selected drawing usage's transform, preserving surrounding graphics state |
| Image delete | Remove the selected usage, not every occurrence of a shared XObject |
| Nested/shared forms | Isolate the relevant reference chain before modification |

Original-content editing controls remain previews and do not establish these PDF-editing capabilities. Annotation color/width/opacity are implemented independently.

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
- Keep content-edit transactions separate from annotations and preserve undo/redo semantics.
- Fingerprint the source before applying saved operations. Stop on mismatches or unsupported objects.
- Prefer Save As. Saving failures must not damage the original document.
- A full rewrite can leave historical/orphaned information. Do not advertise ordinary editing as secure redaction.

## Acceptance gate for production editing

- Old text is absent from active content extraction, and new text is searchable/copyable after reopening.
- Unrelated operations and visual regions remain unchanged within a justified tolerance.
- Editing one shared image/Form usage leaves other uses unchanged.
- CJK/subset fonts, `TJ`, transparency, rotations, clipping, overlapping text, and repeated saves have dedicated fixtures.
- Undo/redo, multiple edits, reopen, and save failures preserve integrity.
- Unsupported operations explain the reason and never silently degrade.

The editing engine, interaction mapping, and representative corpus require a separate implementation plan and explicit review of fidelity/performance trade-offs.

## Implemented sidecar workflow

Completed gestures create native overlays in PDF user-space coordinates. Ink erasure, per-page annotation removal, and bounded undo/redo modify only annotation state. A 500 ms debounce writes sidecars with a source SHA-256 and optimistic sidecar revision checks. Read/write/conflict errors remain visible and disable automatic overwrite. Save As selects a new non-clobbering destination and persists its association. Close/quit is refused while annotations are dirty or a save is pending; normal closing also waits for application-state persistence.

Current text selection is a glyph-center rectangular selection; rotated glyph runs and unavailable extraction are not guessed. Notes have an anchor and text, not full rich-text editing. Wavy lines are free paths. Restricted standard annotation export is implemented. Notes are explicitly exported as Unicode sticky notes with an icon, not as guessed-font inline FreeText. Existing annotations are preserved, not imported into editable history. Forms/widgets, signatures, encryption, unsupported UserUnit, and existing destinations are rejected. See [work package 05](../plans/05-annotation-export.md).
