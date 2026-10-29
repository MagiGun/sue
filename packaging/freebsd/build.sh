#!/bin/sh
# Builds Sue for FreeBSD: a tarball laid out for /usr/local.
#
#   packaging/freebsd/build.sh
#
# Runs on FreeBSD with Rust (rustup), pkgconf and alsa-lib
# (`pkg install pkgconf alsa-lib`). To run Sue there, install alsa-lib,
# libxkbcommon and wayland or the X11 libraries, and Mesa or a Vulkan
# driver.
set -eu

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
# The release's version (SUE_VERSION, set by the Release workflow), else
# Cargo.toml's.
version="${SUE_VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1 | tr -d '\r')}"
version="${version#v}"
export SUE_VERSION="$version"
arch="$(uname -m)"
case "$arch" in
  amd64) arch=x86_64 ;;
  arm64) arch=aarch64 ;;
esac
out="$root/dist"
work="$root/target/freebsd-package/sue-$version"
rm -rf "$root/target/freebsd-package"
mkdir -p "$out" "$work/bin" "$work/share/applications" \
  "$work/share/icons/hicolor/512x512/apps" "$work/share/icons/hicolor/scalable/apps" \
  "$work/share/doc/sue"

if [ -z "${SKIP_BUILD:-}" ]; then
  cargo build --release -p sue --bin sue
fi
install -m 755 "${CARGO_TARGET_DIR:-$root/target}/release/sue" "$work/bin/sue"
strip "$work/bin/sue" 2>/dev/null || true
install -m 644 packaging/linux/io.github.MagiGun.Sue.desktop "$work/share/applications/io.github.MagiGun.Sue.desktop"
install -m 644 assets/icons/sue_icon_512.png "$work/share/icons/hicolor/512x512/apps/io.github.MagiGun.Sue.png"
install -m 644 assets/icons/sue_icon.svg "$work/share/icons/hicolor/scalable/apps/io.github.MagiGun.Sue.svg"
install -m 644 README.md LICENSE "$work/share/doc/sue/"

tar -C "$root/target/freebsd-package" -czf "$out/Sue-$version-freebsd-$arch.tar.gz" "sue-$version"
ls -l "$out/Sue-$version-freebsd-$arch.tar.gz"
