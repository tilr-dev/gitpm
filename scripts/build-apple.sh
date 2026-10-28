#!/bin/bash
# Build the FFI static library for macOS (arm64 + x86_64, lipo universal) and
# generate Swift bindings.
#
#   scripts/build-apple.sh [--release] [OUT_DIR]
#   default OUT_DIR: $CARGO_TARGET_DIR/apple or target/apple
#   --release requires both Apple targets and xcodebuild (no partial artifacts).
#
# Output:
#   OUT_DIR/lib/libgitpm_ffi.a       universal (or host-only, see below)
#   OUT_DIR/swift/Gitpm.swift        Swift API (module names: crates/gitpm-ffi/uniffi.toml)
#   OUT_DIR/include/GitpmFFI.h       C header + module.modulemap (C module GitpmFFI)
#   OUT_DIR/GitpmFFI.xcframework     when xcodebuild is available
#
# A missing rustup target is reported and skipped (install with `rustup target add`);
# the script never installs toolchains itself.
set -euo pipefail
STRICT=false
if [ "${1:-}" = --release ]; then
  STRICT=true
  shift
fi
if [ $# -gt 1 ]; then
  echo "usage: $0 [--release] [OUT_DIR]" >&2
  exit 2
fi
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
TARGET_DIR=${CARGO_TARGET_DIR:-$ROOT/target}
OUT=${1:-$TARGET_DIR/apple}
LIB=libgitpm_ffi.a
CRATE=gitpm-ffi
export MACOSX_DEPLOYMENT_TARGET=${MACOSX_DEPLOYMENT_TARGET:-13.0}
case $OUT in /*) ;; *) OUT=$PWD/$OUT ;; esac
# uniffi-bindgen (library mode) runs `cargo metadata` in the working directory.
cd "$ROOT"

installed=$(rustup target list --installed)
if "$STRICT"; then
  for target in aarch64-apple-darwin x86_64-apple-darwin; do
    if ! grep -qx "$target" <<<"$installed"; then
      echo "Release build requires rustup target $target" >&2
      exit 1
    fi
  done
  if ! command -v xcodebuild >/dev/null 2>&1; then
    echo "Release build requires xcodebuild" >&2
    exit 1
  fi
fi
built=()
missing=()
for target in aarch64-apple-darwin x86_64-apple-darwin; do
  if grep -qx "$target" <<<"$installed"; then
    echo "Building $target"
    cargo build --locked --manifest-path "$ROOT/Cargo.toml" -p "$CRATE" --release --target "$target"
    built+=("$TARGET_DIR/$target/release/$LIB")
  else
    missing+=("$target")
    echo "WARNING: rustup target $target is not installed; skipped (rustup target add $target)" >&2
  fi
done
if [ "${#built[@]}" -eq 0 ]; then
  echo "No Apple target is installed." >&2
  exit 1
fi

rm -rf "$OUT"
mkdir -p "$OUT/lib" "$OUT/swift" "$OUT/include"
if [ "${#built[@]}" -gt 1 ]; then
  lipo -create "${built[@]}" -output "$OUT/lib/$LIB"
else
  cp "${built[0]}" "$OUT/lib/$LIB"
fi
lipo -info "$OUT/lib/$LIB"

# Bindings are read from the library's embedded UniFFI metadata (library mode).
cargo run --locked --manifest-path "$ROOT/Cargo.toml" -p "$CRATE" --features bindgen --bin uniffi-bindgen --quiet -- \
  generate --library "${built[0]}" --language swift --out-dir "$OUT/swift"
mv "$OUT/swift/"*.h "$OUT/include/"
mv "$OUT/swift/"*.modulemap "$OUT/include/module.modulemap"

if command -v xcodebuild >/dev/null 2>&1; then
  xcodebuild -create-xcframework -library "$OUT/lib/$LIB" -headers "$OUT/include" \
    -output "$OUT/GitpmFFI.xcframework" >/dev/null
fi

echo "Library:  $OUT/lib/$LIB"
echo "Swift:    $OUT/swift/Gitpm.swift"
echo "Headers:  $OUT/include"
[ -d "$OUT/GitpmFFI.xcframework" ] && echo "XCFramework: $OUT/GitpmFFI.xcframework"
if [ "${#missing[@]}" -gt 0 ]; then
  echo "Missing targets (not in the library): ${missing[*]}" >&2
fi
