#!/bin/sh
# Local review bundle, not a signed/notarized distribution artifact.
set -eu
cd "$(dirname "$0")/.."
if [ "$(uname -s)" != Darwin ]; then
  echo "This helper is only for the local macOS review bundle." >&2
  exit 1
fi
cargo build --locked -p breeze-app
bundle="$PWD/target/Breeze.app"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
cp assets/brand/Breeze.icns "$bundle/Contents/Resources/Breeze.icns"
cp LICENSE "$bundle/Contents/Resources/LICENSE"
cp Cargo.lock README.md docs/releases.md "$bundle/Contents/Resources/"
mkdir -p "$bundle/Contents/Resources/licenses"
cp assets/licenses/COPYING-MUPDF.txt assets/licenses/NOTICE-MUPDF.md "$bundle/Contents/Resources/licenses/"
cat > "$bundle/Contents/Resources/SOURCE-CODE.txt" <<'SOURCE'
Breeze is licensed under AGPL-3.0-only. This local review bundle was built from
the Breeze checkout used to invoke scripts/bundle-macos.sh. Published versions
include a vendored corresponding-source archive alongside their installers.
SOURCE
cp target/debug/breeze-app "$bundle/Contents/MacOS/Breeze.next"
mv "$bundle/Contents/MacOS/Breeze.next" "$bundle/Contents/MacOS/Breeze"
cat > "$bundle/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>dev.breeze.reader.prototype</string>
<key>CFBundleName</key><string>Breeze</string>
<key>CFBundleDisplayName</key><string>Breeze</string>
<key>CFBundleExecutable</key><string>Breeze</string>
<key>CFBundleIconFile</key><string>Breeze.icns</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.1.0</string>
<key>NSHighResolutionCapable</key><true/>
<key>LSMinimumSystemVersion</key><string>15.0</string>
<!-- A viewer claim puts Breeze in Finder's Open With menu. The Alternate
     handler rank means Launch Services never takes the default over. -->
<key>CFBundleDocumentTypes</key><array><dict>
<key>CFBundleTypeName</key><string>PDF Document</string>
<key>CFBundleTypeRole</key><string>Viewer</string>
<key>LSHandlerRank</key><string>Alternate</string>
<key>LSItemContentTypes</key><array><string>com.adobe.pdf</string></array>
<key>CFBundleTypeExtensions</key><array><string>pdf</string></array>
</dict></array>
</dict></plist>
PLIST
printf 'Local review bundle: %s\n' "$bundle"
