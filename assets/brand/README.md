# Breeze identity

## Concept

Two wind-lifted pages express calm reading and the application's name without
binding the identity to PDF. The offset, rising right edges give the mark its
silhouette. The two stacked forms loosely suggest a lowercase **b**, but the
primary reading is moving paper, not a literal letter. No wordmark is embedded,
so the icon remains usable at small sizes and across interface languages.

The tile uses a warm paper gradient from `#EAE2D5` to `#CEBDA4`. Ivory pages
range from `#FFFEF9` to `#EFE3CF`, with a slightly darker rear page. Rounded
corners, a narrow gap, thin lower edges, and soft shadows give the compact mark
subtle paper-like depth rather than a glossy or heavily extruded 3D appearance.
The page group is enlarged by 25% around its visual center for a fuller tile,
while preserving the outer tile and page proportions.
The standalone monochrome mark uses warm ink (`#82715B`). The logo does not
change document colors or UI palette preferences.

## Files

- `breeze-icon.svg`: editable 1024 × 1024 application icon master, with a rounded
  tile and transparent outer margin.
- `breeze-mark.svg`: single-color symbol on transparency. For dark backgrounds,
  change its root fill to ivory; preserve both paths and their spacing.
- `breeze-icon.png`: 1024 × 1024 RGBA icon for general use and Linux packaging.
- `Breeze.icns`: macOS icon with 16, 32, 128, 256 and 512 point representations
  at 1× and 2×. Used by `scripts/bundle-macos.sh`.
- `breeze.ico`: Windows icon with 16, 32, 48, 64, 128 and 256 pixel images.
  Provided for future Windows packaging; executable resource integration is not
  implemented by this asset change.

The SVG files are the source of truth. The master PNG was rasterized with
resvg to preserve SVG gradients and shadows, and resized with ImageMagick; the
ICNS container was assembled with macOS `iconutil`. These are
asset-authoring tools, not application dependencies. Use transparent backgrounds
when rasterizing. Create disposable intermediate iconsets beneath `target/`.

Do not stretch, outline, add text to, or rearrange the mark. Leave generous clear
space around the standalone symbol. Keep the application icon's supplied margin;
do not add a second rounded tile around it. These are original project assets,
not third-party stock artwork. This design has not undergone trademark clearance.

## Verification scope

The exported large icon and small-size raster are visually inspected during
creation. This is not native Dock, Windows taskbar, or Linux desktop acceptance.
macOS review bundles reference the ICNS file; no in-reader branding area is added.
