#!/bin/sh
# Regenerate the Breeze application-icon rasters from the SVG master.
#
# Platform canvases differ. macOS frames an app icon on Apple's grid, where the
# artwork covers 824 of 1024 px and the canvas supplies the clear margin, while
# Windows and Linux draw an icon file edge to edge. The master therefore holds
# the tile alone and this script adds whichever canvas each consumer expects.
#
# ImageMagick rasterizes the master at every target size and assembles the ICO
# container; iconutil assembles the ICNS container. These are asset-authoring
# tools, not application dependencies. Intermediates stay under target/.
#
# Requires macOS for iconutil, plus ImageMagick 7 for `magick`.
set -eu
cd "$(dirname "$0")/.."

BRAND=assets/brand
MASTER=$BRAND/breeze-icon.svg
WORK=target/brand-icons
# Apple's macOS app-icon grid, in 1024ths of the canvas.
MACOS_ARTWORK_PX=824
MACOS_CANVAS_PX=1024
# Artwork shares below are expressed in 1024ths of a canvas, and no raster is
# rendered from a sample grid smaller than this.
SHARE_UNIT=1024
MIN_SAMPLED_PX=1024
# Every Windows representation, largest first: the ICO container order follows
# this list.
ICO_SIZES="256 128 64 48 32 16"
# macOS representations as "pixels:iconset file name".
ICNS_ENTRIES="16:icon_16x16 32:icon_16x16@2x 32:icon_32x32 64:icon_32x32@2x \
128:icon_128x128 256:icon_128x128@2x 256:icon_256x256 512:icon_256x256@2x \
512:icon_512x512 1024:icon_512x512@2x"
# The tile's rounded square spans these units in the master's viewBox.
TILE_MIN=2
TILE_SIZE=20

for tool in magick iconutil awk sed; do
	if ! command -v "$tool" >/dev/null 2>&1; then
		echo "brand-icons: $tool is required" >&2
		exit 1
	fi
done

rm -rf "$WORK"
mkdir -p "$WORK/Breeze.iconset" "$WORK/ico"

# Centering the tile on a canvas that keeps `artwork` of `canvas` pixels for it
# needs a margin of (canvas / artwork - 1) * TILE_SIZE / 2 units per side.
macos_viewbox=$(awk -v artwork="$MACOS_ARTWORK_PX" -v canvas="$MACOS_CANVAS_PX" \
	-v min="$TILE_MIN" -v size="$TILE_SIZE" 'BEGIN {
		margin = (canvas / artwork - 1) * size / 2
		printf "%.5f %.5f %.5f %.5f", min - margin, min - margin, size + 2 * margin, size + 2 * margin
	}')

# render <viewBox override> <pixels> <destination>
#
# Rasterize four times the target size, never below MIN_SAMPLED_PX, and filter
# down: ImageMagick's own SVG renderer antialiases curves coarsely, and the page
# corners have to stay smooth from 16 px upwards. An empty override keeps the
# master's own full-bleed viewBox, which renders the tile edge to edge.
render() {
	viewbox=$1
	pixels=$2
	destination=$3
	sampled=$((pixels * 4))
	if [ "$sampled" -lt "$MIN_SAMPLED_PX" ]; then
		sampled=$MIN_SAMPLED_PX
	fi
	if [ -n "$viewbox" ]; then
		sed -e "s|width=\"[0-9]*\"|width=\"$sampled\"|" \
			-e "s|height=\"[0-9]*\"|height=\"$sampled\"|" \
			-e "s|viewBox=\"[^\"]*\"|viewBox=\"$viewbox\"|" \
			"$MASTER" >"$WORK/render.svg"
	else
		sed -e "s|width=\"[0-9]*\"|width=\"$sampled\"|" \
			-e "s|height=\"[0-9]*\"|height=\"$sampled\"|" \
			"$MASTER" >"$WORK/render.svg"
	fi
	# `-strip` drops the date and profile chunks ImageMagick would otherwise
	# write, so the same master produces byte-identical rasters on every run and
	# no local timestamp reaches a release asset.
	magick -background none "$WORK/render.svg" -filter Lanczos \
		-resize "${pixels}x${pixels}" -strip "png32:$destination"
}

# macOS app icon. `bundle-macos.sh` and `release.py` ship this container.
for entry in $ICNS_ENTRIES; do
	render "$macos_viewbox" "${entry%%:*}" "$WORK/Breeze.iconset/${entry#*:}.png"
done
iconutil -c icns "$WORK/Breeze.iconset" -o "$BRAND/Breeze.icns"

# General 1024 px raster, also the About page tile. It follows the macOS canvas
# so the logo reads at one size in the Dock, in About, and in installer artwork.
render "$macos_viewbox" "$MACOS_CANVAS_PX" "$BRAND/breeze-icon.png"

# Windows installer artwork and the Linux hicolor icon are drawn edge to edge,
# which is how both platforms display an icon file.
ico_inputs=""
for pixels in $ICO_SIZES; do
	render "" "$pixels" "$WORK/ico/$pixels.png"
	ico_inputs="$ico_inputs $WORK/ico/$pixels.png"
done
# shellcheck disable=SC2086
magick $ico_inputs "$BRAND/breeze.ico"
render "" 256 "$BRAND/breeze-icon-256.png"

# Report what each raster actually contains, and fail when a canvas no longer
# holds the artwork share its platform expects. A 50% alpha threshold measures
# the tile's rounded square; the two-pixel tolerance covers threshold rounding
# at 16 px.
failed=0
check() {
	file=$1
	artwork_px=$2
	canvas=$(magick "$file" -format '%w' info:)
	bbox=$(magick "$file" -alpha extract -threshold 50% -trim -format '%wx%h' info:)
	corner=$(magick "$file" -alpha extract -format '%[fx:int(255*u.p{0,0})]' info:)
	want=$(((canvas * artwork_px + 512) / SHARE_UNIT))
	printf '%-32s %5s px canvas  %-9s artwork  %4s px expected  corner alpha %s\n' \
		"$file" "$canvas" "$bbox" "$want" "$corner"
	if [ "$corner" != 0 ]; then
		echo "brand-icons: $file: canvas corner is not transparent" >&2
		failed=1
	fi
	for edge in "${bbox%x*}" "${bbox#*x}"; do
		if [ "$edge" -lt $((want - 2)) ] || [ "$edge" -gt $((want + 2)) ]; then
			echo "brand-icons: $file: artwork $bbox is not $want px" >&2
			failed=1
			break
		fi
	done
}

check "$BRAND/breeze-icon.png" "$MACOS_ARTWORK_PX"
for entry in $ICNS_ENTRIES; do
	check "$WORK/Breeze.iconset/${entry#*:}.png" "$MACOS_ARTWORK_PX"
done
for pixels in $ICO_SIZES; do
	check "$WORK/ico/$pixels.png" "$SHARE_UNIT"
done
check "$BRAND/breeze-icon-256.png" "$SHARE_UNIT"

if [ "$failed" != 0 ]; then
	exit 1
fi
printf 'brand-icons: regenerated %s, %s, %s and %s\n' \
	"$BRAND/breeze-icon.png" "$BRAND/Breeze.icns" "$BRAND/breeze.ico" "$BRAND/breeze-icon-256.png"
