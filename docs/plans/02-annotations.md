# Stage 2: Real Annotations and Sidecars

## Scope

Implement document-coordinate annotations on an independent native canvas. Annotations are not PDF content edits. Start with actual drawing, markup, notes, undo/redo, debounced autosave, and explicit durable sidecar saving. Standard PDF export is gated separately.

## Changes

1. Add serializable, validated geometry and annotation types to `breeze-core`: area/text highlight, underline, strikeout, ink, note, ellipse, rectangle, line, squiggle, and arrow.
2. Normalize user-space/display transforms for CropBox origin and quarter-turn page rotation. Store user-space geometry plus the source fingerprint, not screen pixels.
3. Draw only current-page overlays. Pointer-down/move updates a draft; pointer-up creates one undoable command. Eraser removes editable ink only, never page content.
4. Semantic text markup requires valid extracted glyph bounds. Do not turn an arbitrary rectangle into fake text markup. Unsupported glyph geometry remains unavailable with a reason.
5. Store each file's annotations in `<filename>.breeze.json`. The built-in sample uses the application data directory. Validate size/count/finite-number limits before accepting sidecars.
6. Hash source bytes, check the source again before saving, and compare the sidecar's previous revision. External changes block save/load rather than being silently overwritten.
7. Use atomic replacement for an existing sidecar and non-clobbering creation for a new one. Report the exact saved destination and errors.
8. Preserve per-document undo/redo and draft isolation. Closing/quitting with unsaved changes must not silently discard them.

## Acceptance

- Geometry round trips at 0/90/180/270 degrees with nonzero crop origins.
- Ink/shape/note creation and markup selection survive zoom/page switches and a sidecar reopen.
- Undo/redo is one action per completed gesture; eraser never modifies background pixels.
- A changed PDF, invalid sidecar, conflicting revision, or failed write leaves original files intact.
- Export is not enabled until its own gate passes.

## Review boundary

UI/frame geometry and sidecar correctness can be completed independently of general PDF font fidelity. This does not certify the engine's glyph extraction for every script or PDF feature. Record restrictions rather than using visual approximations as structural proof.
