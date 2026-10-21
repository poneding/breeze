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
| Front matter | Kit's `FrontmatterPlugin` |
| TOC markers | `TocPlugin`, registered with `MarkdownExtensions` |
| Inline math | `InlineMathPlugin`, rendered through `InlineElement` and RaTeX |
| Display math | `MathBlockPlugin`, rendered as a centred RaTeX image |
| Mermaid/flow/seq fences | `MermaidBlockPlugin`, rendered through `mermaid-rs-renderer` |

All other Markdown rendering is Kit's. Code blocks use Kit's native card and
`code_block_actions` for copy.

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
