#!/usr/bin/env bash
# Builds Sue's Flatpak bundle (one .flatpak file people install with
# `flatpak install --user Sue-*.flatpak`) from the Linux tarball in dist/.
#
#   packaging/linux/build.sh && packaging/flatpak/build.sh
#
# Needs flatpak and flatpak-builder; the freedesktop 24.08 runtime comes
# from Flathub the first time.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
# The release's version (SUE_VERSION, set by the Release workflow from the
# version typed or the tag pushed), else Cargo.toml's. A checkout with
# Windows line endings leaves a carriage return to strip.
version="${SUE_VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1 | tr -d '\r')}"
version="${version#v}"
export SUE_VERSION="$version"
arch="$(uname -m)"
out="$root/dist"
tarball="$out/Sue-$version-linux-$arch.tar.gz"
if [ ! -f "$tarball" ]; then
  echo "no $tarball: run packaging/linux/build.sh first" >&2
  exit 1
fi
work="$root/target/flatpak-package"
rm -rf "$work"
mkdir -p "$work"
cp packaging/flatpak/io.github.MagiGun.Sue.desktop "$work/"
sed -e "s/@VERSION@/$version/" -e "s/@DATE@/$(date -u +%Y-%m-%d)/" \
  packaging/flatpak/io.github.MagiGun.Sue.metainfo.xml > "$work/io.github.MagiGun.Sue.metainfo.xml"
cp "$tarball" "$work/sue.tar.gz"
sed "s|path: TARBALL|path: sue.tar.gz|" packaging/flatpak/io.github.MagiGun.Sue.yml > "$work/io.github.MagiGun.Sue.yml"

flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
cd "$work"
flatpak-builder --user --install-deps-from=flathub --force-clean --disable-rofiles-fuse \
  --repo=repo build io.github.MagiGun.Sue.yml
flatpak build-bundle repo "$out/Sue-$version-linux-$arch.flatpak" io.github.MagiGun.Sue \
  --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo
ls -l "$out/Sue-$version-linux-$arch.flatpak"
