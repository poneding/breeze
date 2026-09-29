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
mkdir -p "$bundle/Contents/MacOS"
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
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.1.0</string>
<key>NSHighResolutionCapable</key><true/>
<key>LSMinimumSystemVersion</key><string>15.0</string>
</dict></plist>
PLIST
printf 'Local review bundle: %s\n' "$bundle"
