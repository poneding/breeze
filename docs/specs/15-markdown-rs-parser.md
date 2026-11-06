# Markdown parser and projection rebuild

Design accepted: October 20, 2026. Rendering simplified October 21, 2026.
Status: **implemented for native macOS; Windows and Linux pending**. This
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
| `markdown/metadata.rs` | Lightweight reading metadata: headings, top-level block starts, character count, fingerprint and relative-image resolution. It retains no HTML or render tree. |
| `markdown/semantic.rs` | Editor semantics: one `LineKind` per source line (block kind, quote depth, hidden setext underline, block marker range) and styled inline `Span`s. |
| `markdown/projection.rs` | Display mapping: hidden or substituted markers, source↔display correspondence, reveal rules, live/source modes. Consumes the semantic layer; its line-local scanner remains only as the provisional projection used before a worker result arrives. |

The editor keeps its own source buffer, transactions, history, positions and
pipeline unchanged. They are parser-independent and already verified.

### Reading path

The user directed a complete renderer replacement on October 21, 2026: the
reading surface is one GPUI Kit `TextViewState::markdown` for the whole
document. Kit owns Markdown parsing, top-level block virtualization, selection,
links, tables, task checkboxes, images, code cards and scrolling. Breeze keeps
only a bounded metadata scan for outline rows, progress offsets and character
counts; it does not build an HTML projection or lay out per-block text views.

Project-specific constructs are registered through Kit's `MarkdownPlugin`
interface:

| Plugin | Node | Rendering |
|---|---|---|
| `FrontmatterPlugin` | YAML front matter | Kit's metadata description list |
| `TocPlugin` | `[TOC]` placeholders | Native heading links that reveal source ranges |
| `InlineMathPlugin` | inline math | RaTeX inline image with text fallback |
| `MathBlockPlugin` | display math and math fences | Centred RaTeX image with TeX fallback |
| `MermaidBlockPlugin` | `mermaid`, `flow`, `seq` fences | Native Mermaid renderer, rasterized through the shared image path |

The former mdast-to-HTML projection and per-block reading model were removed.
Markdown editing remains GPUI Kit's tree-sitter source editor and does not use
the reading renderer.

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
- The mdast semantic/projection modules in `breeze-reflow` are not used by the
  reading renderer. The source editor is GPUI Kit's editor; the parser and
  lightweight metadata scan remain the application-owned Markdown inputs.
- The reading parse ceiling does not apply to editing: a larger file is read and
  edited as source, while the reader still reports its limit.
- The source toolbar exposes Undo, Redo and Save. Save publishes the draft back
  to its source file after a fingerprint check, using the same background
  publication path as the storage layer; it never acknowledges text typed
  during the write. The user chose direct-to-source save on October 30, 2026;
  Save As and crash recovery remain deferred.
- Those actions report their availability: Save is disabled for a clean draft,
  while publishing and after a failed load, and Undo/Redo are disabled at the
  ends of Kit's history. Kit publishes the `Undo` and `Redo` actions but no
  depth query, and a replay emits no `InputEvent::Change`. The application
  therefore shadows up to 256 reachable text states (rope clones that share
  structure) and resolves a completed step by the text the editor shows, which
  keeps both ends exact where counting changes cannot: typing coalesces into one
  transaction, and identical text can occur at several history positions. An
  upstream `can_undo`/`can_redo` accessor would replace this shadow.

### Source status line (November 6, 2026)

The source status line reports how many lines the draft occupies instead of
naming its surface ("Markdown source"). A count is the only reading of that slot
the preview cannot duplicate: the reading surface reports the characters of the
text the parser produces, while the editor's character count measures the source
it holds, so the two figures differ by every Markdown marker the preview strips
(a three-line fixture renders 20 characters and holds 28). The line count is a
fact about the source alone. One trailing newline opens an empty line in the
rope but not a line a reader counts, and an empty draft still reports its first
line.

The trailing save state (`modified`, `saving`, `saved`) left the bar with the
label. The toolbar's Save button is the draft's state display: it is inert for a
clean draft, while a publication is in flight, and after a failed load, and a
publication failure still appears above the editing surface.

## Reading constructs (October 21, 2026)

Kit's TextView renders ordinary prose, headings, block quotes, lists, GFM
tables, task lists, fenced/indented code, links, images and raw HTML. Breeze's
plugins add the five constructs listed under the reading path above. The
reader no longer draws a separate native grid for tables or a separate card per
code block; those are the TextView's own layouts.

Image references use the same bounded loader for local files, remote `http(s)`
URLs and SVG badges. Relative paths are decoded from Markdown URI syntax before
the file is opened. A failed image is reported in the document information
panel and never becomes the document-wide status notice.

Editing remains source text and never rewrites unsupported constructs. A source
file above the reading parse ceiling remains editable while the reading surface
shows the explicit limit notice.

## Resource limits

| Limit | Value | Behavior |
|---|---|---|
| Editing/source session | 16 MiB | Unchanged; buffer and history are parser-independent. |
| One Markdown metadata/render parse | 2 MiB | Breeze refuses a larger reading input with an explicit error. Container chains above 64 leading markers on a line are refused before parsing. Editing uses Kit's editor and does not parse the whole document. |
| Reading a larger file | — | The reader reports the limit and keeps the Edit control; the editor opens directly in Source mode with a notice. |
| Inline spans / blocks / depth | 200 000 / 100 000 / 64 | Hostile-input bounds measured on the metadata mdast tree. TextView applies its own internal block and parse limits. |

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
editing, math typesetting, image insertion, Save As, crash-recovery wiring and
rich clipboard conversion. [Work package 25](../plans/25-markdown-live-editing.md)
P4–P7 keep their own gates; explicit source Save and the editor toolbar were
added on October 30, 2026.

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
