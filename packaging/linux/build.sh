#!/usr/bin/env bash
# Builds Sue's Linux packages from the release binary: a tarball, a .deb,
# an .rpm (when rpmbuild is there) and an AppImage.
#
#   packaging/linux/build.sh            (builds the release binary first)
#   SKIP_BUILD=1 packaging/linux/build.sh
#
# Needs: cargo, dpkg-deb; rpmbuild for the .rpm; curl or wget the first
# time, to fetch appimagetool into target/tools.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
# A checkout with Windows line endings leaves a carriage return to strip.
version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1 | tr -d '\r')"
arch="$(uname -m)"
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
  cargo build --release -p sue
fi
binary="${CARGO_TARGET_DIR:-$root/target}/release/sue"
strip "$binary" 2>/dev/null || true

# The files every package installs.
stage() {
  local prefix="$1"
  install -Dm755 "$binary" "$prefix/usr/bin/sue"
  install -Dm644 packaging/linux/sue.desktop "$prefix/usr/share/applications/sue.desktop"
  install -Dm644 assets/icons/sue_icon.png "$prefix/usr/share/icons/hicolor/512x512/apps/sue.png"
  install -Dm644 assets/icons/sue_icon.svg "$prefix/usr/share/icons/hicolor/scalable/apps/sue.svg"
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
Depends: libc6, libasound2 | libasound2t64, libxkbcommon0, libwayland-client0, libgtk-3-0, libudev1
Description: Sue, the Singular Unified Engine
 One creative environment: painting and illustration, comics and
 animation, writing and publishing, sound, and a file manager and
 converter, all on the same data.
CONTROL
dpkg-deb --build --root-owner-group "$deb" "$out/Sue-$version-linux-$deb_arch.deb"

# Fedora, openSUSE and others.
if command -v rpmbuild >/dev/null; then
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
Requires: alsa-lib, libxkbcommon, gtk3

%description
One creative environment: painting and illustration, comics and animation,
writing and publishing, sound, and a file manager and converter.

%install
cp -a $rpm/root/. %{buildroot}/

%files
/usr/bin/sue
/usr/share/applications/sue.desktop
/usr/share/icons/hicolor/512x512/apps/sue.png
/usr/share/icons/hicolor/scalable/apps/sue.svg
/usr/share/doc/sue/README.md
%license /usr/share/doc/sue/LICENSE
/usr/share/doc/sue/copyright
SPEC
  rpmbuild --define "_topdir $rpm" --define "_build_id_links none" -bb "$rpm/SPECS/sue.spec"
  find "$rpm/RPMS" -name '*.rpm' -exec cp {} "$out/Sue-$version-linux-$arch.rpm" \;
else
  echo "rpmbuild is not installed: no .rpm this time"
fi

# AppImage: one file that runs on most distributions.
appdir="$work/Sue.AppDir"
stage "$appdir"
ln -sf usr/bin/sue "$appdir/AppRun"
cp packaging/linux/sue.desktop "$appdir/sue.desktop"
cp assets/icons/sue_icon.png "$appdir/sue.png"
tools="$root/target/tools"
mkdir -p "$tools"
appimagetool="$tools/appimagetool-$arch.AppImage"
if [ ! -x "$appimagetool" ]; then
  url="https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-$arch.AppImage"
  if command -v curl >/dev/null; then curl -fsSL -o "$appimagetool" "$url"; else wget -q -O "$appimagetool" "$url"; fi
  chmod +x "$appimagetool"
fi
# No FUSE in containers and CI: run the tool unpacked.
APPIMAGE_EXTRACT_AND_RUN=1 ARCH="$arch" "$appimagetool" "$appdir" "$out/Sue-$version-linux-$arch.AppImage"

ls -l "$out"
