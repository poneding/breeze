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
is already resolved in `Cargo.lock` through `gpui-base 0.8.0`; pin it exactly in
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

### Application corpus probe

`cargo test --release -p breeze-reflow --test markdown_performance -- --ignored
--nocapture`, same machine, 20 samples up to 100 KiB and 5 samples at 1 MiB. The
corpus is plain application content: headings, a paragraph with inline markup
and CJK, a nested list, a quote, a small table and a fence per section.

| Source | Lines | Semantic parse p50/p95 | Reading model p50/p95 | Projection p50/p95 | Provisional p50/p95 |
|---|---|---|---|---|---|
| 10 KiB | 475 | 2.0 / 2.2 ms | 2.0 / 2.2 ms | 0.04 ms | 0.2 ms |
| 100 KiB | 4 655 | 36.4 / 37.8 ms | 36.9 / 40.4 ms | 0.4 ms | 2.4 ms |
| 1 MiB | 47 025 | 1 985 / 2 027 ms | 2 003 / 2 014 ms | 5.6 / 6.7 ms | 29.4 / 30.2 ms |

The 2 MiB ceiling is refused before parsing. A 1 MiB file costs about two
seconds of worker time when opened; editing re-parses after the debounce and
keeps the provisional projection on screen meanwhile.

### Adversarial input

markdown-rs is also superlinear in token count. Measured on the same machine:

| Input | Bytes | Parse |
|---|---|---|
| 100 KB of unmatched `[` | 100 KB | 2.1 s |
| 400 KB of unmatched `[` | 400 KB | 49 s |
| 50 000 emphasis runs | 439 KB | 20.7 s |
| 50 000 images | 789 KB | 11.4 s |
| 20 000 links | 498 KB | 2.3 s |
| 20 000 nested block quotes | 40 KB | 3.3 s |

Leading container chains (`>`, list markers) deeper than the 64-level policy are
now refused by a cheap pre-parse scan, so the deep-nesting case cannot occupy a
worker for minutes before the tree check. Dense inline markup has no equivalent
guard: it stays a measured limitation of the pinned parser. Parsing runs on a
worker, typing stays responsive through the provisional projection, and stale
results are rejected, but no latency bound is claimed for such documents.

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

## Editing surface (October 20, 2026)

The user replaced the home-grown live-preview editing surface with a reliable
source editor: GPUI Kit's code editor with the tree-sitter Markdown grammar.
The reading view is the rendered preview. Evidence for that choice:

- The only published GPUI WYSIWYG component, `zorite-editor 0.11.2` (MIT),
  compiled against the then-current `gpui-pre 0.3.8`, but measured 0.86 s for the
  first 64 KiB frame in a debug harness and did not finish the first 1 MiB frame
  within 60 s in a Release build. `zorite-markdown` lays out a whole document
  and has the same scale problem. Velotype is a complete application, not a
  reusable crate, and would require extracting ~25 000 lines and upgrading the
  GPUI/Kit stack.
- Kit's editor owns text, selection, undo/redo, IME composition, soft wrap,
  search and syntax highlighting. It is virtualized and handled the existing
  2 MiB editing fixture in the application suite.
- Editing therefore shows Markdown source with highlighting; the reader is the
  preview. The `Live formatting` / `Markdown source` toolbar pair is removed.
- The mdast semantic/projection modules in `breeze-reflow` are no longer used by
  the application. They remain tested while a later cleanup decides whether to
  retire them; the reading model (`Document`, HTML projection) and the parser
  are still the application's rendering path.
- The reading parse ceiling does not apply to editing: a larger file is read and
  edited as source, while the reader still reports its limit.

## Reading constructs (October 20, 2026)

The reading surface now draws the constructs a hand-written Markdown note uses,
not only CommonMark paragraphs. The user reported thirteen gaps against a
reference note; the implementation classifies blocks and chooses the widest
reader that can draw each one:

| Construct | Reading behavior |
|---|---|
| Front matter | A `---` YAML block becomes a native metadata card; its fences do not render. `title: …` and other lines are shown as key/value rows. |
| Table of contents | `[TOC]`, `[[toc]]`, `[[_TOC_]]`, `{{toc}}` and `<toc>` become a generated heading list with anchor links. The list is nested by heading level and filled after every heading is known, so it may precede them. |
| Rules | `---` thematic breaks and a paragraph line of only `===`/`---`/`***` (three or more, spaces allowed) become a native 1.5 px divider drawn in the muted foreground. CommonMark alone treats `===` as a paragraph. |
| Superscript/subscript | `<sup>…</sup>` and `<sub>…</sub>` runs map to Unicode superscript/subscript characters where they exist (`O<sup>2</sup>` → `O²`, `X<sub>2</sub>` → `X₂`). Characters without a Unicode counterpart keep their baseline. |
| Abbreviations | A paragraph made only of `*[ABBR]: expansion` lines is a declaration and is not rendered. Usages remain plain text; Kit's HTML reader has no `<abbr>` styling or tooltip. |
| Tables | Table blocks are drawn as a native grid: one inline text view per cell, GFM column alignment applied to the cell text, and borders in the reader's palette. Selection rectangles therefore follow the aligned text instead of the column start, and every cell renders (including inline code such as `help()`). |
| Task lists | Task rows are drawn natively with a 16 px rounded checkbox (4 px radius) and the item's inline content; nested items indent by level. |
| Emoji | A bounded GitHub shortcode table replaces known `:name:` runs in ordinary text; unknown names stay literal. Code spans are untouched. Source line breaks become reading line breaks. |
| Formulas | A `$$…$$` block is typeset with RaTeX (embedded KaTeX fonts) into an image. Inline `$…$` shows its TeX in a code run; inline typesetting is still open. |
| Mermaid | A ```mermaid fence is rendered natively by `mermaid-rs-renderer` to SVG, then rasterized with the system fonts. Unsupported diagram syntax falls back to the source code card. |
| Code blocks | A native card shows the fence language (muted) on the left, a copy button and an ellipsis menu on the right. The menu toggles line numbers and soft wrapping; the body uses the monospace face at the reader's code size. |
| Images | An image-only paragraph becomes a centred, borderless row; multiple badges stay on one line, scaling down together to the reading measure. Local files, remote `http(s)` images and SVG badges are supported. Remote fetches are bounded (10 s, 16 MiB, five redirects); SVGs use system fonts for badge text. |

These are reading-only projections: editing remains source text, and an
unsupported construct never disappears from the draft.

Open limitations: abbreviation tooltips, inline formula typesetting, a dark
Mermaid palette, diagrams beyond the native renderer's syntax set, and no
syntax highlighting in the native code card. Source lines that a parser reads as
a setext heading (a title line immediately followed by `---`/`===`) remain
headings, not rules; a separator needs its own paragraph.

## Resource limits

| Limit | Value | Behavior |
|---|---|---|
| Editing/source session | 16 MiB | Unchanged; buffer and history are parser-independent. |
| One Markdown parse/layout | 2 MiB | The reading model refuses larger input with an explicit error. Container chains above 64 leading markers on a line are refused before parsing. Editing uses Kit's editor and does not parse the whole document. |
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
