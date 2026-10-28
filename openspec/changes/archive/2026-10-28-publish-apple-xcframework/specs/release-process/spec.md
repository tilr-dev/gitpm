## MODIFIED Requirements

### Requirement: Prebuilt binaries
Each stable release and RC prerelease SHALL attach CLI archives for both Apple and Linux architectures and a universal macOS FFI archive with matching Swift bindings, each with a SHA-256 sidecar. Native CLI builds MUST check their version against the tag. Apple builds MUST use strict release mode. Uploads MUST verify checksums and require successful CLI and Apple builds. Build jobs SHALL be read-only; only the asset upload job SHALL receive release asset write permission.

#### Scenario: CLI archive contents
- **WHEN** a release builds CLI assets
- **THEN** it attaches `gitpm-cli-vX.Y.Z-TARGET.tar.gz` and `.sha256` for `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu` (glibc 2.35+), holding the `gitpm` binary, licenses and README
- **AND** names match cargo-binstall metadata

#### Scenario: cargo binstall
- **WHEN** a user runs `cargo binstall gitpm-cli`
- **THEN** the prebuilt archive of their target is downloaded from the GitHub Release

#### Scenario: Stable and candidate Apple assets
- **WHEN** a stable release or RC prerelease is built successfully
- **THEN** the matching tagged Apple FFI archive and SHA-256 sidecar are uploaded alongside the CLI archives
- **AND** the archive contains the xcframework and Swift bindings from that same build
- **AND** its name is `gitpm-ffi-TAG-universal-apple-darwin.tar.gz`, with the full tag including `v` and any RC suffix
- **AND** its top-level folder matches the archive basename and contains `GitpmFFI.xcframework` (universal macOS `libgitpm_ffi`), `swift/Gitpm.swift`, `include/` headers and module map, licenses and README

#### Scenario: Apple build failure
- **WHEN** the Apple release build fails, including for a missing target or xcodebuild
- **THEN** the asset upload job does not run
