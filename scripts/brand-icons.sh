#!/bin/sh
# Regenerate the Breeze application-icon rasters from the SVG master.
#
# Every raster is the master at a different render size, with nothing added and
# nothing cropped: the master's own viewBox is the canvas, so the tile keeps
# Apple's app-icon grid (824 of 1024 px) in the Dock, in the About page, in the
# Windows installer and in the Linux hicolor icon alike. Nothing in this script
# second-guesses the master's composition.
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
# The master's canvas is 1024 px and its tile covers 824 of them. The rasters are
# checked against that grid so a resized or recentred tile cannot ship unnoticed.
CANVAS_PX=1024
ARTWORK_PX=824
# No raster is rendered from a sample grid smaller than this.
MIN_SAMPLED_PX=1024
# Every Windows representation, largest first: the ICO container order follows
# this list.
ICO_SIZES="256 128 64 48 32 16"
# macOS representations as "pixels:iconset file name".
ICNS_ENTRIES="16:icon_16x16 32:icon_16x16@2x 32:icon_32x32 64:icon_32x32@2x \
128:icon_128x128 256:icon_128x128@2x 256:icon_256x256 512:icon_256x256@2x \
512:icon_512x512 1024:icon_512x512@2x"

for tool in magick iconutil awk sed; do
	if ! command -v "$tool" >/dev/null 2>&1; then
		echo "brand-icons: $tool is required" >&2
		exit 1
	fi
done

rm -rf "$WORK"
mkdir -p "$WORK/Breeze.iconset" "$WORK/ico"

# render <pixels> <destination>
#
# Rasterize four times the target size, never below MIN_SAMPLED_PX, and filter
# down: ImageMagick's own SVG renderer antialiases curves coarsely, and the page
# corners have to stay smooth from 16 px upwards. The master is rendered as
# authored; only its declared pixel size is raised for the sample grid.
render() {
	pixels=$1
	destination=$2
	sampled=$((pixels * 4))
	if [ "$sampled" -lt "$MIN_SAMPLED_PX" ]; then
		sampled=$MIN_SAMPLED_PX
	fi
	sed -e "s|width=\"[0-9]*\"|width=\"$sampled\"|" \
		-e "s|height=\"[0-9]*\"|height=\"$sampled\"|" \
		"$MASTER" >"$WORK/render.svg"
	# `-strip` drops the date and profile chunks ImageMagick would otherwise
	# write, so the same master produces byte-identical rasters on every run and
	# no local timestamp reaches a release asset.
	magick -background none "$WORK/render.svg" -filter Lanczos \
		-resize "${pixels}x${pixels}" -strip "png32:$destination"
}

# macOS app icon. `bundle-macos.sh` and `release.py` ship this container.
for entry in $ICNS_ENTRIES; do
	render "${entry%%:*}" "$WORK/Breeze.iconset/${entry#*:}.png"
done
iconutil -c icns "$WORK/Breeze.iconset" -o "$BRAND/Breeze.icns"

# The 1024 px raster is the same picture as the app icon, and is also what the
# About page decodes.
render "$CANVAS_PX" "$BRAND/breeze-icon.png"

# Windows installer artwork and the Linux hicolor icon are the same picture again.
ico_inputs=""
for pixels in $ICO_SIZES; do
	render "$pixels" "$WORK/ico/$pixels.png"
	ico_inputs="$ico_inputs $WORK/ico/$pixels.png"
done
# shellcheck disable=SC2086
magick $ico_inputs "$BRAND/breeze.ico"
render 256 "$BRAND/breeze-icon-256.png"

# Report what each raster actually contains, and fail when the artwork no longer
# covers the master's share of the canvas. A 50% alpha threshold measures the
# tile's rounded square; the two-pixel tolerance covers threshold rounding at
# 16 px.
failed=0
check() {
	file=$1
	canvas=$(magick "$file" -format '%w' info:)
	bbox=$(magick "$file" -alpha extract -threshold 50% -trim -format '%wx%h' info:)
	corner=$(magick "$file" -alpha extract -format '%[fx:int(255*u.p{0,0})]' info:)
	want=$(((canvas * ARTWORK_PX + CANVAS_PX / 2) / CANVAS_PX))
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

check "$BRAND/breeze-icon.png"
for entry in $ICNS_ENTRIES; do
	check "$WORK/Breeze.iconset/${entry#*:}.png"
done
for pixels in $ICO_SIZES; do
	check "$WORK/ico/$pixels.png"
done
check "$BRAND/breeze-icon-256.png"

if [ "$failed" != 0 ]; then
	exit 1
fi
printf 'brand-icons: regenerated %s, %s, %s and %s\n' \
	"$BRAND/breeze-icon.png" "$BRAND/Breeze.icns" "$BRAND/breeze.ico" "$BRAND/breeze-icon-256.png"
