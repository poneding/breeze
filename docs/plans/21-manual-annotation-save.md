# Manual Annotation Saving

## Request

On October 13, 2026 the user asked that annotations stop saving themselves: a
changed annotation set must reach the sidecar only when the user saves it.

## Change

- Remove the 500 ms debounce (`schedule_annotation_save` and
  `DocumentAnnotations::autosave`) and every call site: created, edited,
  recolored and deleted annotations, eraser strokes, undo/redo, annotation-list
  removal, and the follow-up write that followed a completed save.
- An edit marks the document unsaved. The reader status area and the annotation
  panel footer report unsaved annotations; Save (Primary+S) and Save As remain
  the only paths that write a sidecar.
- Closing the window, quitting, closing a tab, and removing a library entry stay
  refused while annotations are unsaved, so pending work is never discarded
  silently.
- A cancelled Save As or a failed save no longer starts a follow-up write.
  Reassociation and recovery copies still write immediately: both are explicit
  user actions on a file the user chose.

## Trade-off

Nothing is written until the user saves, so an unexpected termination — crash,
force-quit, power loss — can lose that session's annotations. The close/quit,
tab-close and library-removal guards cover the ordinary paths; they are the
accepted mitigation, and background writes are not used to narrow that window.

## Verification

`cargo fmt --all -- --check`, `cargo test --workspace --locked` and
`cargo clippy --workspace --all-targets --locked -- -D warnings` pass.
`annotations::tests::edits_stay_unsaved_until_the_user_saves` adds an annotation,
advances the retired 500 ms window, and asserts that the sidecar still holds the
previous snapshot, the document reports unsaved annotations, and closing stays
refused; only an explicit `save_annotations` then writes both annotations to the
file. Existing save, Save As, retry, conflict and recovery tests still pass, so
explicit saves keep their fingerprint, revision and conflict behavior. Native
visual inspection was not performed for this pass.
