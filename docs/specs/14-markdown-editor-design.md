# Markdown editor technical design

Design baseline: October 16, 2026. The user accepted the research direction and
requested a technical design followed by an implementation plan. This document
defines the implementation contracts; it does not record implemented behavior.
The application still reads Markdown without editing it.

[Assessment 13](13-markdown-live-editing.md) retains the candidate comparison
and pinned upstream evidence. [Work package 25](../plans/25-markdown-live-editing.md)
owns sequencing and verification. Changes to the contracts below must update
both documents before dependent work proceeds.

## D1. Native stack and ownership

Use one source-preserving Markdown model and one native GPUI editing surface.
Live and source modes are two projections of the same model. The native surface
owns a document-wide input handler; independently editable paragraph widgets
and separate mode-specific undo stacks are outside this design.

Keep `gpui-kit = 0.7.1` and the checked `gpui-pre 0.3.8` resolution. Use Kit for
buttons, tooltips, menus, inputs in dialogs and shared application chrome.
Implement the document surface with GPUI shaping/layout and
`EntityInputHandler`. The published interface includes text/range replacement,
marked text and bounds/hit testing; those are the integration contract, not a
promise that native IME behavior has already been validated [R1].

Keep `pulldown-cmark = 0.13.4` for semantics. Add an explicit exact dependency on
the existing `ropey = 2.0.0-beta.1` resolution when implementing the model, with
the required byte/UTF-16/line metrics enabled and tested. Its shared snapshots
are useful for worker handoff [R2]; beta status is a dependency risk to verify,
not grounds to silently upgrade the application dependency graph.

Velotype and `gpui-markdown-editor` remain reference sources. Adopt small,
reviewed routines only when they preserve these contracts and carry their
licenses/notices. Do not depend on an entire editor fork, copy unsafe native
hooks, introduce a WebView, or add another Markdown semantic parser. A Kit
source-editor widget can replace the source projection only after proving it
can use the same transactions, source and history; it is not a prerequisite.

| Existing crate/module | Responsibility introduced by this work |
|---|---|
| `breeze-reflow::markdown` | Source buffer, edits/history, source positions, semantic parsing and projection data; no GPUI types or source writes. Split into cohesive submodules as implemented. |
| `breeze-app::markdown_editor` | Native editor entity, visual layout, input/IME, session lifecycle, formatting and table/image interactions. |
| `breeze-app::commands`, `persistence`, `reflow` | Format-aware dispatch, per-tab lifetime, reading integration and draft-derived navigation. Keep orchestration small. |
| `breeze-core::document` | Concrete Markdown edit capability and necessary view mode/location data; no generic editor/backend framework. |
| `breeze-storage::markdown` | Background Markdown file loading/saving and recovery records using storage primitives; independent of PDF sidecars. |

Names above are proposed module boundaries, not existing APIs. No new crate is
needed. `Book` owns the session handle; `ReflowView` may mount its active surface
but cannot own the only copy of an unsaved draft.

## D2. Source, transactions and history

| Value | Contract |
|---|---|
| Session identity | A transient unique identity survives tab/view changes. Async results never address a document solely by its mutable book index or path. |
| Source buffer | Valid UTF-8 with exact original bytes, including BOM, CRLF/LF, tabs, comments, unknown extensions and terminal newline. Source offsets include these bytes. |
| Revision | Monotonic generation advanced on every visible source change, including undo/redo and composition updates. Used to reject stale derived work. |
| Content checkpoint | Identifies a historical buffer state; separate from revision so undo to saved content can become clean. |
| Selection | Source byte anchor/head, direction, boundary affinity and desired visual x for vertical movement. |
| Edit transaction | Sorted non-overlapping source replacements against one expected revision, inverse edits, before/after selection and content checkpoints. |
| Saved baseline | Chosen destination, observed identity/fingerprint, saved content checkpoint and recoverable last-saved bytes. |

Apply a transaction atomically after validating UTF-8 boundaries, expected
revision and resource limits. Failure leaves source and history unchanged.
History retains inverse deltas/shared immutable data rather than copying the
whole source for every keystroke. Undo/redo restores selection and content;
monotonic revision still advances. A new edit after undo clears redo.

Coalesce adjacent typing in the same range within a 500 ms initial window.
Selection movement, paste, formatting, structural commands, save checkpoints,
mode changes and IME commit terminate a typing group. An automatic list
continuation belongs to its triggering Enter transaction. A mode change itself
does not add an undo entry or change content.

Dirty state means current content differs from the saved checkpoint, not merely
that its revision is larger. Undoing to that checkpoint clears dirty state.
When equal bytes are reached through a different history path, bounded worker
comparison may establish equality; stale equality results cannot clear dirtiness.

Saving reads the source snapshot directly. It never reconstructs the document
from HTML, canonical AST serialization or rendered text. Preserve untouched
bytes; inserted newlines follow the surrounding line ending, using the
document's dominant ending only when there is no adjacent line. Keep original
mixed endings. Invalid UTF-8 is an explicit load error.

## D3. Projection, selection and native input

A projection maps source ranges to styled text runs, hidden delimiters and
objects such as images. Each segment records source span, displayed span and
mapping kind. Escapes, character entities, line breaks and removed delimiters
need explicit mappings; parser offsets alone are not a caret map.

Live mode applies heading/inline/list presentation. Source mode uses literal
source and monospaced text. Both retain one selection, history and viewport
anchor. Contextually reveal syntax touched by the caret/selection; never destroy
markers to hide them. Resolve equal visual positions with left/right affinity.
Backspace/Delete operates on visible graphemes or an explicit structural edit,
not an accidental UTF-8 byte or invisible-marker deletion. Reveal the affected
syntax when a structural operation would be ambiguous.

Use four explicit coordinate domains: source UTF-8 bytes, native input UTF-16
units, projected text boundaries, and shaped screen positions. Native input
queries and replacements consistently use the source text domain converted to
UTF-16. Range bounds and pointer hit tests go through the projection. Never mix
UTF-16 code units, Unicode scalar counts and grapheme movement.

Selections may span unmounted blocks. Dragging, Shift-selection and keyboard
movement use document coordinates; virtualized elements merely paint visible
selection fragments. Copy/cut operates on the selected source range, so the
initial text clipboard preserves Markdown. Plain-text paste is one source
transaction. Rich HTML conversion is deferred.

Composition is an explicit session state:

1. Capture the pre-composition checkpoint and selection on the first marked
   replacement. Subsequent marked updates replace that composition range.
2. Keep the active entity, marked range and candidate bounds stable. Reveal the
   affected syntax and defer structural Markdown conversion in that range.
3. Commit the final composed result as one history entry; cancellation restores
   the original selection/content without an extra history step.
4. A mode switch, tab switch, save or close requests a native composition
   boundary first. If composition remains active, keep the request pending and
   retain the entity. Do not guess commit/cancel from an Enter key event.

GPUI's native composition callbacks determine completion. Chinese/Japanese
input, reconversion, candidate positioning, emoji and combining characters must
be tested on native platforms. Text movement, editing, selection and IME keys
take precedence over reading/page/zoom bindings.

## D4. Semantic work and layout scheduling

Use the reader's existing CommonMark extension options: tables, strikethrough,
task lists, footnotes and heading attributes. The offset iterator supplies
semantic source ranges [R3]. Keep syntax not handled by live presentation as
editable literal spans with its bytes intact.

The initial semantic implementation parses a complete immutable snapshot on a
worker. `pulldown-cmark` is not treated as an incremental edit parser. Admit one
active parse and one newest pending snapshot across the editing coordinator;
active-document input has priority, without starving a pending session. Flatten
the rope, parse, build spans and hash source off the UI thread. Reject results
whose session/revision no longer matches.

Text feedback cannot wait for parsing. Locally update the affected range's
literal projection while preserving current styling where mappings remain
valid. Existing unaffected layouts remain mounted. Semantic completion replaces
only changed projections and dependent references; reference definitions,
footnotes, fence boundaries and nested lists can invalidate distant content.
Do not split semantic work on blank lines or claim incremental parsing merely
because layout updates are incremental.

The frame cache is keyed by source/projection identity, width, text style,
device scale and theme generation. Keep source anchors through reflow rather
than storing only block indices or scroll pixels. The active composition block
cannot be evicted. Unmounted layout estimates are corrected around the anchor,
and visible-range/measurement callbacks are coalesced after layout.

Virtualize visible blocks with one viewport of lookahead. A very long paragraph,
list, fence or table additionally needs line/item/row windows; creating one
entity for a giant block is not sufficient. Per-block entity eviction never
evicts source, composition or history. Use native shaping rather than bitmap
screenshots of text.

## D5. Initial resource and performance budgets

These are initial implementation limits/targets, not measured capabilities.
Account retained bytes conservatively, including shared snapshots still pinned
by workers. Release measurements must justify any later adjustment.

| Resource | Initial policy |
|---|---|
| Source | Existing 16 MiB per document; depth 64 and 100,000 semantic blocks remain. |
| Retained source/checkpoint data | 128 MiB application-wide; include live drafts, saved baselines and worker snapshots. Refuse additional admission before losing a dirty draft. |
| Undo/redo | At most 256 transactions and 32 MiB per session; 64 MiB application-wide. Evict oldest eligible history, never source/recovery data; reject a command if its required inverse cannot fit. |
| Parsed/projection/layout data | 64 MiB application-wide retained data budget, with a measured separate parse/shaping transient peak. Evict reconstructible inactive state first. |
| Semantic jobs | One active and one newest pending; at most one snapshot per admitted slot. |
| Saves/recovery | One file publication at a time; explicit saves outrank recovery. Coalesce recovery requests and keep queued requests lightweight until admitted. |
| Local images | Reuse the reader's 16 MiB encoded-input and 32 MiB/64-entry retained-image limits; add bounded decoder admission and explicit decoded-pixel checks before publishing editor images. |
| Interactive feedback | Ordinary-corpus p95 input-to-presented-text at most 50 ms; existing p95 frame target at most 16.7 ms. Semantic-preview delay measured separately. |

Do not apply an edit and then discover it cannot be stored/undone. Check memory
admission before bulk operations; if native composition approaches the limit,
preserve its last valid state and report the limit without silently truncating
input. Eviction or history trimming must never mark a dirty buffer clean.
Existing whole-application RSS and idle targets remain in
[quality gates](05-platform-quality.md); the limits above do not establish a
process-RSS ceiling.

The ordinary reference corpus includes 10 KiB, 100 KiB and 1 MiB documents with
CJK prose, lists, links, tables and code. A 16 MiB document, giant single block
and pathological nesting are stress cases. Record their responsiveness and
resource peaks separately; the stress label does not excuse freezes or data
loss. Benchmark hashes, machine/OS, viewport/scale, samples, p50/p95 and memory
scope must accompany results.

## D6. Save, recovery and document lifetime

Opening a Markdown file starts in Reading; choosing Edit creates a session whose
default editing projection is Live. Source is an alternate editing projection.
Reading the current draft is allowed without saving it. Leaving the editing
surface, changing tabs, or going Home retains the session and dirty indicator.

The first Save offers Save As with a new filename. Choosing an existing target
requires an explicit replacement choice. Later Save uses that chosen target;
the original is never an automatic save target. There is no additional prompt
for every ordinary save. Save As adopts the new destination only after success,
and file identity/library bookkeeping updates atomically in application state.

| Event/state | Required outcome |
|---|---|
| Save requested | Resolve composition; capture session, content checkpoint, source snapshot and expected target identity/hash. A second explicit save is coalesced as a pending intent, not another full snapshot. |
| Publication succeeds | Acknowledge exactly that checkpoint and target fingerprint. Later typing remains dirty; an old result cannot acknowledge a different session or target. |
| Target changed/deleted | Retain draft, stop replacement and offer Save As or explicit reload/discard. No automatic merge or force overwrite. |
| Close/quit/remove library entry | Save, Discard, or Cancel. Wait for admitted saves; a recovery snapshot is not a successful Save. Failures leave the document open. |
| Crash recovery available | Offer restoring a draft against recorded source/target fingerprints; never overwrite a file on restore. |

The storage worker captures the actual load bytes and their fingerprint together
and detects concurrent replacement, rather than parsing one read and hashing a
different read. On save, acquire a Breeze target lock, inspect identity, stage
beside the destination, flush, recheck the expected fingerprint and atomically
publish. Preserve permissions where supported. Initially reject replacement
through symbolic links and multiply linked files; offer a new regular file.
Non-clobber publication is mandatory for a new destination.

Advisory locks and a pre-publication hash cannot exclude every external-editor
race. Retain last-saved content and the draft as recovery data, verify publication
receipts and report observed conflicts; do not advertise universal atomic
compare-and-swap guarantees. Durable directory-flush and permission behavior are platform
verification items, not assumptions supplied by `atomic_write`.

Recovery is a versioned, bounded Markdown-specific record under application
data, separate from the general settings file and PDF sidecars. Store source,
baseline bytes, selection, checkpoint and destination metadata with integrity
checks. Schedule after 2 seconds idle and at least every 10 seconds during
continuous committed typing; composition previews do not replace the last
committed recovery. Arm timers from edit events and stop them when there is no
pending work. Bound storage to 256 MiB initially; prune clean obsolete
records first and never silently delete the only unsaved recovery. If capacity
or I/O fails, retain the in-memory draft and visibly report that recovery is
unavailable. Normal source saves remain explicit.

Moving Save As to another directory must not silently retarget relative links
or images. In the first authoring scope, allow cross-directory Save As only when
there are no relative resource destinations whose meaning would change. Otherwise
identify the affected references and request a same-folder destination. Automatic
resource copying/reference rewriting is deferred to a separate transaction
design; do not rewrite source during an in-flight save. Same-directory saves
never normalize reference syntax. This initial limitation must be visible in
the destination flow and covered by tests.

## D7. Authoring scope and UI integration

The first complete release scope is native editing for paragraphs, headings,
emphasis/strike/inline code, links, quotes, lists/tasks, fenced code, GFM tables
and local image references. Include source view, undo/redo, selection, plain-text
clipboard, save/recovery and draft-based outline/search. Unknown extensions,
front matter, HTML, math and Mermaid remain editable literal content; advanced
rendering, rich clipboard conversion, uploads and export are follow-up work.

Formatting commands make minimal source replacements. Preserve list bullet and
numbering styles when continuing existing lists. Enter splits paragraphs or
continues a list; Enter in an empty list item exits one level; Backspace at a
structural boundary joins or lifts the current item as one undoable operation.
Code contents stay literal. Unsupported combinations are edited in source rather
than reformatted through a serializer.

Table cells edit their own source spans but share document selection/history.
Tab/Shift-Tab moves through cells; Enter follows a defined row-navigation action
and must not submit a Chinese composition. Escape embedded pipes and preserve
alignment markers when modifying a row. Do not reformat an entire table after
editing one cell. Cross-cell deletion and row/column commands are transactions.

Local image insertion initially references an existing selected file; it does
not copy, upload or delete files. Expose alt text/destination as editable source.
Treat images as selectable projected objects and decode off-thread with the
session/revision/resource-base identity. Clipboard image-file creation is
deferred; changing that scope requires its own filesystem transaction design.

Expose Reading/Edit plus Live/Source within the existing compact command area.
Use real Actions, focus contexts and native Tooltip shortcut display. Dispatch
Save/Undo according to document capability and focused editor; retain PDF
annotation commands/history and leave EPUB read-only. Update the existing
shortcut catalog and guard the recently added horizontal page-arrow bindings.

Reuse shared chrome constants, navigation sizing and Kit registry themes. This
package does not authorize resizing bars. The contributor guide says 34/38/32 px
while the current `style::TOOLBAR_HEIGHT` and delivery record use 34 px for the
middle bar; record/reconcile that existing baseline discrepancy before native
UI acceptance rather than introduce a Markdown-specific height.

## D8. Acceptance and remaining validation

| Gate | Evidence required before claiming it passes |
|---|---|
| G1 Source/history | No-op open/mode/save preserves bytes; localized edits, inverse operations, saved-checkpoint undo, bulk-limit refusal and unknown syntax survive exact round trips. |
| G2 Native editing | Real component tests plus native CJK composition/cancel/reconversion, candidate bounds, cross-block selection, clipboard, graphemes and mixed-direction text. |
| G3 Async/layout | Stale parse/image/save results rejected; active text appears while parsing; giant-block scrolling and viewport anchors remain correct within budgets. |
| G4 Persistence | New-file non-clobber, explicit replacement, failures/full disk, aliases, external edits, typing during save, recovery and close/quit outcomes verified. |
| G5 Authoring/UI | Every D7 operation has fixtures; source/live/reading switches preserve draft/history; PDF/EPUB commands and accepted chrome regressions pass. |
| G6 Platform/performance | Release corpus report and separate native macOS, Windows and Linux results; no equivalence between configured CI, cross-compilation and runtime acceptance. |

G1–G6 are unverified. The first prototype must establish native input/layout and
dependency feasibility before broad authoring work. Platform-specific gaps,
budget misses or necessary GPUI changes must be reported with measurements and
options; source preservation, engine constraints and quality targets cannot be
relaxed silently. Implementation status belongs in `docs/plans/STATUS.md`.

## Primary API references

- **R1:** [GPUI 0.3.8 EntityInputHandler](https://docs.rs/gpui-pre/0.3.8/gpui/trait.EntityInputHandler.html);
  the matching local registry `src/input.rs` was inspected.
- **R2:** [Ropey 2.0.0-beta.1 Rope](https://docs.rs/ropey/2.0.0-beta.1/ropey/struct.Rope.html);
  the matching source documents shared snapshots and UTF-16 metrics.
- **R3:** [pulldown-cmark 0.13.4 Parser](https://docs.rs/pulldown-cmark/0.13.4/pulldown_cmark/struct.Parser.html).
