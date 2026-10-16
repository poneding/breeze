# Markdown live editing assessment

Research date: October 16, 2026. Status: **proposal; no editor implementation or
native editor acceptance has been performed**. The user requested Markdown
editing with a Typora-like experience and asked for technical research first.
The earlier read-only Markdown scope is superseded as a product requirement;
the current application still implements reading only. EPUB and PDF content
remain read-only.

The subsequent [technical design](14-markdown-editor-design.md) is the
implementation baseline. It resolves this assessment's proposed choices; retain
the assessment as the source record rather than treating every candidate as an
adopted dependency. Prototype and platform acceptance remain outstanding.

## Recommendation

Build a native GPUI live editor around **one preserved Markdown source buffer**,
with rendered blocks and inline styles projected from that buffer. Keep GPUI Kit
for the application controls and evaluate its editing primitives before adding
custom code. Use the existing pinned `pulldown-cmark` as the semantic parser.
Treat Velotype and its extracted editor component as implementation references
and prototype candidates, not accepted drop-in dependencies.

The first engineering milestone should prove continuous editing, Chinese IME,
source preservation, and selection/history correctness. A styled preview with
an input field on the active paragraph would not establish those capabilities.
The proposed work sequence is in [work package 25](../plans/25-markdown-live-editing.md).

## Experience to reproduce

Typora describes live preview as inline formatting appearing when syntax is
completed, with block formatting appearing during typing or after leaving a
paragraph. Inline delimiters appear contextually; block delimiters disappear
when rendered [S1]. This is the interaction reference, not evidence of Typora's
internal architecture.

Proposed Breeze acceptance scenarios:

| Interaction | Expected behavior |
|---|---|
| Type `# Heading` or `**bold**` | See heading/bold presentation in the editing surface; reveal syntax as needed to edit it. |
| Click formatted text | Place the caret at the corresponding text position without opening a separate editor or moving the paragraph. |
| Arrow keys and Shift-selection | Traverse paragraphs, lists, code, and tables through one logical document selection. |
| Enter and Backspace | Split/join paragraphs, continue lists, and exit an empty list item predictably. |
| Chinese composition | Keep marked text and the candidate window stable; conversion waits for composition boundaries. |
| Undo and redo | Reverse text, structural changes, formatting, and cross-block deletion in one document history. |
| Change to source view and back | Preserve source, selection direction, history, dirty state, and the viewport anchor. |
| Save | Write ordinary Markdown through an explicit, conflict-checked save workflow. |

The default authoring surface should be live editing. Source view is an explicit
editing tool for exact syntax and unsupported extensions. The existing reading
mode can remain available; neither switching modes nor merely opening a file
should change its contents.

## What exists in Breeze

The checked working tree contains `gpui-kit = 0.7.1`, resolving to `gpui-pre
0.3.8`, and `pulldown-cmark = 0.13.4`. Read the actual manifest/lockfile rather
than older specification version summaries.

- [`breeze-reflow/src/markdown.rs`](../../crates/breeze-reflow/src/markdown.rs)
  parses the entire input and retains top-level source ranges, HTML, plain text,
  headings, and a fingerprint. It does **not** retain the complete source or a
  lossless token map. Link references are resolved before blocks are separated.
- [`breeze-app/src/reflow.rs`](../../crates/breeze-app/src/reflow.rs) displays
  separate selectable `TextViewState` entities, with a 96-entry block cache.
  These states are not a shared editable document selection/history.
- [`breeze-core/src/document.rs`](../../crates/breeze-core/src/document.rs)
  distinguishes formats and reading locations; it has no Markdown editing
  capability or dirty-document lifecycle yet.
- [`breeze-storage/src/lib.rs`](../../crates/breeze-storage/src/lib.rs) provides
  useful atomic-write, identity, fingerprint, and locking primitives. Its state
  and annotation APIs are not a Markdown source-save workflow.

Existing reading limits are 16 MiB source, 64 MiB generated render text,
100,000 blocks, and depth 64. These are not verified editing capacities. A long
list or code fence may be one very large semantic block, so top-level block
virtualization alone cannot establish bounded editing work.

## Implementation routes

| Route | Evidence and fit | Assessment |
|---|---|---|
| Browser editor: Milkdown or Vditor | Milkdown is built on ProseMirror/remark; Vditor is a TypeScript browser editor with a Typora-like instant-rendering mode [S2, S3]. | Useful interaction references. Embedding them requires changing Breeze's native Rust/no-WebView constraint, so they are outside the current implementation scope. |
| Embed `gpui-markdown-editor` | Extracted from Velotype; exposes rendered/source modes, selection, history, host events, and image policy. Its manifest uses GPUI 0.2.2 [S4]. | Closest reusable candidate. Requires a GPUI port and a source-preservation assessment; not plug-compatible with the locked Kit stack. |
| Adapt Velotype itself | Native block editor with projection, document history, and source mapping. Inspected version 0.7.5 uses GPUI 0.2 [S5]. | Stronger evolving reference, but extracting a narrow component from a complete application has integration and maintenance costs. |
| Breeze source buffer plus native projection | Reuses the selected parser, Kit controls, image loading and application lifecycle; introduces the missing edit model and layout/input mapping. | Recommended architecture, with the largest new work concentrated in native editing behavior. The prototype must determine how much upstream code can be reused. |

### Findings that affect reuse

The inspected component revision is
`a422e2c2996214c7e5398f95b94318064188eaa5` (July 15, 2026); Velotype is
`a61c71955c6104fc5d647088b5957da6d59149ca` (October 13, 2026). These are the
default-branch revisions observed during research, not compatibility guarantees.
Both manifests declare Apache-2.0, and their checked license files contain that
license; retain applicable upstream notices if code is adopted [S4, S5].

The component documents canonical serialization when switching modes. Velotype's
test `imports_parenthesized_ordered_lists_and_serializes_canonical_dot_markers`
expects `1) one` to become `1. one`. Semantic round-tripping therefore does not
establish byte-preserving source editing. The component's history entries retain
complete source `String` snapshots and limit entry count; Breeze needs an
explicit byte budget [S4, S5]. These are source observations, not runtime results.

Velotype still lists more complete IME behavior as planned. Its Windows
`src/ime.rs` uses unsafe native hooks, while its PDF export launches a browser
through `chromiumoxide`. Those application subsystems cannot be copied into
Breeze under the current constraints. This does not make its native editor
rendering a WebView implementation [S5].

### GPUI Kit capabilities and gaps

Inspection of the pinned 0.7.1 package shows shared editing state, native input
handling, selection, undo, soft wrapping, source-editor display mapping, text
decorations, and atomic inline tokens [S6]. These can reduce work, but none
establishes a complete Markdown live editor:

- `TextView` handles rendered rich text; it is not the document editing model.
- `Editor` wraps and folds source lines. Its line folding does not by itself
  provide contextual zero-width inline delimiter elision.
- `TextDecoration` uses `HighlightStyle`; the resolved GPUI type supports
  emphasis/color/underline but no per-range font size. It cannot by itself
  provide heading metrics, table cells, and image layout [S7].
- Inline tokens have atomic editing semantics. Treating every Markdown marker
  as an atomic object is not equivalent to editable hidden syntax.

Prototype the public extension points first. If they cannot express the needed
layout and source/display mapping, use a focused custom GPUI editing element
behind the same document session. Reuse Kit `Editor` for source view where
practical. Do not assume that Kit's internal text state can be embedded unchanged
in a new rich layout, and do not modify downloaded registry sources.

## Proposed document and input design

### Preserve source as the authority

Keep an original snapshot, an editable rope, a monotonically increasing revision,
and source-based selection anchors. A rope is already transitive in the Kit
dependency graph (`ropey 2.0.0-beta.1` in the inspected lockfile); any direct
dependency must be deliberately pinned and verified, not assumed stable.

Editing transactions replace source ranges and record inverse edits plus
selection before/after. Formatting commands add/remove Markdown syntax in those
ranges. Render trees, headings, layout caches, and search results are derived
state; saving never serializes the whole document from rendered HTML or an AST.

Retain untouched bytes, including front matter, comments, reference definitions,
list markers, fence choices, hard-break spaces, final newline, BOM, and line
endings. Preserve existing CRLF/LF sequences; define the insertion policy from
the surrounding document without normalizing unrelated lines. Invalid UTF-8
must produce an explicit error, not lossy decoding.

Unsupported constructs remain editable as literal source at their own ranges.
They are not discarded or silently converted. This is a representation inside
one editor and one parser policy, not another rendering engine.

### Keep one logical editing session

Rendering can be virtualized into blocks, but focus, selection, composition, and
history belong to the document session. A source selection can span unmounted
blocks; copying/deleting operates on the source model rather than on visible
entities. A mounted table cell or code editor must participate in the same
transaction history instead of establishing an independent undo stack.

Map explicitly between source UTF-8 offsets, platform UTF-16 ranges, grapheme
boundaries, visible runs, and shaped positions. Carry left/right affinity when
several source positions share one visual position because delimiters are
hidden. Escapes, entities, image placeholders, and line breaks need mappings too;
parser event offsets alone are insufficient for caret hit testing.

Use GPUI's native text-input contract. While composing, retain the active input
entity and marked range; do not rebuild its block or apply syntax conversion
under the IME. Preview updates must not create undo entries. A completed
composition and any automatic structural change need intentional transaction
boundaries. Reading shortcuts must yield to the editing context.

Mode switches are projections over the same session. A secondary source-editor
widget must share/bridge the canonical history rather than mirror text through
two independently editable buffers. Changing mode during composition must wait
for a safe composition boundary.

### Parse and invalidate deliberately

Keep `pulldown-cmark 0.13.4` for CommonMark and the currently enabled extensions.
Its offset iterator supports constructing source-based semantic spans [S8].
It is not an incremental edit parser. Start the prototype with background
whole-snapshot parsing, one active parse and at most one newest queued snapshot,
and source/revision checks when applying results.

The active input must paint current text without waiting for a full parse.
Keep a provisional literal representation of the affected range while semantic
work is outstanding. Large range edits must not require synchronous full-source
copying or parsing on the UI thread. Measure snapshot materialization too.

Diff completed semantic blocks, preserve identities through edits, and invalidate
only changed layouts plus dependent references. Blank-line splitting is unsafe:
list nesting, fences, setext headings, HTML blocks, link definitions, and footnotes
can change the interpretation of later or earlier content. Local reparse windows
require explicit boundary/context tests before adoption.

| Parser option | Role in this proposal |
|---|---|
| `pulldown-cmark` | Keep as semantic authority; retain raw source separately. Offsets do not supply editing, history, or incremental reparsing. |
| `comrak` | Has an AST and CommonMark formatter [S9], but neither substitutes for a source-preserving buffer. No demonstrated reason to replace the existing parser now. |
| `tree-sitter-markdown` | Possible later syntax/invalidation accelerator. Its own README warns about parsing inaccuracies and advises against correctness-critical use [S10]. Do not make it the save/semantic authority. |

## Application integration and saving

Keep responsibilities within the existing crates initially:

| Location | Proposed responsibility |
|---|---|
| `breeze-reflow` Markdown modules | Source buffer, range transactions, parser spans, source anchors and serialization-free snapshots; no GPUI types. |
| `breeze-app` Markdown editor modules | Native projection, input/IME, hit testing, document session, tables/images, Kit chrome and source-view adapter. |
| `breeze-core` | Only the concrete format capability/mode needed for dispatch; no generic editor framework. |
| `breeze-storage` Markdown module | Background, conflict-checked source saves and bounded recovery data, separate from PDF sidecars. |

Retain the 34 px title bar, 38 px toolbar/navigation header, 32 px status bar,
shared outline, and compact reading measure. Add a Markdown edit capability and
format-specific commands; do not reuse PDF annotation mode/history. Each tab
must retain its own draft and history even while its rendered view is unmounted.
Outline and search should use the current draft revision while editing.

Proposed save policy: first offer a new `.md` destination; replacing an existing
Markdown file requires an explicit destination choice. Subsequent explicit Save
uses that session's chosen target. Automatic recovery writes only to application
data, never silently over the original. The final first-save UX remains a product
choice for the prototype review.

Capture revision and expected target identity/fingerprint with each save. Write
a temporary file beside the target, flush it, recheck conflicts, and publish
atomically using the appropriate replace/non-clobber operation. Preserve relevant
permissions and define symlink/hard-link handling before allowing replacement.
An advisory lock cannot stop an unrelated editor; a hash check plus rename is
not a universal compare-and-swap guarantee. Retain recoverable content and test
external edits/replacements instead of claiming that this race cannot occur.

A save receipt acknowledges only the revision written. If typing continues
during a save, newer changes remain dirty. Close/tab/quit guards apply to Markdown
drafts independently of annotation guards. Save As must account for relative
image/link targets without silently changing their destinations. Reuse bounded
off-thread image loading; pasted-image files need a separately defined atomic
publication and undo policy before that feature is enabled.

## Scope and acceptance gates

**First complete authoring scope:** paragraphs, headings, emphasis, inline code,
links, quotes, ordered/unordered/task lists, fenced code, native table editing,
local-image insertion/reference editing, undo/redo, source view, explicit saves,
and draft-aware outline/search. CommonMark plus the reader's existing extensions
is the initial compatibility baseline; it is not a claim of complete Typora
syntax compatibility.

**Separate follow-up scope:** rendered math, Mermaid, advanced HTML/CSS, rich HTML
clipboard conversion, remote image hosting, and export. Preserve their syntax
as source until supported. No browser/JavaScript renderer is implied. Basic
typing quality takes precedence over these extensions.

Before adopting an implementation route, require:

1. **Source fidelity:** byte-identical no-op open/mode-toggle/save; localized
   changes leave unrelated source intact; repeat undo/redo and parse agreement
   on references, lists, tables, hard breaks, front matter, and unknown syntax.
2. **Native interaction:** Chinese IME composition/cancel/reconversion, candidate
   placement, emoji/combining characters, mixed-direction text, cross-block
   selection/deletion, clicks at hidden delimiters, and keyboard-only operation.
   Native macOS, Windows, and Linux tests are separate from component tests.
3. **Lifecycle/integrity:** rapid document switches, typing during save,
   external edits, unwritable/full destinations, replacement failures, crashes,
   and stale parse/image results. An unmounted view cannot discard a draft.
4. **Bounded work:** source, pending snapshots, history, shaping/layout, decoded
   images, and GPU data have separate byte budgets. Oversized individual blocks
   need inner virtualization or a measured explicit limit, not a silent freeze.
5. **Measured performance:** preserve the existing p95 frame target of 16.7 ms;
   propose p95 input-to-visible-text at most 50 ms for the agreed ordinary corpus.
   Record parse latency separately from text feedback. Test 10 KiB, 100 KiB,
   1 MiB and 16 MiB, with CJK prose, long lines/fences, tables and deep lists.
   Use Release; record hardware/OS, fixture hashes, dimensions/device scale,
   sample count, cold/warm conditions, p50/p95, process RSS and cache scope.

No editor benchmark, cross-platform build, or native interaction test was run
for this research. Candidate upstream tests were inspected, not executed. Their
README features and tests are not Breeze acceptance evidence. Lowering quality
targets or introducing another engine requires review under the contributor
guide.

## Primary sources

Repository source was inspected as well as upstream descriptions. The Typora
support page rejected direct HTTP retrieval; its official support-repository
Markdown provided the full live-preview description. Downloaded candidate
checkouts are disposable research data under `target/`, not workspace members.

- **S1:** [Typora Quick Start](https://support.typora.io/Quick-Start/),
  [official source](https://github.com/typora/support.typora.io/blob/gh-pages/_posts/basic/2016-01-01-Quick%20Start.md).
- **S2:** [Milkdown README](https://github.com/Milkdown/milkdown#readme).
- **S3:** [Vditor README](https://github.com/Vanessa219/vditor#readme).
- **S4:** [Extracted GPUI Markdown editor at the inspected revision](https://github.com/yuWorm/gpui-markdeditor/tree/a422e2c2996214c7e5398f95b94318064188eaa5):
  `README.md`, root/crate `Cargo.toml`, `LICENSE-APACHE`, and
  `crates/gpui-markdown-editor/src/editor/{history,mod,serialization}.rs`.
- **S5:** [Velotype at the inspected revision](https://github.com/manyougz/velotype/tree/a61c71955c6104fc5d647088b5957da6d59149ca):
  `README.md`, `Cargo.toml`, `LICENSE`, `src/editor/document.rs`, `src/ime.rs`,
  and `src/export/pdf.rs`.
- **S6:** [GPUI Kit source corresponding to package 0.7.1](https://github.com/longbridge/gpui-kit/tree/87d10ae5e1299d1be18670a37570c5402f952e34):
  the published `gpui-base` package's `src/input/README.md`,
  `editor/display_map/README.md`, `editor/decorations.rs`,
  `base/inline_tokens.rs`, and `base/state.rs` under `src/input` were inspected.
- **S7:** [Resolved GPUI 0.3.8 HighlightStyle](https://docs.rs/gpui-pre/0.3.8/gpui/struct.HighlightStyle.html).
- **S8:** [pulldown-cmark 0.13.4 Parser](https://docs.rs/pulldown-cmark/0.13.4/pulldown_cmark/struct.Parser.html),
  together with the published package README and `src/parse.rs`.
- **S9:** [Comrak parser and formatter](https://github.com/kivikakk/comrak).
- **S10:** [Tree-sitter Markdown scope and accuracy warning](https://github.com/tree-sitter-grammars/tree-sitter-markdown#readme).
