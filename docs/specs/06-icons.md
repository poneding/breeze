# Icon Source and Selection

## Current source

The prototype uses **Lucide SVG icons bundled with `gpui-kit-assets` 0.7.0**, accessed through `gpui_kit::assets::IconName` and rendered with Kit components. `AllAssets` embeds the catalog locally; icons are not emoji, PNG assets, or runtime network downloads.

The bundled package includes Lucide icons and a small set of retained Kit-specific icons. Keep the [Lucide ISC notice](../../assets/licenses/LICENSE-LUCIDE.txt) with redistributed assets. Once selection is stable, Kit's `icon_assets!` mechanism can reduce the embedded catalog without replacing the icon system. This optimization is not implemented yet.

## Choosing replacements

- [Lucide icon catalog](https://lucide.dev/icons/)
- [Lucide repository](https://github.com/lucide-icons/lucide)
- [GPUI Kit Icon component](https://gpui-kit.com/component/icon)

Specify a function and an icon name, or provide a custom SVG with appropriate licensing. The online catalog can be newer than the pinned package: check the local `IconName` before upgrading dependencies merely to obtain an icon.

## Function map

| Function | Lucide name | Rust variant |
|---|---|---|
| Home | `house` | `House` |
| Open file | `folder-open` | `FolderOpen` |
| Document tab | `file-text` | `FileText` |
| Close tab | `x` | `X` |
| Settings | `settings-2` | `Settings2` |
| System / light / dark appearance | `monitor` / `sun` / `moon` | `Monitor` / `Sun` / `Moon` |
| Recent documents | `clock-3` | `Clock3` |
| All documents / category | `files` / `folder` | `Files` / `Folder` |
| Favorite | `bookmark` | `Bookmark` |
| Toggle navigation | `panel-left` | `PanelLeft` |
| Reading pointer / search | `mouse-pointer-2` / `search` | `MousePointer2` / `Search` |
| Text highlight / area highlight | `highlighter` / `scan` | `Highlighter` / `Scan` |
| Underline / strikeout | `underline` / `strikethrough` | `Underline` / `Strikethrough` |
| Pen / eraser | `pencil` / `eraser` | `Pencil` / `Eraser` |
| Text note | `sticky-note` | `StickyNote` |
| Circle / rectangle | `circle` / `square` | `Circle` / `Square` |
| Line / wavy line / arrow | `minus` / `line-squiggle` / `arrow-up-right` | `Minus` / `LineSquiggle` / `ArrowUpRight` |
| Edit / insert text | `text-cursor-input` / `type` | `TextCursorInput` / `Type` |
| Edit / insert image | `image` / `image-plus` | `Image` / `ImagePlus` |
| Bold / italic | `bold` / `italic` | `Bold` / `Italic` |
| Left / center / right alignment | `text-align-start` / `text-align-center` / `text-align-end` | `TextAlignStart` / `TextAlignCenter` / `TextAlignEnd` |
| Previous / next page | `chevron-left` / `chevron-right` | `ChevronLeft` / `ChevronRight` |
| Submit page number | `corner-down-left` | `CornerDownLeft` |
| Fit page / fit width | `maximize` / `move-horizontal` | `Maximize` / `MoveHorizontal` |
| Actual size | `scan` | `Scan` |
| Zoom in / out | `plus` / `minus` | `Plus` / `Minus` |
| Prototype capability information | `info` | `Info` |

`Scan` and `Minus` currently serve more than one meaning in different tool areas; keep tooltips clear or choose distinct alternatives during further refinement.

Mappings live in `crates/breeze-app/src/commands.rs`, `reader.rs`, and `home.rs`. Icon availability does not imply that a corresponding document-writing feature is implemented.
