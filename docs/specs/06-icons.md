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
| Open file | `file-plus-corner` | `FilePlusCorner` |
| Add document after titlebar tabs | `plus` | `Plus` |
| Document tab | `file-text` | `FileText` |
| Close tab | `x` | `X` |
| Settings | `settings` | `Settings` |
| System / light / dark appearance | `monitor` / `sun` / `moon` | `Monitor` / `Sun` / `Moon` |
| Recent documents | `clock-3` | `Clock3` |
| All documents | `library-big` | `LibraryBig` |
| Category group / category | `tags` / `tag` | `Tags` / `Tag` |
| Favorite | `bookmark` / filled derivative | `Bookmark` / local SVG |
| Reveal document in the file manager | `folder-open` | `FolderOpen` |
| Toggle navigation | `panel-left` | `PanelLeft` |
| Outline / Thumbnails navigation tabs | `table-of-contents` / `images` | `TableOfContents` / `Images` |
| Toggle document sidebar | `panel-right` | `PanelRight` |
| Document information / annotations | `info` / `highlighter` | `Info` / `Highlighter` |
| Annotation type filter | `list-filter` | `ListFilter` |
| Outline title search / more | `search` / `ellipsis-vertical` | `Search` / `EllipsisVertical` |
| Expand / collapse all outline branches | `list-chevrons-up-down` / `list-chevrons-down-up` | `ListChevronsUpDown` / `ListChevronsDownUp` |
| Reading pointer / search | `mouse-pointer-2` / `search` | `MousePointer2` / `Search` |
| Text highlight / area highlight | `highlighter` / `scan` | `Highlighter` / `Scan` |
| Underline / strikeout | `underline` / `strikethrough` | `Underline` / `Strikethrough` |
| Pen / eraser | `pencil` / `eraser` | `Pencil` / `Eraser` |
| Text annotation | `type` | `Type` |
| Select annotations | `mouse-pointer-2` | `MousePointer2` |
| Delete selected annotation | `trash` | `Trash` |
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
| Export annotated PDF | `download` | `Download` |

`Scan` and `Minus` currently serve more than one meaning in different tool areas; keep tooltips clear or choose distinct alternatives during further refinement.

Mappings live in `crates/breeze-app/src/commands.rs`, `reader.rs`, and `home.rs`. Icon availability does not imply that a corresponding document-writing feature is implemented.

Favorite buttons in the library and reader use the same bookmark outline when inactive and a filled version when active. The filled asset preserves the pinned Lucide bookmark path and stroke, changing only its fill. Favorite state does not add a persistent button background; normal hover and press feedback remains. Accessible labels distinguish Favorite from Remove Favorite.

## Document identity colors

Document tabs and library cover placeholders share the `file-text` icon from `crates/breeze-app/src/document_icons.rs`. Its foreground identifies the file format, independently of the application's accent color. Light and dark themes retain each format's hue with different brightness:

| Format / extension | Color family | Light theme | Dark theme |
|---|---|---|---|
| PDF / `.pdf` | Orange-red | `#E54B35` | `#FF7862` |
| EPUB / `.epub` | Green | `#56852F` | `#9BC969` |
| Markdown / `.md`, `.markdown` | Blue | `#356DCC` | `#7EABF5` |
| Unknown or missing extension | Neutral | Theme muted foreground | Theme muted foreground |

Extension matching is case-insensitive; the bundled sample uses the PDF color. Active, inactive, and hovered tabs retain this foreground. In both library views, pending or unavailable covers use the same colored `file-text` icon; a cover failure's explanation remains in the tooltip. These colors apply to the icon only. Real cover thumbnails, tab labels/backgrounds, and original page colors retain their existing presentation. EPUB and Markdown colors are presentation mappings for the planned formats; their reading backends remain unavailable.
