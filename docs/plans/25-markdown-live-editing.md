# Work package 25: Markdown live editing

Planning baseline: October 16, 2026. The user requested a Typora-like Markdown
editor, then technical research, a technical design and an implementation plan
in that order. [Assessment 13](../specs/13-markdown-live-editing.md) is the source
record; [design 14](../specs/14-markdown-editor-design.md) defines contracts D1–D8
and acceptance gates G1–G6. This plan executes that design rather than reopening
the candidate selection at every phase.

Research, design and P1 are complete. **P1 source-model tests pass; no editor
surface, prototype validation or editor benchmark has been performed.** P2–P7
are unstarted and Markdown remains read-only in the application.

## Delivery order and readiness

| Phase | Depends on | Concrete deliverable | Exit evidence | Status |
|---|---|---|---|---|
| P1 Source model | Design baseline | Source-preserving buffer, transactions, history and source positions | G1 model tests and dependency checks | Complete (October 16) |
| P2 Native feasibility | P1 | One native surface with live/source projection and Chinese composition | G2 prototype evidence; explicit feasibility decision | Not started |
| P3 Semantic/layout pipeline | P2 passes | Bounded parsing, projection invalidation, viewport and giant-block virtualization | G3 tests and first Release report | Not started |
| P4 Save and session lifetime | P1; P2 for composition behavior | Conflict-checked saves/recovery and per-tab lifecycle guards | G4 storage and component tests | Not started |
| P5 Core authoring integration | P2, P3, P4 | Usable core live editing in the application | G1–G5 core authoring evidence | Not started |
| P6 Tables and local images | P5 | Native structured editing and existing-file image references | G5 table/image matrix and regression checks | Not started |
| P7 Acceptance | P6 | Full verification record, remaining limitations and platform status | G1–G6 evidence; production gates still open if unverified | Not started |

Execute in listed order and commit each completed, verified phase before the
next. P4's limited dependency does not request parallel agents. Split P6 into
table and image commits, each verified separately. Fixes discovered during P7
are separate logical commits, followed by the acceptance record.

Keep editing unavailable in the normal application until P4's draft retention,
save and close behavior passes. Earlier native work uses a development example
or component harness with authored samples under `target/`. Do not make a
partially implemented save workflow reachable from the user's library.

## P1. Source model and deterministic edits

Implement under `crates/breeze-reflow/src/markdown/`, splitting the existing
`markdown.rs` only as needed. Suggested responsibilities are `buffer`, `edit`,
`history` and `positions`; these are planned files, not current APIs. Keep the
read-only `Document::open/parse` behavior working while introducing the editor
model. Add and lock the explicit Ropey dependency/metrics from D1 without
upgrading GPUI or unrelated dependencies.

Deliver the D2 session identity, revision/content checkpoint distinction,
source-range selection, atomic replacement transactions, inverse edits and
history budgets. Implement UTF-8/UTF-16 boundary conversion and grapheme-aware
movement primitives without GPUI or source-file I/O in the model.

Exit tests must exercise:

- Exact source preservation for UTF-8 BOM, CRLF/LF/mixed endings, hard-break
  spaces, final newline/no newline, tabs, comments, front matter and unknown
  syntax. Invalid encoding is an explicit error.
- Replacement, multi-range transactions, invalid boundaries, revision mismatch,
  history coalescing boundaries, redo invalidation and selection restoration.
- Edit → save checkpoint → edit → undo becomes clean at the saved checkpoint;
  monotonic revision still increases. A stale equality result cannot clear dirty.
- Byte-budget eviction and refusal before mutation, including a large paste
  whose inverse would exceed the budget. Check source and history are unchanged
  on refusal.
- UTF-16 surrogate pairs, combining sequences, emoji and deterministic generated
  edit/undo sequences that restore the initial bytes.

Run focused reflow tests and the workspace checks below; inspect dependency and
license changes. Suggested subject:
`feat(markdown): add source-preserving edit transactions`.

## P2. Native editing feasibility gate

Build a small `breeze-app` native development harness around the P1 model, then
keep its useful code in the planned `markdown_editor` module. Use GPUI's public
input interface and shaping APIs on the locked stack. Upstream routines may be
adapted only with source attribution and license retention; an entire editor
fork or application shell is not part of this task.

Support a paragraph, heading, emphasis, list and code block. Live/source views
must share one session and one history. The prototype must prove:

1. Caret hit testing around hidden markers, wrapping and mixed font sizes.
2. Arrow/Shift movement and drag selection across blocks, followed by deletion
   and undo that restores both source and selection direction.
3. One native composition range through repeated Chinese marked-text updates,
   cancellation, commit and reconversion, with candidate bounds following the
   caret after scrolling.
4. Mode/tab/save/close requests during composition do not discard or commit
   partial input accidentally. Test native callbacks, not only key events.
5. Source/live toggles leave bytes and history unchanged, and preserve the
   viewport anchor through height changes.
6. Inactive/hidden markers and unmounted blocks are included correctly in source
   selection and clipboard operations; ordinary page shortcuts do not interfere.

At this stage, save/close requests are harness intents used to verify composition
boundaries; source-file publication is implemented and tested in P4.

Add component tests and inspect a native macOS run with isolated state. Attempt
the focused native input matrix on Windows and Linux when those platforms are
available; otherwise record them as unverified. Capture measurements and visual
evidence under `target/markdown-editor-review/p2/`.

**Decision:** proceed when the single native surface demonstrates the above on
the available native platform with no known architectural blocker. Missing
platform access remains a G6 release gate. If GPUI input/layout cannot express a
required operation, report the reproduction, upstream API gap and a bounded
change proposal before expanding authoring. Do not mask an IME failure with an
independent text field or first-party unsafe platform code.

Suggested subject: `feat(markdown): add a native live editing surface`.

## P3. Bounded semantic and layout pipeline

Implement D4's immutable snapshot handoff, one-active/one-pending parse admission,
revision rejection and minimal projection replacement. Keep the existing
`pulldown-cmark` options and global reference semantics. Flatten snapshots and
build spans off-thread; current input paints without waiting for that work.

Implement stable source anchors, height estimates, one-viewport lookahead,
coalesced measurement callbacks and layout-cache eviction. Giant paragraphs,
lists, code fences and tables must window their inner content. Integrate D5
source/snapshot/history/layout accounting; unfinished work cannot pin an
unbounded series of old document snapshots.

Use literal source projection for structures whose rich editing arrives in P6,
then repeat the same giant-block checks against their rich projection in P6.

Exit coverage includes out-of-order results, rapid edits/document switches,
reference-definition changes, fence insertion/removal, setext headings, nested
lists, footnotes, unmatched delimiters, viewport resize, zoom/theme changes and
scrolling while typing. Assert measured offscreen items never become navigation
events and composing content stays mounted.

Run the first Release probe on the corpus below, including a 16 MiB and a giant
single-block case. Report parse latency separately from input-to-paint and
retained cache bytes separately from process RSS. If budgets fail, profile and
report options before accepting a weaker limit or different architecture.

Suggested subject: `feat(markdown): bound parsing and virtualize editor layout`.

## P4. Storage, recovery and per-tab lifetime

Add `breeze-storage::markdown` for source load snapshots, explicit save targets,
publication receipts and bounded recovery records. Reuse appropriate atomic
write/fingerprint primitives; do not reuse PDF sidecar schemas or acknowledge
PDF annotation saves. The parsed input and baseline fingerprint must describe
the same read bytes.

Introduce a per-`Book` session handle and connect save/close handling in
`breeze-app` while retaining the normal editing-entry gate. Implement D6's first
Save As, explicit replacement choice, one publication job, queued-save intent,
typing-during-save behavior, tab/Home retention, and Save/Discard/Cancel guards.
Adopt a new target/library identity only after its successful save receipt.

Add recovery's idle/deadline scheduling, version/length/integrity checks,
capacity handling and restore-as-draft flow. Recovery records and test files
remain under isolated application data. Recovery success must never clear the
document's unsaved-source indicator.

Use temporary files and controlled failure injection to verify:

- New-destination non-clobber; target deletion/replacement; source changes
  between load/read/check/publication; concurrent Breeze target locking.
- Read-only/full destinations, staging/flush/publication failures and corrupted
  recovery records leave the draft and original recoverable.
- Symlink/hard-link replacement refusal and ordinary-file permission behavior.
  Record platform-specific durability/permission limits explicitly.
- Saving revision A while typing B acknowledges only A; switching tabs or
  choosing another target cannot apply A's receipt to the wrong session.
- Closing or quitting with dirty/composing/saving documents produces the
  specified outcome; Cancel retains all source and selection.
- Cross-directory Save As with affected relative resources refuses before
  publication and identifies the references; same-directory Save As preserves
  their source spelling. Automatic copying/rewriting remains deferred.

Run storage tests, model regressions and native component tests for the guards.
Suggested subject: `feat(markdown): save drafts with conflict and recovery checks`.

## P5. Core authoring and application integration

Enable the concrete Markdown editing capability after P2–P4 pass. Add the
Reading/Edit and Live/Source controls using the shared compact chrome. Resolve
the recorded shared toolbar-height discrepancy against applicable user/design
authority; this phase must not invent a Markdown-specific height or change
unrelated reader density.

Implement minimal source edits for headings, emphasis/strike, inline code,
links, quotes, lists/tasks and fenced code. Specify keyboard results in fixtures:
paragraph split/join; list continuation/empty-item exit/lift; code newline and
indentation; hidden-marker deletion; formatting inside and across selections.
Each operation is one appropriate history transaction, including its automatic
structural change.

Make outline, find results, word/position display and reading mode consume the
current draft revision. Preserve independent tab positions and histories. Wire
Save/Undo/Redo by format and focus, including the existing horizontal page keys,
and update the shortcut catalog/Chinese UI strings. Do not reuse annotation mode
or PDF history for Markdown.

Exit evidence: a user can open an authored sample, enter Live mode, write with
Chinese IME, format it, select across blocks, undo/redo, switch modes/tabs, search
the draft, save, reopen exact bytes, and close safely. Repeat PDF annotation and
EPUB reading regressions. A native visual review must include narrow/wide windows,
light/dark themes, toolbar/status sizing and keyboard-only access.

Suggested subject: `feat(markdown): integrate core live authoring commands`.

## P6. Tables and existing-file images

**P6a Tables:** add source-mapped GFM cells and row/column transactions sharing
document history. Define Tab/Shift-Tab boundary behavior and Enter row navigation
before implementation. Test escaped pipes, alignment, inline formatting,
multiline/unsupported syntax, cross-cell selections, structural undo and Chinese
composition. A cell edit must preserve unrelated rows and their spacing. Review
large-table virtualization natively. Suggested subject:
`feat(markdown): add native table editing`.

**P6b Images:** insert references to selected existing local files and expose alt
text/destination editing. Reuse the bounded image loader with explicit job and
decoded-pixel admission. Key completion by session/revision/resource base; old
images cannot land after a destination change. Test percent-encoded names,
spaces/CJK paths, missing files, oversized images, undo/removal and Save As
restrictions. Selecting/deleting the image affects Markdown only, not the file.
Suggested subject: `feat(markdown): edit local image references`.

Clipboard image-file creation, resource copying, remote hosting, rich HTML paste,
math/Mermaid rendering and export remain out of this package. Their source must
remain readable/editable and survive saves. Commit each P6 slice only after its
own model/component/native checks pass.

## P7. Complete verification and acceptance record

Run the commands below from the repository root on the exact candidate tree:

```sh
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
BREEZE_DATA_DIR="$PWD/target/markdown-editor-review/app-data" cargo run --locked -p breeze-app
```

During development, use `cargo test -p breeze-reflow --locked`,
`cargo test -p breeze-storage --locked` and the relevant real app component tests
for focused checks. The full checks are still required at phase completion when
code is changed. Do not claim the commands above were run for this planning
document; documentation validation consists of link/contract coverage and diff
checks.

For each code phase, inspect the staged diff and record the candidate commit,
commands, counts/results and native limitations in `STATUS.md`. If unrelated
work is in progress, preserve it and use an isolated candidate when needed;
do not attribute failures or successes of a different tree to the phase.

### Fixture and benchmark matrix

| Corpus | Coverage |
|---|---|
| Byte preservation | BOM, mixed endings, final newline, comments/front matter, escapes/entities, hard breaks, link definitions, footnotes, unsupported syntax |
| Ordinary documents | Authored 10 KiB, 100 KiB and 1 MiB samples containing CJK prose, lists, links, tables and code |
| Stress documents | 16 MiB source, giant paragraph/line/fence/list/table, maximum nesting/blocks and edits rejected at limits |
| Lifecycle | Multiple dirty tabs, composition at mode/close boundaries, reordered jobs, external writes, Save As, recovery failures and restarts |
| Native platforms | macOS; Windows with native IME; Linux with representative IBus/Fcitx input and available Wayland/X11 coverage |

Keep redistributable fixture generators/tests in source and generated documents,
screenshots, logs and measurements under `target/markdown-editor-review/`.
Record hashes and exact feature coverage. Benchmark in Release with at least
100 input/scroll samples per ordinary fixture, 20 cold-process opens and 20
warm opens; state that process-cold does not imply cleared filesystem caches.
Record stress sample counts separately. Report hardware/OS, viewport and device
scale, p50/p95, input-to-paint, semantic latency, source load/first frame, RSS,
transient peaks and separately counted cache/history bytes.

Check p95 input-to-visible-text against 50 ms and p95 frame time against 16.7 ms,
along with D5's resource limits and the existing whole-application quality gates.
An unmeasured platform or performance condition stays unverified. Release
acceptance requires native results for all supported platforms; local feature
completion must not be labeled three-platform production acceptance.

### Requirement traceability

| Design contract | Implementation owner | Principal gates |
|---|---|---|
| D1 Stack/ownership | P1, P2, P5 | G1, G2, G5 |
| D2 Transactions/history | P1; P4 checkpoints | G1, G4 |
| D3 Projection/input | P2; P5/P6 commands | G2, G5 |
| D4 Parsing/layout | P3 | G3 |
| D5 Limits/performance | P1, P3, P4, P6; P7 measurement | G3, G6 |
| D6 Save/recovery/lifetime | P4, P5 | G4 |
| D7 Authoring/UI | P5, P6 | G5 |
| D8 Acceptance | Every phase; P7 consolidated record | G1–G6 |

G1–G6 are all **not run** as of this plan. Final delivery must name supported
editing operations, tested source fidelity, exact native checks, measured
performance, known limitations and any unverified gates. Remote push or
publication remains separately authorized; no sub-agents are requested by this
plan.
