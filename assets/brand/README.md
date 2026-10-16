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

The master draws the tile on Apple's app-icon grid: the artwork covers 824 of the
1024 px canvas and the rest is transparent. macOS frames every application icon
that way — the shipped system icons measure 206 of 256 px and 104 of 128 px — so a
tile with that share sits in the Dock the way its neighbours do. A smaller tile
drifts away from them, and a tile that fills its canvas reads as a rounded corner
cut off by the frame. Every raster in this directory is this one composition at
another size; none of them is re-framed per platform.

## Files

- `breeze-icon.svg`: editable 1024 × 1024 master. The tile covers 824 px of the
  canvas, and every raster below is this file rendered at another size.
- `breeze-mark.svg`: single-color symbol on transparency. For dark backgrounds,
  change its root fill to ivory; preserve the two page paths and their spacing.
- `breeze-icon.png`: 1024 × 1024 RGBA icon, the master at full size. Supplies the
  About page tile, general use, and installer artwork.
- `breeze-icon-256.png`: 256 × 256 RGBA icon for Linux desktop integration
  (`hicolor` themes and the AppImage). Installers cannot resize it themselves
  because the release tool is standard-library-only.
- `Breeze.icns`: macOS icon with 16, 32, 128, 256 and 512 point representations at
  1× and 2×. Used by `scripts/bundle-macos.sh` and `scripts/release.py`.
- `breeze.ico`: Windows icon with 16, 32, 48, 64, 128 and 256 pixel images, stored
  as 32 bpp BGRA so the tile keeps its antialiased edges and its rounded corners.
  Used by the Windows installer resources; executable resource integration is not
  implemented by this asset change.

## Regenerating the rasters

`scripts/brand-icons.sh` reproduces every raster above from the master SVG. It
needs macOS for `iconutil` and ImageMagick 7 for `magick`, and is not part of the
application build:

```sh
scripts/brand-icons.sh
```

Each raster is the master at one render size, so the composition stays exactly as
authored and this script never crops, pads, or re-centres it. To keep the curved
page corners smooth, every size is rasterized at four times its final width — never
below 1024 px — and filtered back down with Lanczos; ImageMagick's own SVG renderer
antialiases curves coarsely, which shows at 16 px. The master's plain paths need no
gradients, filters, or shadows, so the renderer draws the flat tile and page colors
exactly and no alpha recovery step is required. ImageMagick writes the ICO
container and `iconutil` assembles the ICNS container. These are asset-authoring
tools, not application dependencies. Intermediates stay beneath `target/`.

Do not stretch, outline, add text to, or rearrange the mark. Leave generous clear
space around the standalone symbol. Keep the application icon's canvas margin: the
tile belongs on the 824 px grid with the frame left transparent, and no second
rounded tile, shadow, or inner margin belongs around it.

## Verification scope

The script prints the canvas and the measured artwork span of every raster and
fails when the artwork no longer covers the master's share of the canvas, which is
what keeps the icon from quietly shrinking again. `crates/breeze-app` repeats that
check for the raster it embeds, so a re-export reaches a failing test instead of a
release. On macOS, IconServices caches an application icon against the bundle's own
modification date, so a review bundle rebuilt in place can keep showing the
previous artwork in the Dock; `scripts/bundle-macos.sh` touches the bundle for
that reason. Rasterization is otherwise checked by reading back pixel values: the
canvas corner stays transparent and the page interiors stay opaque at 16, 32, 48,
64, 128, 256, 512, and 1024 px, and the ICNS and ICO containers are re-extracted
and sampled. This is not native Dock, Windows taskbar, or Linux desktop
acceptance, and no in-reader branding area is added.
