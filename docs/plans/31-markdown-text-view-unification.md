# Work package 31: single GPUI Kit Markdown TextView

Planning baseline: October 21, 2026. Status: **implemented pending native
Windows and Linux acceptance**.

## Decision

The user directed the removal of Breeze's Markdown rendering logic and a
complete switch to GPUI Kit's
[TextView](https://gpui-kit.com/component/text-view/). The reading surface is
now one selectable, scrollable `TextViewState::markdown` for the whole file.
The former mdast-to-HTML projection and per-block virtualization were deleted.

Breeze retains only the non-rendering work Kit does not expose:

- a bounded metadata scan for outline headings, source offsets, block starts
  and character counts;
- relative-image URI resolution and the existing bounded image cache;
- `MarkdownPlugin` implementations for TOC, inline/display math and Mermaid;
- application-owned search ranges, progress persistence and outline navigation.

Kit owns Markdown parsing and syntax, block virtualization, layout, selection,
copying, links, tables, task checkboxes, images and code cards.

## Removed code

- `crates/breeze-reflow/src/markdown/document.rs`, including `Document`,
  `Block`, `RenderKind`, `RenderPayload`, HTML projection and native payloads.
- `crates/breeze-reflow/src/markdown/render.rs`, including the mdast-to-HTML
  renderer, table/task/list projections, emoji mapping and sup/sub rewriting.
- Breeze's outer Markdown `ListState`, 96-entry block cache, per-table/list/task
  secondary text states and custom code-card header/controls.

`crates/breeze-reflow/src/markdown/metadata.rs` replaces the removed reading
model with metadata only and retains no render tree.

## Extension behavior

| Construct | Implementation |
|---|---|
| Front matter | Kit's `FrontmatterPlugin`, wrapped by Breeze for the reading inset |
| TOC markers | `TocPlugin`, registered with `MarkdownExtensions` |
| Inline math | `InlineMathPlugin`, rendered through `InlineElement` and RaTeX |
| Display math | `MathBlockPlugin`, rendered as a centred RaTeX image |
| Mermaid/flow/seq fences | `MermaidBlockPlugin`, rendered through `mermaid-rs-renderer` |

All other Markdown rendering is Kit's. Code blocks use Kit's native card,
tree-sitter language highlighting and `code_block_actions` for copy. Breeze
enables the common language grammars through the Kit feature flags.

The reader also adds a 24 px content inset, so full-width reading does not touch
the window chrome. Kit's native scrollbar is visually covered by a non-input
overlay at the right edge; wheel, keyboard and programmatic scrolling remain
unchanged. Images are rendered through a Breeze `MarkdownPlugin` so SVG assets
can be rasterized at 2x, retain their logical size and invalidate the TextView
layout when loading completes. Mermaid's generated white root rectangle is
removed before rasterization, avoiding a spurious grey/white page behind the
diagram.

## Follow-up fixes (October 21, 2026)

- TOC and outline navigation reveal the complete heading or top-level block
  source range. The previous single marker byte was ignored by Kit's rendered
  source mapping.
- Relative and reference-style images resolve through one shared image cache;
  nested image rows are no longer forced into a full-width inline flow.
- The scrollbar cover is `markdown-scrollbar-cover`; the content inset is
  `markdown-document` plus `markdown-text-viewport`.
- A regression test covers TOC reveal, padding and the hidden scrollbar. A
  component test asserts that Kit installed its code highlighter.

## Front matter and mode-switch fixes (October 26, 2026)

- The leading invisible content-inset block now follows YAML or TOML front
  matter. Markdown only recognizes front matter at byte zero, so the old prefix
  silently downgraded it to ordinary content and bypassed `FrontmatterPlugin`.
  Source ranges account for the marker's variable insertion point.
- Returning from an unchanged editor draft reuses the existing parsed
  `TextViewState`, decoded images and scroll model instead of rebuilding the
  entire preview. An edited draft still produces a fresh preview, but keeps the
  same document's bounded image cache.
- SVG decoding builds one `usvg` tree and loads system fonts once per image;
  badge logical-size inspection no longer duplicates that work before
  rasterization.

## Image and Mermaid edge rendering (October 27, 2026)

- Breeze does not add a decorative border to Markdown images or Mermaid blocks.
  Missing or disconnected outlines came from bitmap minification: wide Mermaid
  SVGs were fitted without scaling their strokes, so one-point rectangle edges
  fell below a device pixel in some positions and disappeared.
- Mermaid SVGs are now rewritten to the available reading width before
  rasterization. Their `width`, `height`, `stroke-width` and stroke-dash values
  change together, and the target-width bucket is part of the render cache key.
- Large raster images are area-resized to a bounded texture before upload while
  retaining their original logical size. This avoids the sparse GPU sampling
  that could skip a one-pixel source border on a multi-megapixel screenshot.

## Front matter spacing and document paths (November 2, 2026)

- YAML front matter keeps its metadata card at byte zero for Markdown parsing,
  but now owns its 48 px top inset in the custom render wrapper. The old separate
  inset block followed the card and produced a 48 px gap before the first body
  block; only the normal paragraph gap remains between them.
- TOML front matter keeps the previous post-block inset because it uses Kit's
  code-block fallback rather than the wrapped YAML metadata renderer.
- Markdown's document information panel reuses the PDF path row: start
  truncation, a full-path tooltip, and a trailing folder-open reveal action.

## Search and navigation

Search reads `TextViewState::rendered_text`, paints matches with
`set_range_highlights`, and scrolls with `reveal_range`. Outline rows carry
source offsets; `reveal_range` also restores the saved reading offset. The
reader stores reading progress from TextView's block list and the metadata
block-start map.

## Verification

- `cargo fmt --all -- --check`
- `cargo test --workspace --locked`
- `cargo clippy --workspace --all-targets --locked -- -D warnings`
- application tests cover a single `markdown-document`, removal of
  `markdown-block`, tables/tasks/code/images through TextView, plugin
  constructs, scrolling progress, outline navigation, wrapped images and the
  2 MiB reading limit.

Native visual inspection and Windows/Linux runtime acceptance remain pending.
