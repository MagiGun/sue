#!/usr/bin/env bash
# Builds Sue for the browser into dist/web: index.html, sue.js (the
# glue wasm-bindgen writes), sue_bg.wasm and the icon. Every path in it
# is relative, so the folder works from any address, such as
# https://magigun.github.io/sue/try/, and from any static host: it needs
# no server of its own and no special headers (Sue uses no wasm threads).
#
#   packaging/web/build.sh
#   SUE_WEB_OUT=/tmp/try packaging/web/build.sh
#   SKIP_BUILD=1 packaging/web/build.sh   (reuses the last .wasm)
#
# Needs: cargo with the wasm32-unknown-unknown target (rust-toolchain.toml
# lists it), wasm-bindgen of the version in Cargo.lock
# (cargo install wasm-bindgen-cli --version <it> --locked), and wasm-opt
# (from binaryen) to make the .wasm smaller; without wasm-opt the build
# still works, only larger.
#
# Try it: python3 -m http.server -d dist/web, then open
# http://localhost:8000/.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
out="${SUE_WEB_OUT:-$root/dist/web}"
target_dir="${CARGO_TARGET_DIR:-$root/target}"
wasm="$target_dir/wasm32-unknown-unknown/web/sue.wasm"

# wasm-bindgen must be the version the program was built against.
wanted="$(sed -n '/^name = "wasm-bindgen"$/{n;s/^version = "\(.*\)"/\1/p;}' Cargo.lock | head -n 1 | tr -d '\r')"
if ! command -v wasm-bindgen >/dev/null; then
  echo "wasm-bindgen is missing: cargo install wasm-bindgen-cli --version $wanted --locked" >&2
  exit 1
fi
have="$(wasm-bindgen --version | awk '{print $2}')"
if [ "$have" != "$wanted" ]; then
  echo "wasm-bindgen $have is installed, Cargo.lock wants $wanted:" >&2
  echo "  cargo install wasm-bindgen-cli --version $wanted --locked" >&2
  exit 1
fi

if [ -z "${SKIP_BUILD:-}" ]; then
  cargo build --profile web --target wasm32-unknown-unknown -p sue --bin sue
fi

rm -rf "$out"
mkdir -p "$out"
wasm-bindgen --target web --no-typescript --out-dir "$out" --out-name sue "$wasm"

if command -v wasm-opt >/dev/null; then
  # The features Rust's wasm32-unknown-unknown target uses, so wasm-opt
  # reads and keeps them.
  wasm-opt -Os --strip-debug --strip-producers \
    --enable-bulk-memory --enable-mutable-globals --enable-sign-ext \
    --enable-nontrapping-float-to-int --enable-reference-types --enable-multivalue \
    -o "$out/sue_bg.opt.wasm" "$out/sue_bg.wasm"
  mv "$out/sue_bg.opt.wasm" "$out/sue_bg.wasm"
else
  echo "wasm-opt is missing (binaryen): the .wasm is left unoptimized" >&2
fi

cp packaging/web/index.html "$out/index.html"
cp assets/icons/sue_icon_512.png "$out/icon.png"
# GitHub Pages runs Jekyll over branches unless told not to.
touch "$out/.nojekyll"

size() { wc -c < "$1" | tr -d ' '; }
echo "Wrote $out: sue_bg.wasm is $(( $(size "$out/sue_bg.wasm") / 1024 )) KiB"
