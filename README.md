# gitpm

Embeddable package manager where packages are folders in Git repositories: releases as
tags, a catalog, dependency resolution, forks and transactional installs. A Rust library
(no CLI, no UI) with a UniFFI wrapper for native apps; the separate `gitpm-cli` crate is a
command-line client over it. Its first host is the Tilr macOS app.

```text
crates/gitpm/                    the library (no FFI deps), published as `gitpm`
crates/gitpm-ffi/                UniFFI bindings (proc-macros, no UDL), staticlib + lib
crates/gitpm-cli/                command-line client, published as `gitpm-cli` (binary `gitpm`)
scripts/build-apple.sh           universal macOS staticlib + Swift bindings
scripts/package-cli.sh           release archive of the `gitpm` binary (release.yml)
```

A package is a Git repository with a `plugin.json` manifest (`id`, `version`,
`dependencies`, optional `forkOf` and `replaces`; other keys are kept as they are) and a
ready `dist/` folder. Installing clones the repository into a workspace and copies `dist/`;
nothing is built or executed. Releases are Git tags (`vVERSION` or `namespace/name@VERSION`).
The catalog is a tree of static JSON files (local folder or HTTP) with exact commits and
file hashes.

## Install

`gitpm-cli` installs a `gitpm` binary:

```sh
cargo install gitpm-cli              # build from source (Rust 1.92+)
cargo binstall gitpm-cli             # prebuilt binary, via cargo-binstall
```

Prebuilt binaries for macOS (arm64, x86_64) and Linux (x86_64, aarch64; glibc 2.35+) are
attached to each [GitHub release](https://github.com/tilr-dev/gitpm/releases) as
`gitpm-cli-vX.Y.Z-TARGET.tar.gz` with a `.sha256` next to it; the archive holds the `gitpm`
binary, the licenses and the README.

From this repository: `cargo install --path crates/gitpm-cli`.

## CLI quickstart

Everything below works on local folders: plugin repositories and the catalog are plain
directories, nothing is pushed or uploaded.

A plugin is a Git repository with `plugin.json` and a ready `dist/` folder; a release is
a `vVERSION` tag (or `namespace/name@VERSION`):

```console
$ mkdir -p hello/dist && cd hello && git init -q -b main
$ echo '{"id": "hello", "version": "0.1.0"}' > plugin.json
$ echo 'export const greeting = "hello v1";' > dist/index.js
$ git add . && git commit -qm "hello 0.1.0" && git tag v0.1.0 && cd ..
$ gitpm install ./hello
OK: install ./hello
$ cat gitpm-workspace/dist/hello/index.js
export const greeting = "hello v1";
$ gitpm list
{
  "plugins": {
    "hello": {
      "source": "/…/hello",
      "current": {
        "version": "0.1.0",
        "commit": "9aed5aa…"
…
```

Release 0.2.0, update, roll back (one previous version is kept):

```console
$ echo 'export const greeting = "hello v2";' > hello/dist/index.js
$ echo '{"id": "hello", "version": "0.2.0"}' > hello/plugin.json
$ git -C hello commit -qam "hello 0.2.0" && git -C hello tag v0.2.0
$ gitpm update hello
OK: update hello
$ cat gitpm-workspace/dist/hello/index.js
export const greeting = "hello v2";
$ gitpm rollback hello
OK: rollback hello
$ cat gitpm-workspace/dist/hello/index.js
export const greeting = "hello v1";
$ gitpm update hello
OK: update hello
```

A catalog is generated from the release tags of the repositories listed in a sources
file (paths are relative to that file). `gitpm init NAMESPACE/NAME` (same as `create`)
scaffolds the plugin repository `./NAMESPACE/NAME` (the folder mirrors the ID) with its
first release; the namespace is required, there is no default. `gitpm commit` bumps the
version, commits everything and tags:

```console
$ export GITPM_CATALOG=$PWD/catalog
$ echo './hello' > sources.txt
$ gitpm catalog-update
Reading releases: /…/hello
Catalog ready: 2 release(s) checked from sources.
$ gitpm info
Catalog: /…/catalog
Type: local folder/JSON
Format: v2
Directories and plugins:
  legacy/
    hello/
      hello — latest: 0.2.0, versions: 2
          Git: /…/hello
$ gitpm init hello
Error: plugin id needs a namespace: use NAMESPACE/NAME, for example dan/hello
$ gitpm init dan/hello

Created: /…/dan/hello
ID: dan/hello
Version 0.1.0: Git commit and tag dan/hello@0.1.0.
…
$ echo 'export const greeting = "dan/hello v2";' > dan/hello/dist/index.js
$ gitpm commit dan/hello 0.1.1
Created local commit and tag dan/hello@0.1.1.
$ echo './dan/hello' >> sources.txt
$ gitpm catalog-update
Reading releases: /…/dan/hello
Reading releases: /…/hello
Catalog ready: 4 release(s) checked from sources.
$ gitpm install-catalog dan/hello
OK: install-catalog dan/hello
$ cat gitpm-workspace/dist/dan/hello/index.js
export const greeting = "dan/hello v2";
$ gitpm check
Workspace: gitpm-workspace
Catalog: /…/catalog

PLUGIN                   INSTALLED      LATEST         ENABLED  STATUS
dan/hello                0.1.1          0.1.1          yes      up to date
hello                    0.2.0          0.2.0          yes      up to date

Updates: 0. Nothing was changed.
```

`gitpm fork ID NAMESPACE/NAME` copies an installed plugin (with its Git history and local
edits) into the new folder `./NAMESPACE/NAME` under that ID. The name stays the same,
only the namespace changes, so the original and the fork sit side by side:

```console
$ gitpm fork dan/hello me/hi
Error: fork keeps the plugin name: use NAMESPACE/hello, not me/hi
$ gitpm fork dan/hello me/hello
Fork created: /…/me/hello
Git history and local edits are kept; the original repository is the upstream remote.
New ID: me/hello. forkOf records the origin; replaces says the fork can stand in for dan/hello
(used only after an explicit: gitpm replace dan/hello --with me/hello).
…
OK: fork dan/hello
$ ls -d */hello
dan/hello
me/hello
$ gitpm uninstall dan/hello
OK: uninstall dan/hello
```

Settings: each has a flag, an environment variable and a default (`gitpm --help` lists
all commands). There is no config file; the old `PM_*` variables are ignored.

| Flag | Variable | Default |
| --- | --- | --- |
| `--root DIR` | `GITPM_ROOT` | `./gitpm-workspace` |
| `--catalog LOC` (folder, `catalog.json` or HTTP(S) URL) | `GITPM_CATALOG` | none: catalog commands, `install-catalog` and `check` ask for it |
| `--sources FILE` | `GITPM_SOURCES` | `./sources.txt` |

Publishing a catalog is up to you: it is a folder of static files, so any static host
works (`catalog-update` and `register` write only to a local folder; HTTP(S) catalogs are
read-only).

## Rules of the library

- Not interactive: no printing, prompts, stdin or environment variables. Every path and
  setting is passed explicitly (`Options`, function arguments). The only implicit input
  is the OS temp directory for scratch clones (`std::env::temp_dir`).
- Results are data (`serde::Serialize`): state, install/update results, check rows,
  catalog overview, uninstall plan, auth check. Progress of long catalog operations is
  reported through a callback.
- Errors are `gitpm::Error` (thiserror). `Error::kind()` is a stable
  snake_case identifier; `Display` is the complete human message.
- Blocking API. `Manager` is `Send + Sync`; its operations are serialized internally.
  Call it off the UI thread.
- All subprocesses (system `git`, `curl`) go through one internal module (`exec`), the
  place to switch to gix/ureq later.
- File formats (state.json, plugin.json, catalog v1/v2) are unchanged from the stand
  prototype; `deny_unknown_fields` is kept where it was.

## API overview

```rust
use gitpm::{Manager, Options};

let pm = Manager::open_with("/path/to/workspace", Options { catalog: Some(url) })?;
pm.install("git@github.com:dan/demo1.git")?;          // latest release tag
pm.install_channel(src, Some("dan/demo1@0.2.0"), false)?; // explicit ref; dev=true follows HEAD
pm.install_from_catalog("dan/demo1", None)?;          // exact catalog release, hashes verified
pm.update("dan/demo1")?;  pm.update_channel(id, None, true)?;
pm.rollback(id)?;  pm.enable(id)?;  pm.disable(id)?;  pm.forget(id)?;  pm.restore(id)?;
let plan = pm.uninstall_plan(id)?;                    // deletes + lost_work
pm.uninstall(id, /* confirm */ !plan.lost_work.is_empty() && user_said_yes)?;
pm.fork_in_namespace(id, Path::new("../forks/demo1"), "me")?;   // sets forkOf + replaces
let plan = pm.replace_plan("vendor/base", "mine/base")?;       // consumers + incompatible
pm.replace("vendor/base", "mine/base", /* confirm */ !plan.incompatible.is_empty() && yes)?;
pm.switch_back("vendor/base", confirm)?;                       // back to the original
pm.uninstall_with(id, confirm, /* force: ignore dependents */ false)?;
pm.set_enabled_with(id, false, force)?;  pm.forget_with(id, force)?;
pm.list()?;  pm.state()?;  pm.check()?;  pm.browse_catalog()?;  pm.catalog_release(id, None)?;
```

Modules with free functions (no workspace lock needed):

| Module | Functions |
| --- | --- |
| `catalog` | `browse`, `list`, `lookup`, `selected`, `register`, `register_build`, `update_sources` (progress callback), `migrate`, `hash_files`, `version_key`, `ensure_publishable` |
| `check` | `read_state` (does not create/lock), `compare`, `check`; `CheckStatus::message()` |
| `auth` | `check(source, &AuthOptions)` — `git ls-remote` only, credentials never stored |
| `scaffold` | `create_namespaced`, `create_at`, `commit` (version tag), `set_remote`, `shell_quote` |
| `identity` | `valid_part`, `qualified`, `release_tag`, `resolve_namespace(explicit, config_path)`, `write_namespace` |
| `releases` | `discover`, `latest` |

### Uninstall: confirmation by plan

`uninstall_plan(id)` is read-only and returns `UninstallPlan { id, registered, deletes,
lost_work }`. `lost_work` lists `LostWork` items: `Modified`, `Untracked`, `Ignored`,
`HeadMoved` (HEAD is not the installed revision), `UnpublishedCommit`,
`UnpublishedBranch`, `UnpublishedTag`, `Stash`. `uninstall(id, false)` refuses with
`Error::LocalChanges { items, .. }` when anything would be lost; `uninstall(id, true)`
deletes anyway. The caller decides how to ask the user. IDs match exactly: on a
case-insensitive file system `Vendor/Module` never resolves to `vendor/module`.

### Dependencies

`dependencies` maps plugin IDs to Cargo-style requirements (`1.2` = `^1.2`, `~1.2`,
`>=1.3, <2`, `=1.0.0`, `1.*`, `*`; no pre-releases). Install resolves them with PubGrub
(`deps` module): installed plugins stay at their versions, missing ones are installed
from the configured catalog in the same transaction (a direct Git install without a
catalog only accepts already installed dependencies). Update and rollback are refused
with `dependency_conflict` when the new version breaks a dependent; new dependencies of
an update are installed. Conflicts carry the solver's explanation naming the plugins.
State keeps the edges (`plugins.<id>.dependencies`); `list()` reports for each
dependency its provider, version and whether it is satisfied, plus each plugin's
`dependents`. Disable, forget and uninstall of a plugin others depend on refuse with
`has_dependents` (names in `dependents`) unless `force`; `UninstallPlan.dependents`
lists them up front. Already broken consumers (after a forced removal) are reported,
not enforced, so they do not block unrelated operations.

### Replacement by a fork

A fork declares `"replaces": {"id": "vendor/base", "version": "1.4.0"}` (`fork` sets
it to the forked version). Nothing is redirected until `replace(original, fork,
confirm)`: then the single global binding `state.replacements[original] = fork`
makes every consumer of `original` — existing and future — resolve to the fork, which
the solver sees as `original` at the declared version. Compatibility is by versions
only: consumers whose requirement the declared version does not satisfy are listed in
`ReplacementPlan.incompatible` and refuse with `incompatible_replacement` unless
confirmed. Self-replacement and cycles are always refused. Replacing again switches the
binding; `switch_back` returns all consumers to the installed original (same
confirmation rule). The active fork is guarded like any provider; a fork update must
keep declaring `replaces`. Uninstalling or forgetting the fork drops its binding.

### Catalog

A workspace uses one catalog. `install_from_catalog` records the canonical catalog
location per plugin (`plugins.<id>.catalog`) and refuses to install from a different
catalog while plugins from another one are installed. `check` reports plugins from
another catalog as `other_catalog` instead of comparing them, and for a fork adds
`upstream { id, base, latest, newer }`: the original's latest catalog release against
the version the fork started from.

### Error kinds

`not_installed`, `already_installed`, `local_changes`, `locked`, `invalid_id`,
`invalid_manifest`, `invalid_version`, `invalid_argument`, `version_conflict`,
`no_previous_version`, `no_releases`, `repository_not_found`, `workspace`, `catalog`,
`git`, `http`, `auth`, `unsupported`, `dependency_conflict`, `has_dependents`,
`incompatible_replacement`, `invalid_data`, `io`.

## Swift bindings

`crates/gitpm-ffi` wraps the library with UniFFI: the Swift module `Gitpm` over the C module
`GitpmFFI` (names in `crates/gitpm-ffi/uniffi.toml`; Kotlin package `gitpm`). Link the static
library and use the generated API:

```swift
import Gitpm

let pm = try PluginManager.open(root: workspace, options: ManagerOptions(catalog: catalogURL))
let plan = try pm.uninstallPlan(id: "dan/demo1")
if plan.lostWork.isEmpty || userConfirmed(plan.lostWork.map(describeLostWork)) {
    try pm.uninstall(id: "dan/demo1", confirm: !plan.lostWork.isEmpty)
}
```

FFI surface: `PluginManager` (open, list, check, browseCatalog, catalogRelease, install,
installFromCatalog, update, rollback, enable, disable(force), forget(force), restore,
uninstallPlan, uninstall(confirm, force), fork, replacePlan, replace, switchBackPlan,
switchBack; `force` defaults to false), free functions `authCheck`, `browseCatalog`, `catalogRelease`,
`readInstalled`, `describeLostWork`; errors are the `PmError` enum (one case per kind,
each with `message`; `LocalChanges` carries `items`). Author/catalog-generation commands
are not exposed.

## Hosts

Tilr's Mac app consumes the library through the UniFFI xcframework: `scripts/build-apple.sh`
builds `GitpmFFI.xcframework` (universal static library + C module) and `Gitpm.swift`, which
the app adds as a SwiftPM `binaryTarget` and a source target (see [Build](#build)).

## Tests

```text
crates/gitpm/src/*.rs                    unit tests in each module's #[cfg(test)] mod tests
crates/gitpm/src/test_support.rs         shared fixtures (temp dirs, hermetic git,
                                         plugin repos, tree hash), compiled only for tests
crates/gitpm/tests/                      integration tests of the public API
                                         (robustness, uninstall, no_env)
crates/gitpm-ffi/src/lib.rs              FFI conversion tests (errors, records, free functions)
crates/gitpm-cli/tests/                  end-to-end tests of the `gitpm` binary
                                         (lifecycle, requirements)
```

```sh
cargo test                                   # whole workspace, ~10 s
cargo test -p gitpm --lib      # unit tests only
cargo test -p gitpm --lib -- --ignored   # known-defect tests (expected to fail)
cargo test -p gitpm-cli                  # CLI tests only
cargo clippy --all-targets -- -D warnings
```

Unit tests cover pure parts directly (ID/namespace rules, version keys and ordering,
manifest/state/catalog serialization, catalog validation, check statuses, auth
diagnosis, error kinds and messages, lost-work descriptions) and use small real Git
repositories in temp directories for workspace transactions, release discovery,
lost-work detection, catalog registration/generation, scaffold and fork. Every failing
`Manager` operation is checked to leave the workspace byte-identical (tree hash) with
no transaction directory left behind; interrupted commits are rolled forward.

Isolation: tests never read `GITPM_*`/`PM_*`, `HOME` or the network. `.cargo/config.toml` points
`GIT_CONFIG_GLOBAL` at `tests/gitconfig` and sets `GIT_CONFIG_NOSYSTEM=1`; this applies
only when cargo runs from this repository (cargo reads config from the current
directory). CLI tests run the binary with a cleared environment, a fixture
`HOME`/`TMPDIR` and Git config, and every `GITPM_*` path inside their temp directory.
Unit-test fixtures additionally pass identity/signing settings with
`git -c`, so they do not depend on the developer's Git config. Temp directories are
removed unless `GITPM_KEEP_TEST_ARTIFACTS` is set (the stand's `verify.command` keeps them
for its report).

`#[ignore = "known defect: …"]` tests assert the intended contract and currently fail;
they keep known defects visible (`cargo test -- --ignored` lists them). Remove the
attribute together with the fix.

## Build

```sh
cargo test
cargo clippy --all-targets -- -D warnings

# Swift bindings + universal staticlib (aarch64 + x86_64 macOS):
scripts/build-apple.sh            # → target/apple/{lib,swift,include}, GitpmFFI.xcframework
# Bindings only, from any built library:
cargo run -p gitpm-ffi --features bindgen --bin uniffi-bindgen -- \
  generate --library target/release/libgitpm_ffi.a --language swift --out-dir out
```

`build-apple.sh` works from any directory (it runs cargo and the bindgen from the repo root).
It skips (and reports) a rustup target that is not installed; it never
installs toolchains. UniFFI version: 0.32.

## License

MIT OR Apache-2.0, at your option: [LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE).
