#!/usr/bin/env bash
# release.sh — build shh release binaries for macOS, Linux, and Windows from a Mac.
#
# Prereqs (one-time):
#   brew install zig
#   cargo install cargo-zigbuild
#   rustup target add aarch64-apple-darwin x86_64-apple-darwin \
#     aarch64-unknown-linux-musl x86_64-unknown-linux-musl x86_64-pc-windows-gnu
#
# Usage:
#   scripts/release.sh

set -euo pipefail

NAME="shh"
DIST="dist"

# Version: highest semver-sorted git tag (leading "v" or "v." stripped),
# falling back to Cargo.toml if the repo has no tags.
TAG="$(git tag --sort=-v:refname 2>/dev/null | head -n1 || true)"
if [[ -n "$TAG" ]]; then
  VERSION="${TAG#v}"
  VERSION="${VERSION#.}"
else
  VERSION="$(awk -F\" '/^version *=/ {print $2; exit}' Cargo.toml)"
fi

TARGETS=(
  "aarch64-apple-darwin"        # Apple Silicon
  "x86_64-apple-darwin"         # Intel Mac
  "aarch64-unknown-linux-musl"  # ARM64 Linux (static)
  "x86_64-unknown-linux-musl"   # x86_64 Linux (static)
  "x86_64-pc-windows-gnu"       # Windows 64-bit (GNU ABI)
)

need() { command -v "$1" >/dev/null 2>&1 || { echo "missing: $1" >&2; exit 1; }; }

need cargo
need rustup
need tar
need zip
need shasum

friendly_suffix() {
  case "$1" in
    aarch64-apple-darwin)        echo "macos-arm64" ;;
    x86_64-apple-darwin)         echo "macos-x64" ;;
    universal-apple-darwin)      echo "macos-universal" ;;
    aarch64-unknown-linux-musl)  echo "linux-arm64" ;;
    x86_64-unknown-linux-musl)   echo "linux-x64" ;;
    x86_64-pc-windows-gnu)       echo "windows-x64" ;;
    *) echo "$1" ;;
  esac
}

if ! command -v zig >/dev/null 2>&1; then
  echo "zig not found — install with: brew install zig" >&2
  exit 1
fi
if ! command -v cargo-zigbuild >/dev/null 2>&1; then
  echo "cargo-zigbuild not found — install with: cargo install cargo-zigbuild" >&2
  exit 1
fi

installed_targets="$(rustup target list --installed)"
for t in "${TARGETS[@]}"; do
  if ! grep -q "^${t}\$" <<<"$installed_targets"; then
    echo "==> rustup target add $t"
    rustup target add "$t"
  fi
done

rm -rf "$DIST"
mkdir -p "$DIST"

build_one() {
  local target=$1
  echo
  echo "===> $target"

  case "$target" in
    *-apple-darwin)
      cargo build --release --target "$target"
      ;;
    *)
      cargo zigbuild --release --target "$target"
      ;;
  esac

  local stem="${NAME}-v${VERSION}-$(friendly_suffix "$target")"
  if [[ "$target" == *windows* ]]; then
    local bin="${NAME}.exe"
    local archive="${DIST}/${stem}.zip"
    (cd "target/${target}/release" && zip -q "${OLDPWD}/${archive}" "$bin")
  else
    local bin="${NAME}"
    local archive="${DIST}/${stem}.tar.gz"
    tar -czf "$archive" -C "target/${target}/release" "$bin"
  fi
  echo "    $archive"
}

for t in "${TARGETS[@]}"; do
  build_one "$t"
done

# Universal macOS binary — runs on both Apple Silicon and Intel.
arm_bin="target/aarch64-apple-darwin/release/$NAME"
x86_bin="target/x86_64-apple-darwin/release/$NAME"
if [[ -f "$arm_bin" && -f "$x86_bin" ]]; then
  echo
  echo "===> universal-apple-darwin (lipo)"
  uni_dir="target/universal-apple-darwin/release"
  mkdir -p "$uni_dir"
  lipo -create "$arm_bin" "$x86_bin" -output "$uni_dir/$NAME"
  archive="${DIST}/${NAME}-v${VERSION}-$(friendly_suffix universal-apple-darwin).tar.gz"
  tar -czf "$archive" -C "$uni_dir" "$NAME"
  echo "    $archive"
fi

echo
echo "===> checksums"
(cd "$DIST" && shasum -a 256 *.tar.gz *.zip > SHA256SUMS)
cat "$DIST/SHA256SUMS"

echo
echo "==> done. artifacts in ${DIST}/"
ls -lh "$DIST"
