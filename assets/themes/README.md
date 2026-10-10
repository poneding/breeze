# GPUI Kit theme collection

`gpui-kit/` contains unchanged theme JSON files from the official
[GPUI Kit theme gallery](https://github.com/longbridge/gpui-kit/tree/0c830f4d257e69fdd17200650533ab4ca9a40cc0/themes),
pinned to commit `0c830f4d257e69fdd17200650533ab4ca9a40cc0`. This is the source
commit recorded by the published `gpui-component 0.7.0` dependency in
`.cargo_vcs_info.json`.

The 21 files define 36 named themes (11 light, 25 dark). Kit itself supplies
Default Light and Default Dark, giving 38 themes. Breeze adds Follow System as
an application preference, not a theme. The dropdown groups these choices by
System, Light, and Dark, retaining every upstream theme name.

The files are compiled into Breeze and loaded into `ThemeRegistry` at startup
using `load_themes_from_str`. No network request, user theme directory, runtime
filesystem read, or theme-directory watcher is needed. Keep these assets in
source control so packaged applications have the same choices as development
builds.

All files were checked against the Git blob IDs returned for the pinned
upstream tree. Preserve their author and URL metadata. The upstream repository
license is retained at
[`../licenses/LICENSE-GPUI-KIT-THEMES.txt`](../licenses/LICENSE-GPUI-KIT-THEMES.txt).
These are upstream theme definitions, not Breeze-designed palettes. When
updating them, review the upstream version, license notices, theme names/modes,
and compatibility with the locked GPUI Kit schema; update the complete-list
regression test as an explicit product change.
