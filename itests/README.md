# itests — gitpm integration tests in Dagger

A black-box suite for the `gitpm` binary built from this repository. Every test runs in
its own container, concurrently, on the pb Dagger engine (or a local one):

| Kind | What | Id |
| --- | --- | --- |
| rust | each libtest test of the gitpm workspace (unit + integration) | `rust:<package>/<target>::<test>` |
| lint | clippy `--workspace --all-targets -D warnings` | `lint:clippy` |
| fixture | building a prepared state and checking its invariants | `fixture:<name>` |
| scenario | each case of `scenarios/uc_*.py` (use cases UC-001…077) | `scenario:uc_015:<case>` |

Scenarios drive only `gitpm` (and `gitpm-api`, built from `crates/gitpm/examples/api-parity.rs`)
with `GITPM_ROOT`, `GITPM_CATALOG` and `GITPM_SOURCES`; the only network is loopback HTTP
served by the case itself. The registry refuses a case or fixture that mentions `PM_*`
variables or the old stand binary `pm`.

This suite is in addition to `cargo test --workspace` (the required CI gate), not a
replacement; it runs on pb only (no GitHub Actions job).

Workspace layout changes (bare storage, package worktrees and transaction activation)
require the **full suite before review and merge**, in addition to the default workflow
checks. In a workflow task folder, commit the final changes, run `bin/check dt`, then
`bin/check --verify --need dt`. Repeat the gate on the final revision after merging or
changing code. The shared default check set does not include `dt`.

### Layout assertions

`t.clone(ID)` is the shared **bare repository** under `repos/<key>`; it has `objects`
and `HEAD`, not a `.git` subdirectory. `t.worktree(ID)` is the detached package checkout
under `worktrees/<namespace>/<name>` and has a `.git` **file** linking its admin directory
in the bare repository. Assert both when preparing an installed fixture. To create a
local test ref in the bare store, use `git update-ref refs/heads/NAME COMMIT`: `git branch`
can refuse a store whose HEAD points outside `refs/heads`.

Deleting the bare store leaves an orphan checkout. Uninstall and forget drop its
record and dist but preserve those local files. Such cases use
`t.not_installed(ID, orphan_worktree=True)` and a byte snapshot of the orphan checkout;
ordinary removal still requires no checkout. Activation journals replace the bare
store and package checkout separately and record the worktree admin link for recovery.

Diagnostic assertions follow the current specification: moved forgotten HEADs refuse
restore with `local_changes`; nonempty malformed version strings use `invalid_version`,
while missing required manifest fields and empty or whitespace-only manifest versions
use `invalid_manifest`; replacement incompatibility
has no confirmation bypass. `check` uses a listed catalog source without querying
origin and falls back to the installed origin for an unrelated catalog source with
the same ID. Fork provenance records a commit: peel annotated release tags with
`^{commit}` before comparing it.

## Running

Requires the `dagger` CLI **v0.21.10** exactly (the pb engine's version; `./itests/dt`
refuses any other) and, for pb, `dagger-pb` (kube context `promobot`, namespace `dagger`).
Python on the host only runs the report tools, through `uv run`.

```sh
./itests/dt                                   # everything on pb (rust, lint, scenarios)
./itests/dt --only scenarios --filter uc_015  # one use case (and the fixtures it needs)
./itests/dt --only rust --filter lifecycle    # some Rust tests
./itests/dt --only lint                       # clippy only
./itests/dt --cached                          # rerun only tests whose inputs changed
./itests/dt --jobs 32                         # concurrency (default 16)
./itests/dt --open                            # open report.html afterwards
./itests/dt fixtures                          # fixture graph and how many cases use each
./itests/dt fixture catalog                   # export a snapshot to itests/results/fixture-catalog
./itests/dt call gitpm export --path ./gitpm-linux   # the Linux gitpm binary
./itests/dt call cache-regression             # controlled build-cache ownership regression
./itests/dt coverage                          # write openspec/coverage/ (no Dagger)
DT_ENGINE=dagger ./itests/dt                  # the same on a local engine (Docker)
```

With the 1Password SSH agent, `SSH_AUTH_SOCK` can make the Dagger CLI hang; run
`env -u SSH_AUTH_SOCK ./itests/dt …` (the wrapper does not unset it silently).

`./itests/dt` refuses to start when a `.env` file exists in `itests/` or any parent
directory (Dagger would parse it as the module's dotenv) and names the file.

Results go to `itests/results/<timestamp>/` (gitignored; `results/latest` links the last
complete run): `summary.txt` (failures with the tail of their log, totals), `report.json`
(every test with its status and spec tags), `junit.xml`, `logs/<test-id>.log`, `status`,
`report.html` (filters, search, logs). `results/index.html` lists every run with what broke
or got fixed since the previous run with the same arguments (`./itests/dt history`).
`./itests/dt clear [--keep N]` deletes local reports. The exit code is non-zero when a test
is `FAIL` or `XPASS`.

Statuses: `PASS`; `FAIL`; `BLOCKED` (the case needs a capability absent from the runtime,
or its fixture failed — the fixture itself is then `FAIL`); `XFAIL` (a known defect, the case
names the change that fixes it, and it still fails); `XPASS` (an `xfail` case that now
passes: a failure until the marker is removed).

## How it works

`src/gitpm_itests/main.py` (module `gitpm-itests`, engine v0.21.10, Python SDK). The module's
`source` is this repository's root (its Git context, `DefaultPath("/")`), reduced to
`Cargo.*`, `README.md`, `crates/`, `tests/`, `.cargo/` and `openspec/specs/`.

1. **build** — one `rust:1.94.0-bookworm` container compiles every test target, `gitpm` and the
   `api-parity` example, copies the executables into a layer and lists their tests.
   `CARGO_HOME` is shared; target caches are keyed by the complete build source digest
   (excluding `openspec/`) and purpose (build or clippy). Identical snapshots reuse
   compilation; changed sources get separate outputs, because Cargo's mtime freshness
   checks cannot distinguish normalized source timestamps in different snapshots.
   The Rust group includes `rust:build-cache`, a small controlled regression for
   sequential/concurrent source ownership and warm cache reuse.
2. **rust** — one slim container per test running the copied executable with `--exact NAME`.
   Sources sit at `/src/gitpm`, the binary at `/target/debug/gitpm`, as during the build
   (tests bake `CARGO_MANIFEST_DIR` and `CARGO_BIN_EXE_gitpm`); Git config as in
   `.cargo/config.toml` (`tests/gitconfig`).
3. **fixtures** — each fixture is a Dagger layer: parents' `/fx` merged, then
   `python3 -m gitpmtest fixture MODULE:NAME` (build + check), cached by its inputs:
   * *source tier* (`gitpm=False`): git only, the binary is not in the container;
   * *gitpm tier* (`gitpm=True`): uses the CLI; rebuilt once per binary change.
4. **scenarios** — one container per case: fixture snapshot at `/fx` (copied into the
   writable layer first: overlayfs refuses renames and hard links of lower-layer files),
   the case's module, `gitpm` on PATH.
5. Every run gets a fresh run id, so nothing is served from cache unless `--cached`. A red
   test is data: the runner records exit code, duration and log; the module never fails
   because of a test. The report is one bundle unpacked by a single exec (hundreds of
   `with_new_file` layers break the export).

## Writing tests

```
itests/
  gitpmtest/           helper (stdlib only, Python 3.11 in the containers)
  fixtures/*.py        prepared states, shared by many cases
  scenarios/uc_NNN.py  cases of one use case
  tools/               report.py, history.py, coverage.py (host side)
```

### Container layout

```
/fx              working directory, the fixture snapshot
/fx/repos/ID     author repositories (git), e.g. /fx/repos/dan/hello
/fx/ws           GITPM_ROOT       /fx/catalog  GITPM_CATALOG
/fx/sources.txt  GITPM_SOURCES
gitpm, gitpm-api the binaries under test; git, python3, curl, script(1)
```

Git identity, default branch `main` and no signing come from `GIT_CONFIG_GLOBAL`. Commits
and tags made through `t.git`/`t.release`/`t.commit` get deterministic dates.

### A case

```python
from gitpmtest import T, case

@case(fixture="hello_installed", spec=["updates/stable-update-moves-to-the-newest-release"])
def update_to_new_release(t: T):
    """1. update without a catalog picks the new release tag."""
    commit = t.release("dan/hello", "0.1.1")
    t.gitpm_ok("update", "dan/hello")
    t.eq(t.package("dan/hello")["current"], {"version": "0.1.1", "commit": commit}, "current")
```

* `spec=[…]` names what the case verifies: `CAPABILITY/REQUIREMENT-SLUG[#SCENARIO-SLUG]`,
  slugs being the heading text of `openspec/specs/<capability>/spec.md` in lowercase with
  non-alphanumerics collapsed to `-`. A module-level `SPEC = [...]` applies to cases that
  name none; `none:REASON` is for harness self-checks. An untagged case or a tag that
  resolves to nothing fails the registry (and the run, before any test).
* `xfail="CHANGE"` marks a known defect fixed by OpenSpec change CHANGE (`XFAIL`/`XPASS`).
* One case = one independent subcase, starting from a fresh copy of its fixture.
* A scenario module imports only `gitpmtest` (only that file is mounted in the container).
* If gitpm disagrees with the spec, the case fails — do not weaken it to pass; mark it
  `xfail` with the change that fixes it and note it at the top of the module.

### Command names

Scenarios call catalog commands by their current names (`info`, `catalog`,
`catalog-update`, `register`); `gitpmtest.COMMANDS` maps each to the
argv `T.gitpm` really runs, so a CLI rename (change `cli-catalog-commands`) is a change of
that mapping. `t.run("gitpm", …)` bypasses it.

### Helper (`gitpmtest.T`)

| | |
| --- | --- |
| `gitpm(*args)`, `gitpm_ok(*args)`, `gitpm_err(*args, kind=, contains=)` | run the CLI; `input=` feeds stdin, `env=` adds variables, `cwd=` |
| `run(*argv, check=False)` | any process, logged |
| `git(repo_or_id, *args)` | git in an author repo (or any path); raises on failure |
| `new_package(id, version, deps=, extra=, files=)` | init `/fx/repos/ID` with a tagged first release |
| `release(id, version, deps=, extra=, files=, tag=True)` | plugin.json + dist/index.js, commit, tag `ID@VERSION`; returns SHA |
| `commit(repo, message, files)`, `tag(repo, name, rev, force=)`, `rev(repo, ref)` | |
| `write(repo, {path: text/bytes/None})` | edit files (None deletes) |
| `state()`, `package(id)`, `clone(id)`, `dist(id)`, `not_installed(id)` | workspace |
| `catalog_index(id)`, `catalog_release(id, version)`, `edit_json(path, fn)` | catalog files |
| `snap(*paths, git=False)` → `same(snapshot, what)` | "nothing changed" |
| `eq`, `ok`, `has`, `lacks` | assertions (logged) |
| `serve(dir)` → `.url`, `.requests`, `.take()` | loopback HTTP server with a request log |

### A fixture

```python
from gitpmtest import T, fixture

@fixture(requires=["hello"], gitpm=True)
def hello_installed(t: T):
    """dan/hello 0.1.0 installed directly from Git; no catalog."""
    t.gitpm_ok("install", t.repo("dan/hello"), "--ref", "dan/hello@0.1.0")

@hello_installed.check
def _(t: T):
    t.eq(t.package("dan/hello")["current"]["version"], "0.1.0", "current version")
```

* Every fixture has a check (the registry refuses one without). A failed fixture marks its
  cases BLOCKED.
* Prefer the source tier; a source fixture cannot build on a gitpm-tier parent.
* Fixture names are global; a module holds a family of related fixtures.

## Coverage report

`./itests/dt coverage` (`uv run --no-project python itests/tools/coverage.py`, no Dagger)
writes `openspec/coverage/coverage.md` and `coverage.json`: per capability, each
requirement and scenario with the cases tagged for it, the cargo tests the spec text names
("Verified by …"), and each case's status in `itests/results/latest` when present;
requirements with neither are listed under "Uncovered". It exits 1 on an invalid registry.
The report is a local artifact: `openspec/coverage/` is ignored by Git and nothing in it is
committed; regenerate it when you need it.

## Not ported

From the old stand suite (`~/projects/tilr/pm-dagger`, which drove the stand CLI `pm`):

Whole files (58 cases), obsolete by design:

- `uc_005` Interactive init and EOF (7): `gitpm init` takes `NS/NAME --into DIR` and never prompts (spec `package-ids`, no default namespace).
- `uc_038` Sync dry-run and upload validation (15), `uc_039` Sync upload failures and cache purge (19), `uc_064` Sync against existing remote state (10): R2/Cloudflare publication is Tilr infrastructure; gitpm never uploads.
- `uc_051` Publish and consumer consistency under failure (7): publication via `sync`; its consumer side is re-expressed as `uc_077` (partial publication over loopback HTTP).

Single cases dropped from ported files (and what replaced them, if anything):

- `uc_001:missing_fork_path` (fork takes `ID [ID...] --ns --into`; now `missing_fork_id`, `missing_fork_into`, `missing_init_into`), `uc_001:publish_outside_catalog_update` and `uc_063:publish_with_sync` (no `--publish`, no `sync`), `uc_001:dry_run_outside_sync` (now `dry_run_outside_update_all`).
- `uc_002:wrapper_preserves_cwd_and_relative_init`, `uc_002:wrapper_defaults_point_at_the_stand`, `uc_002:env_file_sourced_twice` (the stand wrapper and its `.env`); `uc_002:direct_binary_defaults_in_isolated_cwd` (stand defaults; now `defaults_in_isolated_cwd`).
- `uc_003:fallback_is_local`, `config_beats_fallback`, `env_beats_config`, `option_beats_env`, `changing_default_renames_nothing` (namespace config, `PM_NAMESPACE`, `namespace` command; now `init_without_namespace_refused`, `release_and_update_keep_the_id`).
- `uc_063:namespace_with_install` (now `namespace_flag_is_unknown`), `uc_062:namespace_same_value`, `uc_025:same_namespace_from_config` (now `missing_namespace`).
- `uc_034:overview_selection_precedence` (remote/public catalog precedence; now `info_without_a_catalog_names_it`), `uc_057:info_prints_both_addresses_and_public_fallback` (now `info_shows_the_selected_catalog_only`), `uc_057:catalog_does_not_change_publish_destination` (R2).
- `uc_037:backup_excluded_from_publication`, `uc_041:fifo_in_catalog_is_skipped_by_sync`, `uc_041:symlink_in_catalog_is_refused_by_sync`, `uc_042:sync_dry_run_lists_reachable_files` (`sync`).
- The stand's `sync` test kind (`scripts/test_sync.py`) and `lint:clippy-stand`.
