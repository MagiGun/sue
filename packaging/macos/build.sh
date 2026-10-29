#!/usr/bin/env bash
# Builds Sue.app for macOS (one binary for Apple silicon and Intel) and a
# disk image to install it from.
#
#   packaging/macos/build.sh
#
# Needs macOS with the Xcode command line tools and the Rust targets
# aarch64-apple-darwin and x86_64-apple-darwin. Signing is left to
# `codesign` with your own identity (SUE_SIGN_IDENTITY), else the app is
# signed ad hoc so it runs on this Mac.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
# A checkout with Windows line endings leaves a carriage return to strip.
version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1 | tr -d '\r')"
out="$root/dist"
work="$root/target/macos-package"
rm -rf "$work"
mkdir -p "$out" "$work"

for target in aarch64-apple-darwin x86_64-apple-darwin; do
  cargo build --release -p sue --target "$target"
done
app="$work/Sue.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
lipo -create -output "$app/Contents/MacOS/sue" \
  target/aarch64-apple-darwin/release/sue target/x86_64-apple-darwin/release/sue
sed "s/@VERSION@/$version/g" packaging/macos/Info.plist > "$app/Contents/Info.plist"
cp LICENSE "$app/Contents/Resources/LICENSE"

# The icon, every size iconutil wants, from the PNG.
iconset="$work/sue.iconset"
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" assets/icons/sue_icon.png --out "$iconset/icon_${size}x${size}.png" >/dev/null
  double=$((size * 2))
  sips -z "$double" "$double" assets/icons/sue_icon.png --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/sue.icns"

codesign --force --deep --sign "${SUE_SIGN_IDENTITY:--}" "$app"
ditto -c -k --keepParent "$app" "$out/Sue-$version-macos.zip"
hdiutil create -volname Sue -srcfolder "$app" -ov -format UDZO "$out/Sue-$version-macos.dmg"
ls -l "$out"
