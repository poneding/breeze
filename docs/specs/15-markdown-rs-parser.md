# Markdown parser and projection rebuild

Design accepted: October 20, 2026. Status: **implementation in progress**. This
document supersedes the parser choice in
[design 14](14-markdown-editor-design.md) D1/D4; the source, history, input,
save and authoring contracts there remain in force.

On October 20, 2026 the user asked for rendering, live editing and the source
view to be rebuilt entirely on **markdown-rs**, taking
[ELY Office's `apps/ely-mark`](https://github.com/ELY-Open-Source/ElyOffice/tree/main/apps/ely-mark)
as the implementation reference. The user then chose:

1. One AST projection for all three surfaces; the reader keeps GPUI Kit's rich
   text widget and block virtualization.
2. Editing scope stays at the current capability; the parser swap is not a
   vehicle for table, math, image or save features.
3. Whole-document parsing is limited to **2 MiB** per Markdown file. Larger
   files offer the source view only, with an explicit notice.

## Parser and dependency decision

Use `markdown = 1.0.0` (wooorm's markdown-rs) with `ParseOptions::gfm()` as the
only Markdown parser in the application. Remove `pulldown-cmark`. The version
is already resolved in `Cargo.lock` through `gpui-base 0.7.1`; pin it exactly in
`breeze-reflow` and keep the lockfile committed.

markdown-rs produces an mdast tree in which every node carries a byte-accurate
`position`, which is what the source-preserving editor needs. GFM supplies
tables, strikethrough, task lists, footnotes and autolink literals. Math and
front matter constructs stay disabled: Breeze has no typesetter or front-matter
editor, and [design 14](14-markdown-editor-design.md) D7 keeps them literal.

`apps/ely-mark` was inspected at the repository's default branch (October 20,
2026) together with the pinned `ely-gpui-component` revision
`105bf948f1571bbd0765d3bf86bcfb0bab9eccb2`. Its relevant decisions are: parse
the whole file with markdown-rs; read mdast into an application-owned display
tree; keep every shown run's source span; reuse unchanged head/tail blocks
between parses; debounce typing; and refuse to read files above a fixed size
(`LARGEST_READ = 2 MiB`, `AT_ONCE = 128 KiB`, 300 ms pause). Breeze adopts the
model and the limits, not the component code. Both upstreams are MIT; no code
is copied verbatim.

## Measured parser cost

Conditions: Apple M1 Pro, macOS 15.8.1, `cargo build --release`, one process,
warm allocator, three samples, `markdown 1.0.0` versus `pulldown-cmark 0.13.4`.
Sources: synthetic prose with CJK; "structured" adds ATX headings, lists,
tables, fences and links.

| Source | markdown-rs | pulldown-cmark |
|---|---|---|
| 1 MiB paragraphs only | 186 ms | — |
| 256 KiB realistic inline markup | 182 ms | — |
| 256 KiB light inline markup | 55 ms | — |
| 256 KiB structured | 225 ms | — |
| 1 MiB structured | ~3.0 s | ~8 ms |
| 2 000 small tables (62 KiB) | 0.56 s | — |
| 4 000 small tables (124 KiB) | 2.24 s | — |
| 14 000 headings (266 KiB) | 0.62 s | — |
| 256 KiB inline-markup dense | 0.64 s | — |

The cost is superlinear in node count, not linear in bytes: quadrupling the
block count can cost ten to sixteen times as much. This is a known upstream
issue ([markdown-rs#113](https://github.com/wooorm/markdown-rs/issues/113));
the proposed fix ([#197](https://github.com/wooorm/markdown-rs/pull/197)) is
unmerged. Breeze therefore bounds one parse at 2 MiB and keeps parsing on a
worker with revision rejection. A 2 MiB structured file may take many seconds
to lay out; the reading surface stays responsive and shows its loading state.
Do not describe the limit as a measured capacity for adversarial input.

## Architecture

```
source bytes ── parse (worker) ── mdast root (positions)
                                 ├─ reading blocks ─ mdast→HTML or block markdown ─ Kit TextView
                                 ├─ semantic lines/spans ─ display projection ─ native editor
                                 └─ headings, plain text, definitions ─ outline/search/status
```

One parse function, one option set and one mdast root per source revision.
Reading, editing and the source view never use a second Markdown parser.

### `breeze-reflow::markdown`

| Module | Responsibility |
|---|---|
| `markdown/ast.rs` | Parse admission and limits, mdast helpers, link-definition and footnote resolution, plain-text and source-segment extraction. |
| `markdown/render.rs` | mdast → the HTML subset GPUI Kit's rich text renders. It resolves document-wide references; it never rewrites the source. |
| `markdown/document.rs` | Reading model: top-level blocks with source ranges, headings with anchored block indices, plain text for search and the character count. |
| `markdown/semantic.rs` | Editor semantics: one `LineKind` per source line (block kind, quote depth, hidden setext underline, block marker range) and styled inline `Span`s. |
| `markdown/projection.rs` | Display mapping: hidden or substituted markers, source↔display correspondence, reveal rules, live/source modes. Consumes the semantic layer; its line-local scanner remains only as the provisional projection used before a worker result arrives. |

The editor keeps its own source buffer, transactions, history, positions and
pipeline unchanged. They are parser-independent and already verified.

### Reading path

GPUI Kit's Markdown view already parses with markdown-rs, but the reader cannot
hand it a whole document: it would lay out every block, and Breeze virtualizes
per top-level block. It also cannot use Kit's HTML path for fenced code, because
that renderer has no `<pre>` code card. Reading therefore keeps two projections
of the same AST:

- A block whose subtree contains a code block is handed to Kit's Markdown view
  as its own source slice, keeping list/fence context and the code card.
- Every other block is rendered to HTML from mdast, with document-wide link
  references, footnote numbering, tables, images and raw HTML honoured.

This split is a rendering-widget limitation, not a second semantic parser.

## Resource limits

| Limit | Value | Behavior |
|---|---|---|
| Editing/source session | 16 MiB | Unchanged; buffer and history are parser-independent. |
| One Markdown parse/layout | 2 MiB | `Document::parse`, `Semantic::parse` and the editor pipeline refuse larger input with an explicit error. |
| Reading a larger file | — | The reader reports the limit and keeps the Edit control; the editor opens directly in Source mode with a notice. |
| Inline spans / blocks / depth / render text | 200 000 / 100 000 / 64 / 64 MiB | Unchanged hostile-input bounds, now measured on the mdast tree. |

The 2 MiB ceiling is an accepted reduction from the previous 16 MiB *rendering*
limit, chosen by the user on October 20, 2026. The source session limit does not
change; a larger file is readable and editable as source.

## Accepted behavior changes

- `pulldown-cmark`'s `HEADING_ATTRIBUTES` (`# Title {#id}`) is not available in
  mdast. Anchors come from heading text; duplicates get `-1`, `-2`, as today.
- GFM autolink literals (`https://…` without angle brackets) now resolve to
  links while reading, because they are `Link` nodes in the shared GFM AST.
- Footnotes render as superscript labels and definition blocks, without anchor
  navigation. Math, Mermaid, front matter and unknown extensions stay literal
  source, as before.
- Task-list checkboxes remain invisible in reading mode (Kit's HTML renderer
  has no checkbox element); the item text is unchanged.

## Out of scope

Incremental or chunked parsing, a vendored markdown-rs patch, table-cell
editing, math typesetting, image insertion, save/recovery wiring and rich
clipboard conversion. [Work package 25](../plans/25-markdown-live-editing.md)
P4–P7 keep their own gates.

## Verification

The rebuild is accepted when:

1. No pulldown-cmark dependency remains in the workspace and `cargo tree` shows
   one Markdown parser.
2. `markdown_semantic`, `markdown_projection`, `markdown_editing` and the app's
   Markdown component tests pass with the documented behavior changes.
3. Reading, live editing and source view show text from the same mdast parse;
   a stale parse is still rejected by session/revision.
4. The reading surface preserves its virtualized list, outline, search,
   images and reading position; the editor preserves selection, history, IME
   composition and source bytes.
5. `fmt`, `test --workspace`, `clippy -D warnings`, a Release parse measurement
   for the 10 KiB/100 KiB/1 MiB corpus and native visual inspection are recorded
   in [delivery status](../plans/STATUS.md).
