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
# The release's version (SUE_VERSION, set by the Release workflow from the
# version typed or the tag pushed), else Cargo.toml's. A checkout with
# Windows line endings leaves a carriage return to strip.
version="${SUE_VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1 | tr -d '\r')}"
version="${version#v}"
export SUE_VERSION="$version"
out="$root/dist"
work="$root/target/macos-package"
rm -rf "$work"
mkdir -p "$out" "$work"

for target in aarch64-apple-darwin x86_64-apple-darwin; do
  cargo build --release -p sue --bins --target "$target"
done
app="$work/Sue.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
lipo -create -output "$app/Contents/MacOS/sue" \
  target/aarch64-apple-darwin/release/sue target/x86_64-apple-darwin/release/sue
cli="$work/sue-cli"
mkdir -p "$cli"
lipo -create -output "$cli/sue-cli" \
  target/aarch64-apple-darwin/release/sue-cli target/x86_64-apple-darwin/release/sue-cli
# On a CI runner two release builds nearly fill the disk, and the disk
# image needs room: the build folders go once the programs are out.
if [ -n "${CI:-}" ]; then
  rm -rf target/aarch64-apple-darwin target/x86_64-apple-darwin
fi
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
# hdiutil sizes the image from the folder and can guess too small for a
# large program ("No space left on device" inside the image), so it gets
# twice the app's size and some room.
size_mb=$(( $(du -sm "$app" | cut -f1) * 2 + 32 ))
hdiutil create -volname Sue -srcfolder "$app" -size "${size_mb}m" -ov -format UDZO \
  "$out/Sue-$version-macos-universal.dmg"

# The command line program, for a terminal: --self-check, --version.
codesign --force --sign "${SUE_SIGN_IDENTITY:--}" "$cli/sue-cli"
cp LICENSE README.md "$cli/"
ditto -c -k --keepParent "$cli" "$out/Sue-cli-$version-macos-universal.zip"
ls -l "$out"
