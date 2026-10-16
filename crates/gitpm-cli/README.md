# gitpm-cli

Command-line client for [gitpm](https://crates.io/crates/gitpm), a package manager where
packages are folders in Git repositories: releases are tags, installs copy a ready
`dist/` folder, and a catalog is a tree of static JSON files.

## Install

The crate installs a `gitpm` binary:

```sh
cargo install gitpm-cli              # build from source (Rust 1.92+)
cargo binstall gitpm-cli             # prebuilt binary, via cargo-binstall
```

Prebuilt binaries for macOS (arm64, x86_64) and Linux (x86_64, aarch64; glibc 2.35+) are
attached to each [GitHub release](https://github.com/tilr-dev/gitpm/releases) as
`gitpm-cli-vX.Y.Z-TARGET.tar.gz` with a `.sha256` next to it; the archive holds the `gitpm`
binary, the licenses and the README.

## Usage

A package is a Git repository with `plugin.json` (`{"id": "hello", "version": "0.1.0"}`)
and a `dist/` folder, released with a tag (`v0.1.0`):

```sh
gitpm install ./hello                # latest release tag → ./gitpm-workspace
gitpm list                           # installed state (JSON)
gitpm update hello                   # newest release tag
gitpm rollback hello                 # back to the previous version

echo './hello' > sources.txt
gitpm --catalog ./catalog catalog-update     # local catalog from release tags
gitpm --catalog ./catalog check              # installed versions vs the catalog
gitpm --catalog ./catalog --root ./other install-catalog hello   # hashes verified

gitpm create dan/hello               # new plugin repository ./dan/hello, ID dan/hello
gitpm install ./dan/hello
gitpm fork dan/hello me/hello        # copy installed dan/hello to ./me/hello as me/hello
```

`create`/`init` and `fork` take an explicit `NAMESPACE/NAME`: the folder mirrors it
(`./NAMESPACE/NAME`), there is no default namespace, and `fork` keeps the plugin name.

Everything works on local folders; nothing is built, executed, pushed or uploaded.
Settings come from flags (`--root`, `--catalog`, `--sources`), then `GITPM_ROOT`,
`GITPM_CATALOG` and `GITPM_SOURCES`; there is no config file. Run `gitpm --help` for all commands, and see the
[repository README](https://github.com/tilr-dev/gitpm#cli-quickstart) for a walkthrough.

License: MIT OR Apache-2.0.
