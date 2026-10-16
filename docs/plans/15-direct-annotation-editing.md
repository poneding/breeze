# Direct Annotation Editing

Requested on October 10, 2026. This work applies to Breeze's sidecar annotations, not source PDF text, images, or existing embedded annotations.

## Delivery phases

1. Use Lucide `type` for text annotations, Kit NumberInput for line width, and Kit Slider for opacity. Retain independent tool settings and keyboard input behavior. Verify and commit this controls phase.
2. Create text annotations by clicking a page, entering text in a native input at that location, and pressing Enter to commit. Escape cancels. Add page-local hit testing, selection, editing, and deletion of sidecar items, including changing styles and editing text; preserve undo/redo, source fingerprints, input-method composition, and sidecar save integrity. Verify and commit the interaction phase.

Draft changes must not silently disappear on navigation or close. Transient selection must identify both document and annotation, reject stale targets after list/history changes, and never become an edit to original page content. Changes remain independent from page rasterization. Numeric controls reject non-finite and out-of-range values; slider gestures form one undoable edit rather than one history entry per pointer event.

Keep the accepted 34/38/32 px chrome and eight fixed colors. Use isolated state and sample copies under `target/` for native inspection. Record completed checks and limitations in [STATUS](STATUS.md), preserving unrelated staged and unstaged work.

## October 16 selection and resize follow-up

Requested: stronger selection feedback on white pages; editable width and height
for ellipses, rectangles, and area highlights; removal of the eraser tool.

Use a document-independent blue/white selection frame and a filled active Select
button. Add corner and edge handles in displayed page coordinates, preserving
rotation, page boundaries, minimum shape size, single-transaction undo/redo, and
stale-target guards. Remove the eraser action and its settings/catalog entries;
renumber the session-only tool palette and verify note/shape tools still dispatch
to their own annotation kinds. Verify components and native white-page rendering
before committing this follow-up. Execution results belong in STATUS.md.
