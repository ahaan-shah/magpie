#!/usr/bin/env bash
# Builds Magpie.app (universal: Apple Silicon + Intel) plus a .dmg and .zip.
# Run on macOS: ./scripts/bundle-macos.sh [version]
set -euo pipefail
cd "$(dirname "$0")/.."
VERSION="${1:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)}"
OUT=dist
APP="$OUT/Magpie.app"

rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null
for t in aarch64-apple-darwin x86_64-apple-darwin; do
    MACOSX_DEPLOYMENT_TARGET=11.0 cargo build --profile dist --locked --target "$t" -p magpie-finance
done

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
lipo -create -output "$APP/Contents/MacOS/magpie" \
    target/aarch64-apple-darwin/dist/magpie target/x86_64-apple-darwin/dist/magpie
sed "s/__VERSION__/$VERSION/g" packaging/macos/Info.plist > "$APP/Contents/Info.plist"

# Icon: 1024px PNG -> .iconset -> .icns
ICONSET="$OUT/magpie.iconset"
rm -rf "$ICONSET" && mkdir -p "$ICONSET"
for s in 16 32 128 256 512; do
    sips -z $s $s packaging/linux/icon-1024.png --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
    sips -z $((s * 2)) $((s * 2)) packaging/linux/icon-1024.png --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/magpie.icns"
cp THIRD-PARTY-LICENSES.txt LICENSE "$APP/Contents/Resources/"
rm -rf "$ICONSET"

# Ad-hoc signature so Apple Silicon will run it.
codesign --force --deep --sign - "$APP"

(cd "$OUT" && rm -f Magpie-macos-universal.zip && ditto -c -k --keepParent Magpie.app Magpie-macos-universal.zip)

STAGE="$OUT/dmg"
rm -rf "$STAGE" && mkdir -p "$STAGE"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
rm -f "$OUT/Magpie-macos-universal.dmg"
hdiutil create -volname "Magpie" -srcfolder "$STAGE" -ov -format UDZO "$OUT/Magpie-macos-universal.dmg" >/dev/null
rm -rf "$STAGE"
echo "Built $OUT/Magpie-macos-universal.dmg and .zip (v$VERSION)"
