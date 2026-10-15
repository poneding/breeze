# Breeze identity

## Concept

One open book on a charcoal rounded tile. Two ivory pages with a narrow gap at the
spine read as a single book rather than two loose leaves, and the silhouette stays
recognizable down to 16 px. No wordmark is embedded, so the icon remains usable at
small sizes and across interface languages.

The tile is flat charcoal (`#2C2C2C`) with rounded corners. The pages are ivory
(`#F5F1E8`). The tile edge is the icon's only antialiased boundary: the page paths
are drawn *over* the tile rather than knocked out of it, because a knockout would
antialias the hole edge and the page edge against each other and leave a
translucent seam around each page. The standalone monochrome mark is the book
alone, without the tile, in `#2C2C2C`. The logo does not change document colors or
UI palette preferences.

The master draws the tile alone, with no margin of its own, because the platforms
frame an icon differently. macOS places every application icon on Apple's grid,
where the artwork covers 824 of 1024 px and the system adds nothing further; the
shipped system icons measure 206 of 256 px and 104 of 128 px on that same grid. A
tile with a wider margin sits visibly further from its Dock neighbors than they sit
from each other. Windows and Linux draw an icon file edge to edge instead, so a
margin there is space the icon loses in a taskbar. Each raster therefore carries
the canvas its own consumer expects rather than one shared composition.

## Files

- `breeze-icon.svg`: editable 1024 × 1024 master. The rounded tile fills the
  canvas and each platform canvas is added while rasterizing.
- `breeze-mark.svg`: single-color symbol on transparency. For dark backgrounds,
  change its root fill to ivory; preserve the two page paths and their spacing.
- `breeze-icon.png`: 1024 × 1024 RGBA icon on the macOS grid, so the artwork covers
  824 px. Supplies the About page tile, general use, and installer artwork.
- `breeze-icon-256.png`: 256 × 256 RGBA icon drawn edge to edge for Linux desktop
  integration (`hicolor` themes and the AppImage). Installers cannot resize it
  themselves because the release tool is standard-library-only.
- `Breeze.icns`: macOS icon with 16, 32, 128, 256 and 512 point representations at
  1× and 2×, all on the 824 px grid. Used by `scripts/bundle-macos.sh` and
  `scripts/release.py`.
- `breeze.ico`: Windows icon with 16, 32, 48, 64, 128 and 256 pixel images, drawn
  edge to edge and stored as 32 bpp BGRA so the tile keeps its antialiased edges
  and its rounded corners. Used by the Windows installer resources; executable
  resource integration is not implemented by this asset change.

## Regenerating the rasters

`scripts/brand-icons.sh` reproduces every raster above from the master SVG. It
needs macOS for `iconutil` and ImageMagick 7 for `magick`, and is not part of the
application build:

```sh
scripts/brand-icons.sh
```

The script renders each target size at four times its final width — never below
1024 px — and filters down with Lanczos. ImageMagick's own SVG renderer
antialiases curves coarsely at small sizes, and the page corners have to stay
smooth from 16 px upwards. The master's plain paths need no gradients, filters, or
shadows, so the renderer draws the flat tile and page colors exactly and no alpha
recovery step is required. For the macOS rasters the script widens the viewBox
around the tile until the artwork covers 824 of 1024 px; for the Windows and Linux
rasters it keeps the master's own viewBox, which is edge to edge. ImageMagick
writes the ICO container from those renders. Intermediates stay beneath `target/`.

Do not stretch, outline, add text to, or rearrange the mark. Leave generous clear
space around the standalone symbol. The raster margins are the platforms' own
framing: do not add a second rounded tile, a drop shadow, or a second margin.

## Verification scope

The script prints the canvas and the measured artwork span of every raster and
fails when the artwork no longer covers the share its platform expects, which is
what keeps the icon from quietly shrinking again. `crates/breeze-app` repeats that
check for the raster it embeds, so a re-export reaches a failing test instead of a
release. On macOS, IconServices caches an application icon against the bundle's
own modification date, so a review bundle rebuilt in place can keep showing the
previous artwork in the Dock; `scripts/bundle-macos.sh` touches the bundle for
that reason. Rasterization is otherwise checked by reading back pixel values: the
canvas corner stays transparent and the page interiors stay opaque at 16, 32, 48,
64, 128, 256, 512, and 1024 px, and the ICNS and ICO containers are re-extracted
and sampled. This is not native Dock, Windows taskbar, or Linux desktop
acceptance, and no in-reader branding area is added.
