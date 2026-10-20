# Work package 27: Markdown parser rebuild on markdown-rs

Planning baseline: October 20, 2026. The user asked for Markdown rendering, live
editing and the source view to be rebuilt entirely on markdown-rs and pointed at
ELY Office's `apps/ely-mark` as the reference. The user then chose a unified AST
projection, editing parity rather than new authoring features, and a 2 MiB
whole-file parse ceiling. [Specification 15](../specs/15-markdown-rs-parser.md)
owns the design; [design 14](../specs/14-markdown-editor-design.md) keeps the
source, history, input and save contracts.

This package replaces the parser and the two semantics consumers. It does not
implement save/recovery, table or image editing, math typesetting or new
commands; those stay with [work package 25](25-markdown-live-editing.md).

## Delivery order

| Phase | Deliverable | Exit evidence | Status |
|---|---|---|---|
| M1 Reading model | mdast parse with byte-accurate positions, top-level blocks, headings, plain text, document-wide definitions and the mdast→HTML projection; `pulldown-cmark` removed | `breeze-reflow` Markdown tests, link/footnote/table/code fixtures, dependency check | Complete (October 20) |
| M2 Editor semantics | `LineKind` per source line with block markers, quote depth and hidden setext underlines; styled inline spans with exact source ranges; projection consumes them for live and source modes | `markdown_semantic`, `markdown_projection`, `markdown_editing` tests | Complete (October 20) |
| M3 Application wiring | Reading view on the new projection, source-only behavior above 2 MiB with explicit notices, editor pipeline admission, toolbar state | Component tests plus native visual inspection | Complete except native visual inspection (October 20) |
| M4 Verification | Workspace checks, Release measurements for the corpus, STATUS and specification records | `fmt`, `test --workspace`, `clippy -D warnings`, measurement table, visual review note | In progress (October 20) |

Execute in order and commit each completed phase. M1 and M2 share the commit
`refactor(markdown): rebuild the parser and projections on markdown-rs`: the
parser dependency cannot be removed while one of its two consumers still uses
it. M3 and M4 keep their own commits.

## M1. Reading model

- Parse once with `markdown::to_mdast` and `ParseOptions::gfm()`; bound the
  parse at 2 MiB and keep the existing 100 000-block and 64-depth checks on the
  produced tree.
- Keep `Document`, `Block`, `Heading` and the existing public reading API. A
  block stores its source range, HTML projection, plain text and — when it
  contains a code block — its own source slice for Kit's Markdown view.
- Resolve link and image references against every `Definition` in the file
  before rendering; number footnotes in reference order. Unresolved references
  stay literal source.
- Render ordinary blocks to the HTML subset GPUI Kit parses. Do not claim a code
  card from the HTML path: blocks containing code keep the Markdown hand-off.
- Preserve headings, anchors, search, character counts and reading positions.

Fixtures: setext and ATX headings, duplicate titles, tables, task lists, nested
lists and quotes, fenced and indented code, images and encoded paths, reference
links across blocks, footnotes, raw HTML, CRLF/BOM bytes, empty documents, and
the 2 MiB refusal.

## M2. Editor semantics

- Build one `LineKind` per source line from block-node positions: block kind,
  quote depth, the exact marker range to hide or substitute (ATX hashes, list
  bullet), the hidden setext underline, and fence boundaries.
- Build inline `Span`s for strong, emphasis, strikethrough, links and inline
  code from node positions, nesting depth and exact content ranges. Split text
  nodes into copied runs and decoded runs for escapes and character references
  so caret mapping stays byte-accurate. Unresolved references, images, math,
  HTML and footnote references stay literal, as today.
- Keep the line-local projection scanner only for the provisional projection
  shown before a worker result arrives. With a semantic layer present, markers
  and kinds come from the AST.
- Preserve live/source switching, reveal rules, mapping round trips, composition
  literals, selection and history behavior.

Fixtures: the existing semantic/projection/editing suites plus nested emphasis,
multi-entity text nodes, escaped delimiters, task items, tabs, nested quotes and
lists inside quotes, setext underlines, fences nested in lists, and stale-layer
refusal.

## M3. Application wiring

- The reading surface uses the new blocks unchanged; remove the obsolete
  `markdown`-slice workaround only if Kit's HTML path gains a code card, which
  it has not.
- Above 2 MiB, `Document::open` reports the limit in the reading surface, and
  the editor opens in Source mode with a translated notice while keeping the
  full 16 MiB source session. The Live control is visibly unavailable.
- Keep the provisional projection for typing latency and the one-running /
  one-pending worker admission with revision rejection.

Component evidence: a normal document reads, edits and switches source; an
oversized document reports the limit, refuses Live, and still edits and copies
its source bytes.

## M4. Verification

- `cargo fmt --all -- --check`, `cargo test --workspace --locked`,
  `cargo clippy --workspace --all-targets --locked -- -D warnings`.
- Release measurement of parse and projection for 10 KiB, 100 KiB and 1 MiB
  corpora with hardware, OS, viewport and warm/cold conditions recorded.
- Native visual inspection of reading, live editing and source view on macOS;
  Windows/Linux stay explicitly unverified until run there.
- Update [delivery status](STATUS.md), and correct spec 15 if measurement
  contradicts it.
