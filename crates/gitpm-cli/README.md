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

A package is a Git repository with `plugin.json` (`{"id": "acme/greeter", "version": "0.1.0"}`)
and a `dist/` folder, released with a tag (`acme/greeter@0.1.0`):

```sh
gitpm install ./acme/greeter         # latest release tag → ./gitpm-workspace
gitpm list                           # installed state (JSON)
gitpm update acme/greeter            # newest release tag
gitpm update all --dry-run           # preview a joint compatible upgrade
gitpm update all                     # apply all selected versions atomically
gitpm rollback acme/greeter          # back to the previous version

echo './acme/greeter' > sources.txt
gitpm --catalog ./catalog catalog-update     # local catalog from release tags
gitpm --catalog ./catalog check              # installed versions vs the catalog
gitpm --catalog ./catalog --root ./other install-catalog acme/greeter   # hashes verified

gitpm init acme/hello --into ./hello     # new package repository ./hello, ID acme/hello
gitpm install ./hello
gitpm fork acme/hello --ns me --into ./me-hello   # clone the recorded source as me/hello
gitpm commit ./me-hello 0.2.0            # explicitly release a new version
gitpm install ./me-hello me/hello        # explicitly install the desired fork
gitpm replace-check acme/hello --with me/hello # report issues and missing installs as JSON
gitpm replace-resolve acme/hello --with me/hello # explicitly install missing dependencies
gitpm replace acme/hello --with me/hello # requires a consistent workspace
gitpm replace acme/hello --with acme/hello # return to the installed, enabled original
gitpm fork tilr/store tilr/theme --ns me --into ./me-packages   # several packages, one repo

git init -q tilr-packages
gitpm init tilr/store --into ./tilr-packages   # multi-package repository (gitpm.json, packages/*)
gitpm install ./tilr-packages tilr/store   # one package of it; packages share one clone
gitpm commit ./tilr-packages 0.2.0 --package tilr/store   # tag tilr/store@0.2.0
```

`init` and `fork` take an explicit `NAMESPACE/NAME` (for `fork`: `--ns
NAMESPACE`, the name stays) and an explicit repository `--into DIR`; nothing is derived
from an ID and there is no default namespace. `init --into DIR`: a new `DIR` becomes a
single-package repository, a repository with `gitpm.json` (or without one and without a
root package) gets the folder `PREFIX/NAME` for the first `PREFIX/*` pattern of
`gitpm.json` (`packages/NAME` by default). `fork ID... --ns NAMESPACE --into DIR`:
`DIR` must be new. Git clones the recorded source at installed revisions with history
and submodule links, ignoring local installation edits. All packages are renamed
into the new namespace; all repository files and the package layout are preserved. Initialize submodules with `git -C DIR submodule update --init`;
dependencies between the forks are rewritten. Fork preserves inherited tags, creates
no release tag and leaves workspace state, installed packages, replacement bindings
and dist unchanged. Use `commit`, `install` and `replace` explicitly; an existing
release tag still conflicts with `commit`. The removed fork options `--replace`,
`--version` and `--force` are refused before any changes.

Everything works on local folders; nothing is built, executed, pushed or uploaded.
Settings come from flags (`--root`, `--catalog`, `--sources`), then `GITPM_ROOT`,
`GITPM_CATALOG` and `GITPM_SOURCES`; there is no config file. Run `gitpm --help` for all commands, and see the
[repository README](https://github.com/tilr-dev/gitpm#cli-quickstart) for a walkthrough.

License: MIT OR Apache-2.0.

Replacement checks cover every active package. `replace` never installs dependencies,
never prompts, and accepts neither `--force` nor `--back`. `replace-resolve` installs
only missing catalog releases; it does not update or enable installed packages or apply
the proposed binding. Fix incompatible versions explicitly before replacing.
