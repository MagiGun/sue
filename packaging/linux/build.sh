#!/usr/bin/env bash
# Builds Sue's Linux packages from the release binary: a tarball, a .deb,
# an .rpm (when rpmbuild is there) and an AppImage.
#
#   packaging/linux/build.sh            (builds the release binary first)
#   SKIP_BUILD=1 packaging/linux/build.sh
#   SUE_TARGET=riscv64gc-unknown-linux-gnu SUE_ARCH=riscv64 STRIP=riscv64-linux-gnu-strip SKIP_APPIMAGE=1 packaging/linux/build.sh
#
# Needs: cargo, dpkg-deb; rpmbuild for the .rpm; curl or wget the first
# time, to fetch appimagetool into target/tools.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
# The release's version (SUE_VERSION, set by the Release workflow from the
# version typed or the tag pushed), else Cargo.toml's. A checkout with
# Windows line endings leaves a carriage return to strip.
version="${SUE_VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1 | tr -d '\r')}"
version="${version#v}"
export SUE_VERSION="$version"
# Another CPU when cross-building (SUE_TARGET is the Rust target).
arch="${SUE_ARCH:-$(uname -m)}"
case "$arch" in
  x86_64) deb_arch=amd64 ;;
  aarch64) deb_arch=arm64 ;;
  *) deb_arch="$arch" ;;
esac
out="$root/dist"
work="$root/target/linux-package"
rm -rf "$work"
mkdir -p "$out" "$work"

if [ -z "${SKIP_BUILD:-}" ]; then
  cargo build --release -p sue --bin sue ${SUE_TARGET:+--target "$SUE_TARGET"}
fi
binary="${CARGO_TARGET_DIR:-$root/target}/${SUE_TARGET:+$SUE_TARGET/}release/sue"
"${STRIP:-strip}" "$binary" 2>/dev/null || true

# The files every package installs.
stage() {
  local prefix="$1"
  install -Dm755 "$binary" "$prefix/usr/bin/sue"
  install -Dm644 packaging/linux/io.github.MagiGun.Sue.desktop "$prefix/usr/share/applications/io.github.MagiGun.Sue.desktop"
  install -Dm644 assets/icons/sue_icon_512.png "$prefix/usr/share/icons/hicolor/512x512/apps/io.github.MagiGun.Sue.png"
  install -Dm644 assets/icons/sue_icon.svg "$prefix/usr/share/icons/hicolor/scalable/apps/io.github.MagiGun.Sue.svg"
  install -Dm644 README.md "$prefix/usr/share/doc/sue/README.md"
  install -Dm644 LICENSE "$prefix/usr/share/doc/sue/LICENSE"
  install -Dm644 LICENSE "$prefix/usr/share/doc/sue/copyright"
}

# Tarball.
tarball="$work/sue-$version"
stage "$tarball"
tar -C "$work" -czf "$out/Sue-$version-linux-$arch.tar.gz" "sue-$version"

# Debian and Ubuntu.
deb="$work/deb"
stage "$deb"
mkdir -p "$deb/DEBIAN"
cat > "$deb/DEBIAN/control" <<CONTROL
Package: sue
Version: $version
Architecture: $deb_arch
Maintainer: MagiGun (https://github.com/MagiGun/sue)
Section: graphics
Priority: optional
Depends: libc6, libasound2 | libasound2t64, libxkbcommon0, libwayland-client0
Recommends: libvulkan1, libegl1, xdg-desktop-portal
Description: Sue, the Singular Unified Engine
 One creative environment: painting and illustration, comics and
 animation, writing and publishing, sound, and a file manager and
 converter, all on the same data.
CONTROL
dpkg-deb --build --root-owner-group "$deb" "$out/Sue-$version-linux-$deb_arch.deb"

# Fedora, openSUSE and others.
if command -v rpmbuild >/dev/null && [ -z "${SKIP_RPM:-}" ]; then
  rpm="$work/rpm"
  mkdir -p "$rpm"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}
  stage "$rpm/root"
  cat > "$rpm/SPECS/sue.spec" <<SPEC
Name: sue
Version: $version
Release: 1
Summary: Sue, the Singular Unified Engine
License: LicenseRef-Sayn
URL: https://github.com/MagiGun/sue
Requires: alsa-lib, libxkbcommon, libwayland-client
Recommends: vulkan-loader, mesa-libEGL, xdg-desktop-portal

%description
One creative environment: painting and illustration, comics and animation,
writing and publishing, sound, and a file manager and converter.

%install
cp -a $rpm/root/. %{buildroot}/

%files
/usr/bin/sue
/usr/share/applications/io.github.MagiGun.Sue.desktop
/usr/share/icons/hicolor/512x512/apps/io.github.MagiGun.Sue.png
/usr/share/icons/hicolor/scalable/apps/io.github.MagiGun.Sue.svg
/usr/share/doc/sue/README.md
%license /usr/share/doc/sue/LICENSE
/usr/share/doc/sue/copyright
SPEC
  rpmbuild --define "_topdir $rpm" --define "_build_id_links none" --target "$arch" -bb "$rpm/SPECS/sue.spec"
  find "$rpm/RPMS" -name '*.rpm' -exec cp {} "$out/Sue-$version-linux-$arch.rpm" \;
else
  echo "rpmbuild is not installed: no .rpm this time"
fi

# AppImage: one file that runs on most distributions, with a .zsync beside
# it so AppImage updaters fetch only what changed.
if [ -n "${SKIP_APPIMAGE:-}" ]; then
  ls -l "$out"
  exit 0
fi
appdir="$work/Sue.AppDir"
stage "$appdir"
ln -sf usr/bin/sue "$appdir/AppRun"
cp packaging/linux/io.github.MagiGun.Sue.desktop "$appdir/io.github.MagiGun.Sue.desktop"
cp assets/icons/sue_icon_512.png "$appdir/io.github.MagiGun.Sue.png"
tools="$root/target/tools"
mkdir -p "$tools"
appimagetool="$tools/appimagetool-$arch.AppImage"
if [ ! -x "$appimagetool" ]; then
  url="https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-$arch.AppImage"
  if command -v curl >/dev/null; then curl -fsSL -o "$appimagetool" "$url"; else wget -q -O "$appimagetool" "$url"; fi
  chmod +x "$appimagetool"
fi
# No FUSE in containers and CI: run the tool unpacked.
(
  cd "$out"
  APPIMAGE_EXTRACT_AND_RUN=1 ARCH="$arch" "$appimagetool" \
    -u "gh-releases-zsync|MagiGun|sue|latest|Sue-*-linux-$arch.AppImage.zsync" \
    "$appdir" "Sue-$version-linux-$arch.AppImage"
)

ls -l "$out"
