# Publish Apple FFI release artifacts

## Why
The release workflow currently attaches only CLI binaries. Hosts consuming the
Swift FFI must build it from source instead of using an artifact of the release.

## What Changes
- Attach a versioned archive containing the universal macOS FFI xcframework,
  matching Swift bindings, headers and licenses, with a SHA-256 sidecar, to stable
  releases and release candidates.
- Require both Apple Rust targets and xcodebuild for release builds, preserving
  the permissive local build default.
- Document the artifact and verify the workflow with deterministic tool stubs.

## Out of scope
- Host dependency changes, iOS support, signing and publishing to crates.io.

## Impact
The Apple build script, release workflow, release documentation and two FFI/release
requirements change. Build jobs retain read-only repository permissions.
