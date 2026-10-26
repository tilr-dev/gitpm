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

A package is a Git repository with `plugin.json` (`{"id": "d135a7cf-1c63-549a-93cb-8f89656b2ced", "name": "fixture", "version": "0.1.0"}`)
and a `dist/` folder, released with a tag (`v0.1.0`, shared by every package in the repository).
UUIDs below are examples; use the UUIDs printed by your `init` and `fork` runs:

```sh
gitpm install ./acme/greeter         # latest release tag → ./gitpm-workspace
gitpm list                           # installed state (JSON)
gitpm update d135a7cf-1c63-549a-93cb-8f89656b2ced            # newest release tag
gitpm update all --dry-run           # preview a joint compatible upgrade
gitpm update all                     # apply all selected versions atomically
gitpm rollback d135a7cf-1c63-549a-93cb-8f89656b2ced          # back to the previous version

echo './acme/greeter' > sources.txt
gitpm --catalog ./catalog catalog-update     # local catalog from release tags
gitpm --catalog ./catalog check              # installed versions vs the catalog
gitpm --catalog ./catalog --root ./other install-catalog d135a7cf-1c63-549a-93cb-8f89656b2ced   # hashes verified

gitpm init hello --into ./hello     # new package repository; prints its generated UUID
gitpm install ./hello
gitpm fork 7e6e8392-7be8-55ae-bd03-2b430b1c5d59 --into ./me-hello   # clone the recorded source; prints a fresh UUID
gitpm commit ./me-hello 0.2.0            # explicitly release a new version
gitpm install ./me-hello ce6c8a3b-b2f8-5391-8ef8-4cbb7873f5d4        # explicitly install the desired fork
gitpm replace-check 7e6e8392-7be8-55ae-bd03-2b430b1c5d59 --with ce6c8a3b-b2f8-5391-8ef8-4cbb7873f5d4 # report issues and missing installs as JSON
gitpm replace-resolve 7e6e8392-7be8-55ae-bd03-2b430b1c5d59 --with ce6c8a3b-b2f8-5391-8ef8-4cbb7873f5d4 # explicitly install missing dependencies
gitpm replace 7e6e8392-7be8-55ae-bd03-2b430b1c5d59 --with ce6c8a3b-b2f8-5391-8ef8-4cbb7873f5d4 # requires compatible provider dependencies and consumers
gitpm replace 7e6e8392-7be8-55ae-bd03-2b430b1c5d59 --with 7e6e8392-7be8-55ae-bd03-2b430b1c5d59 # return to the installed, enabled original
gitpm fork 76a9b591-60f3-5c7f-9e1c-31648fc73bfd f851f609-bb81-52d6-bfef-33798624782c --into ./me-packages   # several packages, one repo

git init -q tilr-packages
gitpm init store --into ./tilr-packages   # multi-package repository (gitpm.json, packages/*)
gitpm commit ./tilr-packages 0.1.1   # release the added package
gitpm install ./tilr-packages 76a9b591-60f3-5c7f-9e1c-31648fc73bfd   # one package of it; packages share one clone
gitpm commit ./tilr-packages 0.2.0 --package 76a9b591-60f3-5c7f-9e1c-31648fc73bfd   # all packages at 0.2.0, tag v0.2.0
```

`init NAME --into DIR` takes a nonunique alias and generates an immutable UUID.
`fork ID... --into DIR` takes installed UUIDs and generates a new UUID for every
package in the copied repository, preserving aliases. Both commands require an
explicit destination. A new `init` destination becomes a single-package repository;
an existing package repository uses `PREFIX/UUID` for the first `PREFIX/*` pattern
of `gitpm.json` (`packages/UUID` by default). A fork destination must be new.
Git clones the recorded source at installed revisions with history and submodule
links, ignoring local installation edits; repository files and package layout are
preserved. Initialize submodules with `git -C DIR submodule update --init`;
dependencies between the forks are preserved. Use `gitpm fork-rewrite DIR` to
rewrite them explicitly before release; it requires no workspace and repeated calls
leave the files unchanged. Fork preserves inherited tags, creates
no release tag and leaves workspace state, installed packages, replacement bindings
and dist unchanged. Use `commit`, `install` and `replace` explicitly; an existing
release tag still conflicts with `commit`. The removed fork options `--replace`,
`--version`, `--force` and `--ns` are refused before any changes.

Use `fork ID [ID...] --from SOURCE --into DIR [--ref REF]` to fork a
Git path or URL without opening, creating or locking a workspace. Without `--ref`,
it selects the newest stable `vVERSION` release containing the first ID; every ID
must exist there. Tags without that ID and legacy `ID@VERSION` tags are ignored.
`--ref` accepts any revision, branch or untagged commit and requires `--from` for
fork. Omit IDs only for a single-package source. Every package is retained and
assigned a fresh UUID with its name preserved; the destination must be new and outside the local source repository.
For example, after `gitpm init hello --into ./hello`, run:

```sh
gitpm fork --from ./hello --into ./hello-fork
```

Everything works on local folders; nothing is built, executed, pushed or uploaded.
Settings come from flags (`--root`, `--catalog`, `--sources`), then `GITPM_ROOT`,
`GITPM_CATALOG` and `GITPM_SOURCES`; there is no config file. Run `gitpm --help` for all commands, and see the
[repository README](https://github.com/tilr-dev/gitpm#cli-quickstart) for a walkthrough.

License: MIT OR Apache-2.0.

Replacement checks cover every active package. `replace` never installs dependencies,
never prompts, and accepts neither `--force` nor `--back`. `replace-resolve` installs
only missing catalog releases for the proposed provider and its transitive dependencies;
it does not update or enable installed packages or apply the proposed binding. Only
that dependency closure and active consumers' version requirements on the original
block replacement. Unrelated existing breakage remains diagnostic and does not cause
resolve to reinstall force-removed dependencies. Fix incompatible versions explicitly
before replacing.

On a replacement dependency conflict, the CLI suggests `replace-check ORIGINAL --with
PROVIDER` to inspect dependency issues and `replace-resolve ORIGINAL --with PROVIDER`
to install missing dependencies when resolvable. Fix incompatible or disabled installed
dependencies first. Replacement refusal uses the configured catalog to list available
missing versions; it does not install them.
