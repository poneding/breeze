# Work package 25: Markdown live editing

The user requested a Typora-like Markdown editor on October 16, 2026, with
technical research first. [Assessment 13](../specs/13-markdown-live-editing.md)
records the findings, source references and recommended design. Only the
assessment is complete; all implementation phases below are proposed and
unstarted. Existing Markdown functionality remains read-only.

## 1. Prove the editing architecture

Use an isolated native prototype on the locked GPUI Kit stack. Inspect the
public input/layout extension points and attempt a narrow port of useful
`gpui-markdown-editor` behavior before committing to a component fork or custom
editing element. Do not change the application's GPUI dependency graph just to
make an older editor compile.

The prototype needs only paragraphs, a heading, emphasis, a list and a code
block, but must demonstrate the hard interactions across them:

- One source buffer, cross-block selection, stable caret/scroll mapping, and
  a unified undo history through source/live mode switches.
- Chinese composition and candidate placement, emoji/grapheme movement,
  unfinished syntax, and predictable paragraph/list split/join operations.
- Byte-preserving no-op cycles and local edits that leave unrelated source
  unchanged, including line endings, reference definitions, and front matter.
- Background semantic parsing and current-text feedback during outstanding
  parsing; bounded work for one large block as well as many small blocks.

Record native observations and Release measurements. Compare reuse cost against
the focused custom element, identify any necessary upstream changes, and report
the tradeoffs before expanding syntax. Upstream unsafe Windows hooks cannot be
copied into first-party code. Retain licenses/notices for any adopted code.

## 2. Implement source transactions and safe document lifecycle

Add the Markdown source/edit model under `breeze-reflow`, with source anchors,
inverse edits, revision-scoped parser results and an explicit history byte
budget. Add the concrete editing capability to format dispatch. Keep per-tab
drafts independent of mounted reader views.

Implement background Markdown saves under `breeze-storage`, with target identity
and fingerprint checks, atomic publication, explicit Save/Save As, revision-aware
receipts and close/quit guards. Finalize first-save destination behavior at the
prototype review; automatic recovery must not overwrite originals. Validate
external edits, write failures, concurrent typing, file aliases and relative
resource behavior after Save As. Save acknowledgements are independent of PDF
annotation persistence.

Commit this phase only after meaningful source-preservation, edit/history and
I/O integrity tests pass. Do not expose an editing mode that can silently discard
its draft on tab changes or application exit.

## 3. Integrate native live editing

Connect the approved projection/input element to the existing compact chrome,
outline, search and image loader. Preserve 34/38/32 px bar heights and isolate
reading/page shortcuts from editing keys. Keep theme selection in the Kit
registry; do not import the candidate application's themes or shell.

Deliver paragraphs, headings, inline emphasis/code/links, quotes, lists/tasks,
fenced code, source view and a shared document history first. Then add native
table editing and local-image insertion/reference controls with their own
selection, transaction and resource-lifecycle checks. Keep unsupported syntax
editable as source; do not claim math/Mermaid/HTML presentation without support.

For every completed UI slice, run component tests and isolated native visual
inspection before its local commit. Keep generated fixtures, application data,
captures and measurements under `target/`.

## 4. Validate the complete authoring scope

Run repository rustfmt, locked workspace tests and Clippy with warnings denied.
Launch with isolated `BREEZE_DATA_DIR` and verify real mouse/keyboard/IME flows.
Execute the source, interaction, lifecycle and performance matrix from the
assessment, including 16 MiB and oversized-block stress fixtures.

Record native macOS, Windows, and Linux results independently; component tests
and cross-compilation do not establish IME/GPU/runtime acceptance. Record
performance failures with measurements and options before adjusting limits or
architecture. Update `STATUS.md` with implemented behavior and outstanding
gates, then commit each completed and verified phase locally. No push or
publication is authorized by this plan.
