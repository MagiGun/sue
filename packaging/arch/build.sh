#!/usr/bin/env bash
# Builds Sue's Arch Linux package (.pkg.tar.zst) from the Linux tarball in
# dist/, and writes the AUR's PKGBUILD for this release next to it, its
# checksum filled in.
#
#   packaging/linux/build.sh && packaging/arch/build.sh
#
# Runs on Arch (or in an archlinux container) with base-devel. makepkg will
# not build as root, so as root it builds as a plain `builder` user.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
# A checkout with Windows line endings leaves a carriage return to strip.
version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1 | tr -d '\r')"
arch="$(uname -m)"
out="$root/dist"
tarball="$out/Sue-$version-linux-$arch.tar.gz"
if [ ! -f "$tarball" ]; then
  echo "no $tarball: run packaging/linux/build.sh first" >&2
  exit 1
fi
work="$root/target/arch-package"
rm -rf "$work"
mkdir -p "$work"
sum="$(sha256sum "$tarball" | cut -d ' ' -f 1)"
cp "$tarball" "$work/"
sed -e "s/^pkgver=.*/pkgver=$version/" \
    -e "s|^source_$arch=.*|source_$arch=(\"$(basename "$tarball")\")|" \
    -e "s/^sha256sums_$arch=.*/sha256sums_$arch=('$sum')/" \
    packaging/arch/PKGBUILD > "$work/PKGBUILD"

if [ "$(id -u)" = 0 ]; then
  id builder >/dev/null 2>&1 || useradd -m builder
  chown -R builder "$work"
  su builder -c "cd '$work' && makepkg --force --nodeps --noconfirm"
else
  (cd "$work" && makepkg --force --nodeps --noconfirm)
fi
cp "$work"/sue-bin-*.pkg.tar.zst "$out/"

# The AUR's PKGBUILD for this release: the download and its checksum.
sed -e "s/^pkgver=.*/pkgver=$version/" \
    -e "s/^sha256sums_$arch=.*/sha256sums_$arch=('$sum')/" \
    packaging/arch/PKGBUILD > "$out/PKGBUILD"
ls -l "$out"/sue-bin-*.pkg.tar.zst "$out/PKGBUILD"
