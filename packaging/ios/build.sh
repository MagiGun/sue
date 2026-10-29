#!/usr/bin/env bash
# Builds Sue for iPhone and iPad: Sue.app for devices, packed as an
# unsigned .ipa, and a build for the simulator.
#
#   packaging/ios/build.sh
#
# Needs macOS with Xcode and the Rust targets aarch64-apple-ios and
# aarch64-apple-ios-sim. To install on a device, sign the .ipa with your
# own team (Xcode, or a tool such as `codesign` and a provisioning
# profile); Files shows Sue's folder because of UIFileSharingEnabled.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
# A checkout with Windows line endings leaves a carriage return to strip.
version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1 | tr -d '\r')"
out="$root/dist"
work="$root/target/ios-package"
rm -rf "$work"
mkdir -p "$out" "$work"

bundle() {
  local target="$1" app="$2"
  cargo build --release -p sue --target "$target"
  mkdir -p "$app"
  cp "target/$target/release/sue" "$app/sue"
  sed "s/@VERSION@/$version/g" packaging/ios/Info.plist > "$app/Info.plist"
  cp LICENSE "$app/LICENSE"
  sips -z 120 120 assets/icons/sue_icon.png --out "$app/AppIcon60x60@2x.png" >/dev/null
  sips -z 152 152 assets/icons/sue_icon.png --out "$app/AppIcon76x76@2x~ipad.png" >/dev/null
}

bundle aarch64-apple-ios "$work/device/Payload/Sue.app"
(cd "$work/device" && zip -qr "$out/Sue-$version-ios-unsigned.ipa" Payload)

bundle aarch64-apple-ios-sim "$work/simulator/Sue.app"
codesign --force --sign - "$work/simulator/Sue.app"
(cd "$work/simulator" && zip -qr "$out/Sue-$version-ios-simulator.zip" Sue.app)
ls -l "$out"
