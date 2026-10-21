# Work package 29: Markdown reading constructs

Planning baseline: October 20, 2026. The user tested a hand-written Markdown
file and reported thirteen reading gaps: images and badges, front matter, a
table of contents, `===`/`---` rules, superscript/subscript, abbreviations,
table cells, table alignment, emoji, task checkboxes, formulas and Mermaid.
The reader now classifies blocks and routes each construct to the widest
renderer that can draw it. [Specification 15](../specs/15-markdown-rs-parser.md)
owns the behavior table and limits.

## Phases

| Phase | Deliverable | Exit evidence | Status |
|---|---|---|---|
| R1 Blocks | Front matter cards, generated TOC, native rules, table/task routing to Kit's Markdown reader, formula and Mermaid image blocks | `breeze-reflow` construct tests; the extended-construct component test | Complete (October 20) |
| R2 Inline and images | Sup/sub Unicode mapping, emoji shortcodes, abbreviation declarations, remote/SVG image loading with bounds | component text assertions; SVG and loopback-fetch unit tests | Complete (October 20) |
| R3 Verification | Workspace checks, Release build, native smoke run; record the remaining limitations | `fmt`, `test --workspace`, `clippy -D warnings`, STATUS record | Complete except native visual inspection (October 20) |

## Decisions

- Front matter, TOC, rules, formulas and Mermaid are native blocks; they do not
  go through the HTML reader, which cannot express `<hr>`, alignment, checkboxes
  or images-in-tables.
- Tables and task lists use Kit's Markdown reader for the same reason. A block's
  own source is the projection; document-wide link definitions inside such a
  block resolve only within that block.
- `===` is treated as a rule at the top level when it is the whole paragraph;
  setext headings still win when a title precedes it.
- Sup/sub have no baseline shift in Kit's rich text, so they map to Unicode and
  fall back to the original characters when no mapping exists.
- Display formulas use RaTeX (`ratex-gpui`, render-only) and Mermaid uses
  `mermaid-rs-renderer`; both are MIT and native, with no WebView or external
  process. Their licenses and bundled KaTeX font notices ship in
  `assets/licenses/`.
- Remote images are fetched on demand with a 10 s timeout, 16 MiB body limit and
  five redirects; SVG rendering loads system fonts for badge labels.

## Open items

Abbreviation tooltips and inline formula typesetting need an inline renderer
that Kit's HTML subset does not provide. A dark Mermaid palette and diagram
syntax beyond `mermaid-rs-renderer`'s supported set are follow-ups. Native
visual inspection was not performed because the session's model cannot view
images; Windows and Linux remain unverified.
