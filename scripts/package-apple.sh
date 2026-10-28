#!/bin/bash
# Package build-apple.sh --release output as a GitHub release asset.
# Usage: scripts/package-apple.sh TAG [APPLE_DIR] [OUT_DIR]
set -euo pipefail
if [ $# -lt 1 ] || [ $# -gt 3 ]; then
  echo "usage: $0 TAG [APPLE_DIR] [OUT_DIR]" >&2
  exit 2
fi
ROOT=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd -P)
TARGET_DIR=${CARGO_TARGET_DIR:-$ROOT/target}
APPLE=${2:-$TARGET_DIR/apple}
OUT=${3:-$TARGET_DIR/dist}
case $OUT in /*) ;; *) OUT=$PWD/$OUT ;; esac
NAME=gitpm-ffi-$1-universal-apple-darwin
for entry in GitpmFFI.xcframework swift/Gitpm.swift include/GitpmFFI.h include/module.modulemap; do
  [ -e "$APPLE/$entry" ] || { echo "missing $APPLE/$entry (run scripts/build-apple.sh --release)" >&2; exit 1; }
done
STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT
mkdir -p "$STAGE/$NAME" "$OUT"
cp -R "$APPLE/GitpmFFI.xcframework" "$APPLE/swift" "$APPLE/include" "$STAGE/$NAME/"
cp "$ROOT/LICENSE-APACHE" "$ROOT/LICENSE-MIT" "$ROOT/README.md" "$STAGE/$NAME/"
COPYFILE_DISABLE=1 tar -czf "$OUT/$NAME.tar.gz" -C "$STAGE" "$NAME"
(cd "$OUT" && shasum -a 256 "$NAME.tar.gz" >"$NAME.tar.gz.sha256")
echo "$OUT/$NAME.tar.gz"
