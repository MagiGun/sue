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
# The release's version (SUE_VERSION, set by the Release workflow from the
# version typed or the tag pushed), else Cargo.toml's. A checkout with
# Windows line endings leaves a carriage return to strip.
version="${SUE_VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1 | tr -d '\r')}"
version="${version#v}"
export SUE_VERSION="$version"
# Another CPU's package (SUE_ARCH=aarch64): it only repackages the release
# tarball, so any Arch machine can make it.
arch="${SUE_ARCH:-$(uname -m)}"
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

# makepkg builds for the CPU its configuration names.
sed "s/^CARCH=.*/CARCH=\"$arch\"/" /etc/makepkg.conf > "$work/makepkg.conf"
if [ "$(id -u)" = 0 ]; then
  id builder >/dev/null 2>&1 || useradd -m builder
  chown -R builder "$work"
  su builder -c "cd '$work' && makepkg --config '$work/makepkg.conf' --force --nodeps --noconfirm"
else
  (cd "$work" && makepkg --config "$work/makepkg.conf" --force --nodeps --noconfirm)
fi
cp "$work"/sue-bin-*-"$arch".pkg.tar.zst "$out/Sue-$version-linux-$arch.pkg.tar.zst"

# The AUR's PKGBUILD for this release: the downloads and their checksums.
# A second CPU's run adds its checksum to the first one's.
template=packaging/arch/PKGBUILD
if [ -f "$out/PKGBUILD" ] && grep -q "^pkgver=$version\$" "$out/PKGBUILD"; then
  template="$out/PKGBUILD"
fi
sed -e "s/^pkgver=.*/pkgver=$version/" \
    -e "s/^sha256sums_$arch=.*/sha256sums_$arch=('$sum')/" \
    "$template" > "$work/PKGBUILD.release"
cp "$work/PKGBUILD.release" "$out/PKGBUILD"
ls -l "$out/Sue-$version-linux-$arch.pkg.tar.zst" "$out/PKGBUILD"
