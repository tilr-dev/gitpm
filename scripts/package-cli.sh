#!/bin/bash
# Package a built `gitpm` binary as a GitHub release asset (used by release.yml).
#
#   scripts/package-cli.sh TAG TARGET [OUT_DIR]     default OUT_DIR: target/dist
#
# Expects the binary at $CARGO_TARGET_DIR/TARGET/release/gitpm (target/ by default), i.e.
# `cargo build --release --locked -p gitpm-cli --target TARGET`.
#
# Output:
#   OUT_DIR/gitpm-cli-TAG-TARGET.tar.gz          gitpm-cli-TAG-TARGET/{gitpm,LICENSE-*,README.md}
#   OUT_DIR/gitpm-cli-TAG-TARGET.tar.gz.sha256   "<sha256>  <archive name>" (sha256sum -c)
#
# The archive name and its top-level folder are what [package.metadata.binstall] in
# crates/gitpm-cli/Cargo.toml expects (pkg-url, bin-dir); change them together.
set -euo pipefail
if [ $# -lt 2 ] || [ $# -gt 3 ]; then
  echo "usage: $0 TAG TARGET [OUT_DIR]" >&2
  exit 2
fi
TAG=$1
TARGET=$2
ROOT=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd -P)
TARGET_DIR=${CARGO_TARGET_DIR:-$ROOT/target}
OUT=${3:-$TARGET_DIR/dist}
case $OUT in /*) ;; *) OUT=$PWD/$OUT ;; esac
BIN=$TARGET_DIR/$TARGET/release/gitpm
NAME=gitpm-cli-$TAG-$TARGET

[ -x "$BIN" ] || { echo "missing $BIN (cargo build --release --locked -p gitpm-cli --target $TARGET)" >&2; exit 1; }

STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT
mkdir -p "$STAGE/$NAME" "$OUT"
cp "$BIN" "$STAGE/$NAME/gitpm"
cp "$ROOT/crates/gitpm-cli/LICENSE-APACHE" "$ROOT/crates/gitpm-cli/LICENSE-MIT" \
  "$ROOT/crates/gitpm-cli/README.md" "$STAGE/$NAME/"

# COPYFILE_DISABLE: no AppleDouble (._*) entries from macOS tar.
COPYFILE_DISABLE=1 tar -czf "$OUT/$NAME.tar.gz" -C "$STAGE" "$NAME"
# shasum ships with macOS and the Ubuntu runners (perl); sha256sum does not ship with macOS.
(cd "$OUT" && shasum -a 256 "$NAME.tar.gz" >"$NAME.tar.gz.sha256")
echo "$OUT/$NAME.tar.gz"
