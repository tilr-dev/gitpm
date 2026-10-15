# tilr-plugin-manager

Core of the Tilr plugin manager as a standalone Rust library, plus a UniFFI wrapper
for native apps.

```text
crates/tilr-plugin-manager/      pure library (no FFI deps)
crates/tilr-plugin-manager-ffi/  UniFFI bindings (proc-macros, no UDL), staticlib + lib
scripts/build-apple.sh           universal macOS staticlib + Swift bindings
```

Plugins are Git repositories with a `plugin.json` manifest (`id`, `version`,
`dependencies`, optional `forkOf`) and a ready `dist/` folder. Installing clones the
repository into a workspace and copies `dist/`; nothing is built or executed.
Releases are Git tags (`vVERSION` or `namespace/name@VERSION`). The catalog is a tree
of static JSON files (local folder or HTTP) with exact commits and file hashes.

## Rules of the library

- Not interactive: no printing, prompts, stdin or environment variables. Every path and
  setting is passed explicitly (`Options`, function arguments). The only implicit input
  is the OS temp directory for scratch clones (`std::env::temp_dir`).
- Results are data (`serde::Serialize`): state, install/update results, check rows,
  catalog overview, uninstall plan, auth check. Progress of long catalog operations is
  reported through a callback.
- Errors are `tilr_plugin_manager::Error` (thiserror). `Error::kind()` is a stable
  snake_case identifier; `Display` is the complete human message.
- Blocking API. `Manager` is `Send + Sync`; its operations are serialized internally.
  Call it off the UI thread.
- All subprocesses (system `git`, `curl`) go through one internal module (`exec`), the
  place to switch to gix/ureq later.
- File formats (state.json, plugin.json, catalog v1/v2) are unchanged from the stand
  prototype; `deny_unknown_fields` is kept where it was.

## API overview

```rust
use tilr_plugin_manager::{Manager, Options};

let pm = Manager::open_with("/path/to/workspace", Options { catalog: Some(url) })?;
pm.install("git@github.com:dan/demo1.git")?;          // latest release tag
pm.install_channel(src, Some("dan/demo1@0.2.0"), false)?; // explicit ref; dev=true follows HEAD
pm.install_from_catalog("dan/demo1", None)?;          // exact catalog release, hashes verified
pm.update("dan/demo1")?;  pm.update_channel(id, None, true)?;
pm.rollback(id)?;  pm.enable(id)?;  pm.disable(id)?;  pm.forget(id)?;  pm.restore(id)?;
let plan = pm.uninstall_plan(id)?;                    // deletes + lost_work
pm.uninstall(id, /* confirm */ !plan.lost_work.is_empty() && user_said_yes)?;
pm.fork_in_namespace(id, Path::new("../forks/demo1"), "me")?;
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

### Error kinds

`not_installed`, `already_installed`, `local_changes`, `locked`, `invalid_id`,
`invalid_manifest`, `invalid_version`, `invalid_argument`, `version_conflict`,
`no_previous_version`, `no_releases`, `repository_not_found`, `workspace`, `catalog`,
`git`, `http`, `auth`, `unsupported`, `invalid_data`, `io`.

## Consumers

**Test stand** (`../temp-plugin-manager`): `manager/` is a thin CLI (`plugin-manager-demo`,
wrapper `./pm`) that depends on this crate by path. It reads `PM_*` variables, applies
defaults, prints, asks `Delete anyway? [y/N]` on a TTY, and runs its own `sync.py`
uploader after `catalog-update --publish`.

**Native app** (macOS): link the static library and use the generated Swift API:

```swift
let pm = try PluginManager.open(root: workspace, options: ManagerOptions(catalog: catalogURL))
let plan = try pm.uninstallPlan(id: "dan/demo1")
if plan.lostWork.isEmpty || userConfirmed(plan.lostWork.map(describeLostWork)) {
    try pm.uninstall(id: "dan/demo1", confirm: !plan.lostWork.isEmpty)
}
```

How the Tilr Mac app (`../apps/mac`) consumes it: `apps/mac/scripts/build-plugin-manager.sh`
runs `scripts/build-apple.sh` when this repo's sources (`Cargo.*`, `crates/`, `scripts/`) are
newer than its last build, copies `TilrPluginManagerFFI.xcframework` to `apps/mac/Vendor/` (a
SwiftPM `binaryTarget`) and the generated Swift to `apps/mac/Sources/TilrPluginManager/Generated/`
(both gitignored there). The `TilrPluginManager` Swift target wraps `PluginManager` in an async
`PluginManagerService` (serial queue, lazy open, reopen on a new workspace/catalog, `PmError` →
`PluginManagerFailure { kind, message }`). The app's `run.sh` and release scripts call it; the
library is linked statically into the executable (needs only `-lSystem`), so the app bundle
gains no files to sign. Its checkout is expected at `../plugin-manager` next to `apps/`
(`TILR_PLUGIN_MANAGER_DIR` overrides). Changing the FFI API means updating
`PluginManagerService.swift` (and the app's `--probe`).

FFI surface: `PluginManager` (open, list, check, browseCatalog, catalogRelease, install,
installFromCatalog, update, rollback, enable, disable, forget, restore, uninstallPlan,
uninstall, fork), free functions `authCheck`, `browseCatalog`, `catalogRelease`,
`readInstalled`, `describeLostWork`; errors are the `PmError` enum (one case per kind,
each with `message`; `LocalChanges` carries `items`). Author/catalog-generation commands
are not exposed.

## Tests

```text
crates/tilr-plugin-manager/src/*.rs     unit tests in each module's #[cfg(test)] mod tests
crates/tilr-plugin-manager/src/test_support.rs  shared fixtures (temp dirs, hermetic git,
                                         plugin repos, tree hash), compiled only for tests
crates/tilr-plugin-manager/tests/       integration tests of the public API
                                         (robustness, uninstall, no_env)
crates/tilr-plugin-manager-ffi/src/lib.rs  FFI conversion tests (errors, records, free functions)
../temp-plugin-manager/manager/tests/   CLI tests of the stand (separate repository)
```

```sh
cargo test                                   # whole workspace, ~10 s
cargo test -p tilr-plugin-manager --lib      # unit tests only
cargo test -p tilr-plugin-manager --lib -- --ignored   # known-defect tests (expected to fail)
cargo clippy --all-targets -- -D warnings
```

Unit tests cover pure parts directly (ID/namespace rules, version keys and ordering,
manifest/state/catalog serialization, catalog validation, check statuses, auth
diagnosis, error kinds and messages, lost-work descriptions) and use small real Git
repositories in temp directories for workspace transactions, release discovery,
lost-work detection, catalog registration/generation, scaffold and fork. Every failing
`Manager` operation is checked to leave the workspace byte-identical (tree hash) with
no stage/backup left behind.

Isolation: tests never read `PM_*`, `HOME` or the network. `.cargo/config.toml` points
`GIT_CONFIG_GLOBAL` at `tests/gitconfig` and sets `GIT_CONFIG_NOSYSTEM=1`; this applies
only when cargo runs from this repository (cargo reads config from the current
directory). Unit-test fixtures additionally pass identity/signing settings with
`git -c`, so they do not depend on the developer's Git config. Temp directories are
removed unless `PM_KEEP_TEST_ARTIFACTS` is set (the stand's `verify.command` keeps them
for its report).

`#[ignore = "known defect: …"]` tests assert the intended contract and currently fail;
they keep known defects visible (`cargo test -- --ignored` lists them). Remove the
attribute together with the fix.

## Build

```sh
cargo test
cargo clippy --all-targets -- -D warnings

# Swift bindings + universal staticlib (aarch64 + x86_64 macOS):
scripts/build-apple.sh            # → target/apple/{lib,swift,include}, xcframework
# Bindings only, from any built library:
cargo run -p tilr-plugin-manager-ffi --features bindgen --bin uniffi-bindgen -- \
  generate --library target/release/libtilr_plugin_manager_ffi.a --language swift --out-dir out
```

`build-apple.sh` works from any directory (it runs cargo and the bindgen from the repo root).
It skips (and reports) a rustup target that is not installed; it never
installs toolchains. UniFFI version: 0.32.
