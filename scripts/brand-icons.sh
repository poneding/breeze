#!/bin/sh
# Regenerate the Breeze application-icon rasters from the SVG master.
#
# Every raster is the master at a different render size, with nothing added and
# nothing cropped: the master's own viewBox is the canvas, so the tile keeps
# Apple's app-icon grid (824 of 1024 px) in the Dock, in the About page, in the
# Windows installer and in the Linux hicolor icon alike. Nothing in this script
# second-guesses the master's composition.
#
# The system SVG renderer — the one QuickLook uses — draws the file, because it is
# the only renderer available here that reproduces the tile's rounded corners as
# curves: ImageMagick's own SVG renderer flattens each corner into straight
# segments, which deviates by up to 3.8 px on a 1024 px arc and leaves the tile
# looking cut off. ImageMagick is therefore used for channel arithmetic and the ICO
# container only, never to draw the artwork.
#
# QuickLook composites onto an opaque background, so the transparent margin
# survives only as the difference between two renders of the same size:
# `alpha = 255 - (over white - over black)` and `colour = over black / alpha`,
# which recovers the authored colours exactly and leaves every transparent pixel at
# zero. The two renders also have to come from one graphical session, so this
# script needs a logged-in Mac and cannot run over a plain SSH connection.
#
# Requirements: macOS with QuickLook (`qlmanage`, `iconutil`) and ImageMagick 7.
set -eu
cd "$(dirname "$0")/.."

BRAND=assets/brand
MASTER=$BRAND/breeze-icon.svg
WORK=target/brand-icons
# The master's canvas is 1024 px and its tile covers 824 of them. The rasters are
# checked against that grid so a resized or recentred tile cannot ship unnoticed.
CANVAS_PX=1024
ARTWORK_PX=824
# Every size that appears in a container or as a shipped raster, rendered once.
SIZES="16 32 48 64 128 256 512 1024"
# macOS representations as "pixels:iconset file name".
ICNS_ENTRIES="16:icon_16x16 32:icon_16x16@2x 32:icon_32x32 64:icon_32x32@2x \
128:icon_128x128 256:icon_128x128@2x 256:icon_256x256 512:icon_256x256@2x \
512:icon_512x512 1024:icon_512x512@2x"
# Windows representations, largest first: the ICO container order follows this list.
ICO_SIZES="256 128 64 48 32 16"

for tool in magick iconutil qlmanage awk sed; do
	if ! command -v "$tool" >/dev/null 2>&1; then
		echo "brand-icons: $tool is required" >&2
		exit 1
	fi
done

rm -rf "$WORK"
mkdir -p "$WORK/Breeze.iconset" "$WORK/svg" "$WORK/png"

# render <pixels>
#
# Draw the master at exactly that size, once over white and once over black, and
# recover the straight-alpha raster into $WORK/png. The size comes from QuickLook's
# own thumbnail size rather than from the master's width and height: declaring a
# size that matches the request makes QuickLook render the artwork smaller than
# the canvas, so the master is always rendered exactly as authored.
render() {
	pixels=$1
	# A rectangle in the master's canvas units is what the renderer composites
	# against, so the transparent margin shows up as pure background.
	sed -e "s|<g transform|<rect x=\"0\" y=\"0\" width=\"$CANVAS_PX\" height=\"$CANVAS_PX\" fill=\"#FFFFFF\"/>\n  <g transform|" \
		"$MASTER" >"$WORK/svg/white-$pixels.svg"
	sed -e "s|<g transform|<rect x=\"0\" y=\"0\" width=\"$CANVAS_PX\" height=\"$CANVAS_PX\" fill=\"#000000\"/>\n  <g transform|" \
		"$MASTER" >"$WORK/svg/black-$pixels.svg"
	qlmanage -t -s "$pixels" -o "$WORK/svg" \
		"$WORK/svg/white-$pixels.svg" "$WORK/svg/black-$pixels.svg" >/dev/null 2>&1
	for background in white black; do
		thumbnail="$WORK/svg/$background-$pixels.svg.png"
		if [ ! -f "$thumbnail" ]; then
			echo "brand-icons: QuickLook produced no $pixels px $background render" >&2
			exit 1
		fi
	done
	# The two composites differ only by the tile's coverage, because the black
	# render is the premultiplied colour: white - black = 255 - alpha.
	magick "$WORK/svg/white-$pixels.svg.png" -alpha off \
		"$WORK/svg/black-$pixels.svg.png" -alpha off \
		-compose MinusSrc -composite -colorspace Gray -negate "$WORK/svg/alpha-$pixels.png"
	# Dividing the premultiplied colour by that alpha un-premultiplies it, which is
	# what keeps charcoal and ivory at their authored values on antialiased edges.
	magick "$WORK/svg/black-$pixels.svg.png" -alpha off "$WORK/svg/alpha-$pixels.png" \
		-compose DivideSrc -composite -alpha off "$WORK/svg/color-$pixels.png"
	# `-strip` drops the date and profile chunks ImageMagick would otherwise write,
	# so the same master produces byte-identical rasters on every run and no local
	# timestamp reaches a release asset.
	magick "$WORK/svg/color-$pixels.png" "$WORK/svg/alpha-$pixels.png" -alpha off \
		-compose CopyOpacity -composite -strip "png32:$WORK/png/$pixels.png"
}

for pixels in $SIZES; do
	render "$pixels"
done

# macOS app icon. `bundle-macos.sh` and `release.py` ship this container.
for entry in $ICNS_ENTRIES; do
	cp "$WORK/png/${entry%%:*}.png" "$WORK/Breeze.iconset/${entry#*:}.png"
done
iconutil -c icns "$WORK/Breeze.iconset" -o "$BRAND/Breeze.icns"

# Windows installer artwork.
ico_inputs=""
for pixels in $ICO_SIZES; do
	ico_inputs="$ico_inputs $WORK/png/$pixels.png"
done
# shellcheck disable=SC2086
magick $ico_inputs "$BRAND/breeze.ico"

# Shipped rasters: the 1024 px master at full size (also what the About page
# decodes) and the 256 px Linux desktop icon.
cp "$WORK/png/$CANVAS_PX.png" "$BRAND/breeze-icon.png"
cp "$WORK/png/256.png" "$BRAND/breeze-icon-256.png"

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

for pixels in $SIZES; do
	check "$WORK/png/$pixels.png"
done
check "$BRAND/breeze-icon.png"
check "$BRAND/breeze-icon-256.png"

for entry in $ICNS_ENTRIES; do
	[ -f "$WORK/Breeze.iconset/${entry#*:}.png" ] || {
		echo "brand-icons: iconset entry ${entry#*:} is missing" >&2
		failed=1
	}
done
[ "$(magick identify "$BRAND/breeze.ico" | wc -l | tr -d ' ')" = "6" ] || {
	echo "brand-icons: the ICO container does not hold six images" >&2
	failed=1
}

if [ "$failed" != 0 ]; then
	exit 1
fi
printf 'brand-icons: regenerated %s, %s, %s and %s\n' \
	"$BRAND/breeze-icon.png" "$BRAND/Breeze.icns" "$BRAND/breeze.ico" "$BRAND/breeze-icon-256.png"
