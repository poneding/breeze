# Breeze identity

## Concept

One open book on a charcoal rounded tile. Two ivory pages with a narrow gap at the
spine read as a single book rather than two loose leaves, and the silhouette stays
recognizable down to 16 px. No wordmark is embedded, so the icon remains usable at
small sizes and across interface languages.

The tile is flat charcoal (`#2C2C2C`) with rounded corners and the supplied clear
margin. The pages are ivory (`#F5F1E8`). The tile edge is the icon's only
antialiased boundary: the page paths are drawn *over* the tile rather than knocked
out of it, because a knockout would antialias the hole edge and the page edge
against each other and leave a translucent seam around each page. The standalone
monochrome mark is the book alone, without the tile, in `#2C2C2C`. The logo does
not change document colors or UI palette preferences.

## Files

- `breeze-icon.svg`: editable 1024 × 1024 application icon master, with a rounded
  tile and transparent outer margin.
- `breeze-mark.svg`: single-color symbol on transparency. For dark backgrounds,
  change its root fill to ivory; preserve the two page paths and their spacing.
- `breeze-icon.png`: 1024 × 1024 RGBA icon for general use and Linux packaging.
- `Breeze.icns`: macOS icon with 16, 32, 128, 256 and 512 point representations at
  1× and 2×. Used by `scripts/bundle-macos.sh` and `scripts/release.py`.
- `breeze.ico`: Windows icon with 16, 32, 48, 64, 128 and 256 pixel images.
  Provided for future Windows packaging; executable resource integration is not
  implemented by this asset change.

The SVG files are the source of truth. Each raster size is rendered at its target
size by the macOS QuickLook SVG renderer and rasterized twice — once as authored
and once over an added black background rectangle — so the coverage difference
between the two renders recovers exact alpha for the transparent margin. The
antialiased tile edge is then repainted to the flat tile color, which keeps the
edge pixels free of the light fringe the white render would otherwise leave, and
`iconutil` assembles the ICNS container. ImageMagick performs the channel
arithmetic and the ICO container. These are asset-authoring tools, not application
dependencies. Create disposable intermediates beneath `target/`.

Do not stretch, outline, add text to, or rearrange the mark. Leave generous clear
space around the standalone symbol. Keep the application icon's margin; do not add
a second rounded tile around it.

## Verification scope

Rasterization is checked by reading back pixel values: the outer margin stays
transparent and the page interiors stay opaque at 16, 32, 48, 64, 128, 256, 512,
and 1024 px, and the ICNS and ICO containers are re-extracted and sampled. The
large icon and the small-size rasters are then inspected visually over both dark
and light backgrounds. This is not native Dock, Windows taskbar, or Linux desktop
acceptance, and no in-reader branding area is added.
