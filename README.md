# gitpm

Embeddable package manager where packages are folders in Git repositories: releases as
tags, an optional catalog, dependency resolution, forks and transactional installs. A Rust library
(no CLI, no UI) with a UniFFI wrapper for native apps; the separate `gitpm-cli` crate is a
command-line client over it. Its first host is the Tilr macOS app.

```text
crates/gitpm/                    the library (no FFI deps), published as `gitpm`
crates/gitpm-ffi/                UniFFI bindings (proc-macros, no UDL), staticlib + lib
crates/gitpm-cli/                command-line client, published as `gitpm-cli` (binary `gitpm`)
scripts/build-apple.sh           universal macOS staticlib + Swift bindings
scripts/package-cli.sh           release archive of the `gitpm` binary (release.yml)
```

A package is a folder in a Git repository with a `plugin.json` manifest (`id`, `version`,
`dependencies`, optional `forkOf` and `replaces`, descriptive
[metadata](#package-metadata); other keys are kept as they are) and a
ready `dist/` folder: the repository root, or one of several folders a root `gitpm.json`
lists ([multi-package repositories](#multi-package-repositories)). Installing clones the
repository into a workspace and copies the package's `dist/`; nothing is built or executed.
Releases are Git tags `namespace/name@VERSION`.
The catalog is a tree of static JSON files (local folder or HTTP) with exact commits and
file hashes. It is optional: without one, packages are installed and updated straight
from their Git sources ([working without a catalog](#working-without-a-catalog)).

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

Everything below works on local folders: package repositories and the catalog are plain
directories, nothing is pushed or uploaded.

A package is a Git repository with `plugin.json` and a ready `dist/` folder; a release is
a `namespace/name@VERSION` tag. Package IDs always have a namespace.
Repositories are plain folders named by you (`./greeter`); gitpm never derives a folder
from an ID:

```console
$ mkdir -p greeter/dist && cd greeter && git init -q -b main
$ echo '{"id": "acme/greeter", "version": "0.1.0"}' > plugin.json
$ echo 'export const greeting = "greeter v1";' > dist/index.js
$ git add . && git commit -qm "greeter 0.1.0" && git tag acme/greeter@0.1.0 && cd ..
$ gitpm install ./greeter
installed acme/greeter@0.1.0 from /…/greeter
$ cat gitpm-workspace/dist/acme/greeter/index.js
export const greeting = "greeter v1";
$ gitpm list
{
  "format": 3,
  "packages": {
    "acme/greeter": {
      "source": "/…/greeter",
      "current": {
        "version": "0.1.0",
        "commit": "4b377fe…"
…
```

Release 0.2.0, update, roll back (one previous version is kept):

```console
$ echo 'export const greeting = "greeter v2";' > greeter/dist/index.js
$ echo '{"id": "acme/greeter", "version": "0.2.0"}' > greeter/plugin.json
$ git -C greeter commit -qam "greeter 0.2.0" && git -C greeter tag acme/greeter@0.2.0
$ gitpm update acme/greeter
acme/greeter updated 0.1.0 → 0.2.0 (major)
$ cat gitpm-workspace/dist/acme/greeter/index.js
export const greeting = "greeter v2";
$ gitpm rollback acme/greeter
acme/greeter rolled back 0.2.0 → 0.1.0
$ cat gitpm-workspace/dist/acme/greeter/index.js
export const greeting = "greeter v1";
$ gitpm update acme/greeter
acme/greeter updated 0.1.0 → 0.2.0 (major)
$ gitpm update acme/greeter
acme/greeter up to date (0.2.0)
$ gitpm install ./greeter         # installed already: nothing changes, exit 0
acme/greeter already installed (0.2.0)
```

Installing an installed package again from the same source changes nothing and exits 0:
`ID already installed (VERSION)`, with `; run update` when the source resolves to another
revision (a newer release; `update` moves to it; with `--ref REF` or `--dev` it says
`requested --ref REF — run update ID --ref REF`). The same ID from another source is
still refused (`already_installed`). "Same source" means the same string as recorded at
install, after a local path is canonicalized: a `file://` URL against a plain path, or a
URL with and without `.git`, counts as another source.

`download` takes the arguments of `install` (`download-catalog` those of
`install-catalog`) and does the same clone, verification and recording, but records the
package disabled and deploys nothing; missing dependencies are downloaded from the
catalog the same way, disabled, so a later `enable` needs no network. `install` is a
download plus enabling the package and the dependencies it downloaded, in one
transaction. A disabled package satisfies no dependency: `install`, `enable` and `update
ID` of an enabled consumer refuse with `dependency_conflict` ("lib/core is disabled;
enable it") while a dependency resolves only to a disabled package, `list` and `check`
report it unsatisfied, and `update all` skips such a consumer (`dependency DEP: DEP is
disabled`). `enable ID` never enables dependencies itself;
`enable ID --force` enables it anyway (it is then reported broken), and `install
--force` (`install-catalog --force`) is a download plus that forced enable of the named
package. `install` over a package downloaded from the same source enables it (with the
same check) and prints the install line. `rollback` of an active package and `restore`
of an enabled record refuse a disabled dependency as well:

```console
$ gitpm download-catalog acme/app
downloaded acme/app@1.0.0 from /…/app (disabled)
downloaded lib/core@1.2.0 from /…/core (disabled)
$ gitpm enable acme/app
Error: cannot enable acme/app: lib/core is disabled; enable it; force does it anyway
Error kind: dependency_conflict
Hint: `gitpm enable acme/app --force` enables it anyway
$ gitpm enable lib/core && gitpm enable acme/app
lib/core enabled
acme/app enabled
```

### Releases, pinned refs and branches

Every installed package has a mode, recorded in `state.json` as `mode` (omitted in
release mode), and `update ID` follows it:

| Install | Mode (`state.json`) | `update ID` |
| --- | --- | --- |
| `install SRC` | release (no `mode`) | newest release |
| `install SRC --ref acme/greeter@0.1.0` or `--ref COMMIT` | `{"pinned": "acme/greeter@0.1.0"}` (a commit in full) | stays: `acme/greeter pinned at 0.1.0 (acme/greeter@0.1.0)` |
| `install SRC --ref feature` | `{"branch": "feature"}` | HEAD of `origin/feature`, by commit (no version bump needed) |
| `install SRC --dev` | `{"branch": "main"}` (the origin's default branch, by name) | HEAD of `origin/main` |

A ref is resolved against the origin by exact name: a tag, then an origin branch, then a
commit ID (7 to 64 lowercase hex digits); `HEAD` is the origin's default branch, recorded
by its name. A fresh clone finds a branch the origin has as `origin/BRANCH`; local
branches of a clone and revision expressions (`HEAD~1`, `TAG^`) are refused
(`invalid_argument`). `update ID --ref REF` and `update ID --dev` move the package and
record the new mode the same way (a new mode at the installed commit is a change too:
`ID pinned at V (REF)`); `update ID --release` returns it to release mode at the newest
release, even when that is below the installed version (a plain `update` never goes
down). A tag or commit may go back to an older version or keep the installed one at
another commit (an explicit `--ref` of a moved release tag warns `release tag ID@V now
points to COMMIT`); a
branch whose history no longer contains the installed commit, or that the origin no
longer has, is refused (`version_conflict`, "history of main diverged; automatic update
refused"); `update ID --ref BRANCH` follows it again after a force-push. `previous`
records the mode it had, so `rollback` restores revision and mode together. `update all`
takes no `--ref`, `--dev` or `--release`: it follows each package's mode (pinned ones
print `ID pinned at V (REF)`, a diverged branch `ID kept at V (history of BRANCH
diverged)`, a deleted one `ID kept at V (branch BRANCH is gone)`). Installing an installed
package again names its mode: `ID already installed (V, pinned at REF)`. `check` shows a pinned package as `pinned (REF)` without
asking its origin and compares a branch-mode package with the branch HEAD on the origin.
`install-catalog --version V` installs in release mode.

gitpm does not police release tags: every fetch takes the origin's tags as they are (a
moved tag at its new commit, a deleted one gone), and the installed revision is kept by
commit. A release-mode `update ID` or `update all` compares versions only, so a moved tag
of the installed version prints `ID up to date (V)` with `Warning: release tag ID@V now
points to COMMIT; the installed commit is kept`, and a retracted installed release (its
tag deleted, only older ones left) `ID up to date (V)` with `Warning: installed release
ID@V no longer exists on the origin`; a tag moved onto a commit of another version is a
broken tag (`… now points to COMMIT (skipped: REASON) …`; as the newest tag it still
fails `update ID`, "latest release tag is broken"). Integrity of published versions is the job of a
verified catalog. `rollback` to a previous commit that is no longer in the clone (pruned
after its tag went) fails with `no_previous_version` ("previous revision COMMIT of ID is
no longer available").

```console
$ gitpm install ./greeter --ref acme/greeter@0.1.0
installed acme/greeter@0.1.0 from /…/greeter
$ gitpm update acme/greeter
acme/greeter pinned at 0.1.0 (acme/greeter@0.1.0)
$ gitpm update acme/greeter --release
acme/greeter updated 0.1.0 → 0.2.0 (major)
```

`forget ID` keeps the whole record in `state.json` under `forgotten` (own clone or shared
clone) and `restore ID` records it again as it was: revisions, `catalog`, the `enabled`
flag and the mode. A release-mode or pinned package comes back at its recorded commit; a
branch-mode package takes the clone's HEAD as found when it is the recorded commit or a
commit after it that the origin's branch contains (and its dependencies are satisfied;
`dependency_conflict` otherwise). A dirty clone, commits no remote branch or tag contains, or a
HEAD moved elsewhere refuse with `local_changes`.

### Catalogs

A catalog is generated from the release tags of the repositories listed in a sources
file (paths are relative to that file). `gitpm init NAMESPACE/NAME --into DIR`
scaffolds the package with its first release: a new `DIR` becomes a package
repository, a repository with `gitpm.json` gets a package folder. The namespace and `DIR`
are required, there are no defaults. `gitpm commit` bumps the version, commits
everything and tags:

```console
$ export GITPM_CATALOG=$PWD/catalog
$ echo './greeter' > sources.txt
$ gitpm catalog-update
Reading releases: /…/greeter
Catalog ready: 2 release(s) checked from sources.
$ gitpm info
Catalog: /…/catalog
Type: local folder/JSON
Verified: no (discovery only)
Directories and packages:
  namespaces/
    acme/
      greeter/
        acme/greeter — latest: 0.2.0, versions: 2
            Git: /…/greeter
$ gitpm init hello --into hello
Error: package ID needs a namespace: use NAMESPACE/NAME, for example acme/hello
Error kind: invalid_id
$ gitpm init acme/hello --into hello

Created: /…/hello
ID: acme/hello
Version 0.1.0: Git commit and tag acme/hello@0.1.0.
…
$ echo 'export const greeting = "hello v2";' > hello/dist/index.js
$ gitpm commit hello 0.1.1
acme/hello committed 0.1.1 (local commit and tag acme/hello@0.1.1)
$ echo './hello' >> sources.txt
$ gitpm catalog-update
Reading releases: /…/greeter
Reading releases: /…/hello
Catalog ready: 4 release(s) checked from sources.
$ gitpm install-catalog acme/hello
installed acme/hello@0.1.1 from /…/hello
$ cat gitpm-workspace/dist/acme/hello/index.js
export const greeting = "hello v2";
$ gitpm check
Workspace: gitpm-workspace
Catalog: /…/catalog (unverified: discovery only)

PACKAGE                  INSTALLED      LATEST         ENABLED  SOURCE   STATUS
acme/greeter             0.2.0          0.2.0          yes      catalog  up to date
acme/hello               0.1.1          0.1.1          yes      catalog  up to date

Updates: 0. Nothing was changed.
```

#### Verified catalogs

A catalog is a guarantor only when its owner says so: `"verified": true` in the
top-level `catalog.json`, set by hand (no command sets it; `register` keeps it). Any other
catalog serves discovery only: `install-catalog` still installs exactly the release record
(commit, manifest and file hashes are checked), but records nothing, and the package then
updates from its origin like a direct install.

- Versions enter a verified catalog only one by one through `register` (a manual act of
  verification); `catalog-update` into it fails with `invalid_argument` ("a verified
  catalog takes versions only one by one through register") and changes nothing. An ID
  stays bound to the source that first registered it, which is the namespace ownership.
- A package installed from a verified catalog (and each dependency pulled from it) is
  protected: its record has `catalog` (the canonical location; `list` shows it only for
  protected packages). `update ID` and `update all` move it only to versions that catalog
  lists, read from the recorded location whatever `--catalog` says, after checking that
  the origin has the release commit and that manifest, package folder and dist file hashes
  equal the record (`catalog`: "catalog file hashes do not match git dist of ID@V: the
  release differs from the one added to the catalog"; nothing changes). A newer origin
  release the catalog lacks is not installed: `ID kept at V (upstream W not in catalog)`,
  and `check` says `upstream W not in catalog`. A ref or branch target (`--ref`, `--dev`)
  needs a detach. New dependencies of a protected package come from its catalog too (and
  are protected by it). If the owner removes the flag, or the catalog cannot be read,
  `update ID` fails with `catalog` (or `http`) until the package is detached, while
  `update all` keeps only that package: `ID kept at V (catalog unavailable: REASON)`;
  `check` adds the problem `catalog no longer verified` to its row.
- `install-catalog` over a direct install (or download) of the same source protects it
  when the installed revision is exactly the catalog's record of its version (commit and
  file hashes checked; `ID is now protected by catalog LOCATION`); otherwise it fails with
  `catalog` and changes nothing.
- `update ID --detach` drops the protection: the record loses `catalog` and the package
  updates from its origin by the given target in the same transaction (`ID detached from
  catalog LOCATION; it now follows its origin`). Direct (URL or path) installs never
  consult a catalog.
- One verified catalog per workspace: installing from a second verified catalog is refused
  with `catalog` ("… mixing verified catalogs is not supported"); unverified catalogs are
  exempt. `check` prints `Catalog: LOCATION (verified)` or `(unverified: discovery only)`
  and `info` a `Verified:` line.

```console
$ gitpm install-catalog acme/hello            # catalog.json has "verified": true
installed acme/hello@0.1.1 from /…/hello
$ gitpm update acme/hello                     # the origin also has 0.2.0, not registered
acme/hello kept at 0.1.1 (upstream 0.2.0 not in catalog)
$ gitpm update acme/hello --detach
acme/hello detached from catalog /…/catalog/catalog.json; it now follows its origin
acme/hello updated 0.1.1 → 0.2.0 (major)
```

`gitpm fork ID --ns NAMESPACE --into DIR` copies an installed package (with its Git
history and local edits) into the fork repository `DIR` under the ID `NAMESPACE/<same
name>`: only the namespace changes, so the original and the fork can be installed side by
side. `DIR` is always given (a new folder; or an existing repository with `gitpm.json`,
see below); it is never derived from the ID. The output lists the next steps (commit and
tag, install, replace):

```console
$ gitpm fork acme/hello --ns me --into hello-fork
Fork created: /…/hello-fork
Git history and local edits are kept; the original repository is the upstream remote.
me/hello forked from acme/hello (repository root)
forkOf records each origin; replaces says the fork can stand in for it (used only
after an explicit replace). The original installations are unchanged.

Next steps (or all at once: add --replace):
  gitpm commit '/…/hello-fork' NEW_VERSION --package me/hello   commit local edits, tag me/hello@NEW_VERSION
  gitpm install '/…/hello-fork' me/hello
  gitpm replace acme/hello --with me/hello
Set your own origin with: gitpm remote '/…/hello-fork' GIT_URL
$ ls -d hello*
hello
hello-fork
$ gitpm uninstall acme/hello
acme/hello uninstalled
```

`fork ID --ns NAMESPACE --into DIR --replace` does all of it at once: fork, release the
fork right away (tag `NAMESPACE/NAME@VERSION`; `--version V`, default the forked version;
local edits of the installed copy are committed with it), install it and replace `ID`
with it for every consumer. Incompatible consumers are handled as by `replace`: asked on a
TTY, otherwise only with `--force`. The fork folder is never removed again: when the
release, install or replace step fails, the command says per package what is left and how
to finish or undo (for example `gitpm install … me/hello` then `gitpm replace …`, or
`gitpm uninstall me/hello` and delete the folder). `replace` prints the installed version
of the provider and, when it differs, the version it stands in as (`acme/hello replaced by
me/hello 1.5.0 (as acme/hello 1.0.0) for: …`). When the replace step needs confirmation,
the library returns the replacement as pending; the CLI shows its warning, asks on a TTY
and applies it with the library's resume call.

Output: stdout carries the result lines (one per affected package for workspace-mutating
commands, JSON for `list`, `catalog` and `register`); everything else goes to stderr:
`Warning: …` lines taken from the result (ignored metadata keys, skipped release tags,
the dist report), `[y/N]` prompts with what they list, and failures. A failure prints
`Error: MESSAGE`, for a library error also `Error kind: KIND` (and one `  - item` line
per lost item for `local_changes`, and for an uninstall's `has_dependents`) and, where a gitpm command helps, one `Hint: …` line
naming it; the exit status is 1. The CLI checks only the command line (unknown, repeated
or misplaced options, value sets, argument counts); domain rules and option
combinations are the library's, which also returns everything the CLI prints, so a host
gets the same through the FFI.

```console
$ gitpm forget acme/hello && gitpm install ./hello
acme/hello forgotten (clone kept; restore re-registers it)
Error: acme/hello was forgotten and its clone is kept
Error kind: already_installed
Hint: if acme/hello was forgotten, `gitpm restore acme/hello` records it again and `gitpm uninstall acme/hello` removes its clone
```

Changed in 0.4: the form `fork ID NAMESPACE/NAME [--into DIR]` (fork folder
`DIR/NAMESPACE/NAME`, default `./NAMESPACE/NAME`) and `init NAMESPACE/NAME` without a
folder are gone; `init --in REPO` is now `init --into DIR`.

Changed in 0.5:

- [Plugin metadata](#package-metadata): `title`, `description`, `provides`, `tags` in
  `plugin.json`, reported by `list` (and `list --provides SERVICE`), `info` and the FFI;
  `init` writes a `title`. `info`/`browse` now read each listed plugin's latest
  `release.json` (one extra HTTP request per plugin on a remote catalog, at most 40).
- Output: every mutating command prints one line per affected package, `ID <verb> …`
  (detail lines such as `Fork created:` stay as they were); `OK:` lines are gone from
  all commands (scripts that match them must match the new lines; `Warning:` lines on
  stderr and JSON output are unchanged):

  | Command | Line |
  | --- | --- |
  | `install`, `install-catalog` | `installed ID@VERSION from SOURCE`, or `ID already installed (VERSION)[; run update]` |
  | `update ID`, `update all` | `ID updated A → B`, `ID up to date (A)` (`update all --dry-run`: `would be updated`); `update all` also `ID kept back at A (latest L: REASON)` and `ID skipped at A (dependency DEP is not installed)`; a protected package `ID kept at A (upstream W not in catalog)`; `update ID --detach` first `ID detached from catalog LOCATION; it now follows its origin` |
  | `rollback` | `ID rolled back A → B` |
  | `enable`, `disable` | `ID enabled`, `ID disabled` |
  | `forget`, `restore` | `ID forgotten (clone kept; restore re-registers it)`, `ID restored VERSION` |
  | `uninstall` | `ID uninstalled` |
  | `replace --with` | `ORIGINAL replaced by FORK VERSION [(as ORIGINAL V)] for: consumers` (was `ORIGINAL → FORK …`) |
  | `replace --back` | `ORIGINAL switched back from FORK (ORIGINAL VERSION for: consumers); FORK stays installed and enabled (disable or uninstall it if not wanted)` |
  | `fork` | `FORK forked from ORIGINAL (FOLDER)` (was `Forked: ORIGINAL → FORK …`) |
  | `commit` | `ID committed VERSION (local commit and tag TAG)` |
  | `migrate-catalog` | `Catalog migrated: LOCATION (format v2, one folder per plugin)` (was silent) |
  | `sync-dist` | `ID deployed to dist/ID`, `ID removed from dist/ID` (`--dry-run`: `would be …`) |
- `install` of an installed package from the same source is no longer an error: it
  prints `ID already installed (VERSION)` (`; run update` when the source has another
  revision) and changes nothing; `InstallResult` has `already_installed` and `available`
  (also in the FFI). Another source for the same ID is still `already_installed`.
- `gitpm COMMAND --help` (or `-h`) prints that command's usage; builds from a Git
  checkout that is not at a release tag report `gitpm --version` as `VERSION-dev+HASH`.
- Metadata types: `commit` refuses a `plugin.json` whose metadata keys have the wrong
  type (`invalid_manifest`); `install`, `update`, `rollback`, `fork`, `catalog-update`
  and `list` accept such manifests (published before 0.5) and treat those keys as
  absent, with a warning on stderr for install and update.
- `init --multi` makes a new (or empty) folder a multi-package repository;
  `init --no-release` commits without the release tag.
- `list`: `resolved[]` entries carry `clone` (as `plugins.<id>.clone`).
- Removing a package also removes the folders it leaves empty (`dist/<namespace>`,
  `plugins/<namespace>`, `repos`).
- `dist/` holds only active packages (installed, enabled, not replaced; see
  [Workspace layout](#workspace-layout-and-dist)): `replace` removes the original's
  `dist/`, `replace --back` and `enable` deploy it again, `disable` and `forget` remove
  it. Hosts no longer need `resolved[].replacing`/`.enabled` to pick what to load: load
  everything under `dist/` (or use the new `resolved[].active`). A workspace written by 0.4 keeps every installed package
  there: the first mutating command fixes it; until then (only read-only commands since
  upgrading) run `gitpm sync-dist` (FFI `syncDist`) once.

Changed in 0.6 (no backward compatibility before 1.0; every entry is **BREAKING**):

- `commit`, `init` into an existing repository, and adding to an existing fork now
  refuse a missing configured Git identity with `invalid_argument` instead of using
  a name or email derived by Git. Migration: set `user.name` and `user.email` in Git
  config (or provide both author and committer identity environment variables).
- Package IDs are always `namespace/name`; an unscoped ID (`demo`) is refused with
  `invalid_id` everywhere (manifests, dependencies, state, commands). `update all` is
  always the joint upgrade, as no ID can be `all`. Migration: re-create the package
  under a namespace (`gitpm init NAMESPACE/NAME --into DIR`, or change `id` in
  `plugin.json` and tag it).
- Release tags are only `ID@VERSION`. `vVERSION` tags are no releases any more, in
  single- and multi-package repositories, and are ignored without a warning (a
  repository with only `v1.0.0` fails install with `no_releases`); `init`, `commit` and
  `fork --replace` never create them. Migration: tag `ID@VERSION` on the release commits.
- `state.json` format 3: installed packages are under `packages` (was `plugins`), own
  clones in the workspace folder `packages/` (was `plugins/`). Any other format, or a
  file without `format`, is refused with `unsupported` and never rewritten; there is no
  migration. Migration: re-create the workspace (install its packages again into a new
  workspace root).
- Plugin → Package rename of the library and the FFI (the manifest file stays
  `plugin.json`); messages and CLI output say "package" (`package ID is not installed`,
  `dist/ already holds exactly the active packages.`, `check` column `PACKAGE`).
  Migration: rename in host code.

  | 0.5 | 0.6 |
  | --- | --- |
  | `gitpm::InstalledPlugin` | `gitpm::InstalledPackage` |
  | `State.plugins` (`"plugins"` in `state.json`, `list`) | `State.packages` (`"packages"`) |
  | `catalog::PluginSummary` | `catalog::PackageSummary` |
  | `catalog::Plugin`, `Catalog.plugins` (result of `selected`) | `catalog::Package`, `Catalog.packages` |
  | `catalog::OverviewItem::Plugin { depth, plugin }` | `OverviewItem::Package { depth, package }` |
  | `catalog::CatalogFormat`, `CatalogOverview.format` | removed (only the v2 tree exists) |
  | `State::installed_in(root)` (read metadata from the clone) | removed; use `State::installed()` |
  | `Installed.metadata: Option<Metadata>` | `Installed.metadata: Metadata`, required in `state.json` (`invalid_data` without it) |
  | FFI `PluginManager` | `PackageManager` |
  | FFI `InstalledPlugin`, `PluginSummary` | `InstalledPackage`, `PackageSummary` |
  | FFI `OverviewItem.plugin(depth, plugin)` | `OverviewItem.package(depth, package)` |
  | FFI `CatalogFormat`, `CatalogOverview.format` | removed |
  | workspace `plugins/<ns>/<name>/` | `packages/<ns>/<name>/` |

- v1 catalogs are refused: a monolithic `catalog.json` (`{"plugins": …}`) fails every
  read and write with `catalog`; `legacy/<id>/` folders and releases without file
  hashes are gone (`catalog release of ID has no file hashes`). `migrate-catalog` and
  `catalog::migrate` are removed; `info` no longer prints a `Format:` line. Migration:
  regenerate the catalog with `catalog-update` into an empty folder.
- The CLI alias `create` is removed (`init` only); `create` and `migrate-catalog` fail
  with `unknown command: X; use --help`.
- `list` takes metadata only from the state record; `metadata` is required in every format-3 record (a record without it is `invalid_data`).
- A new fork no longer gets the Git config keys `pm.forkBase`/`pm.forkVersion` (nothing
  read them); `forkOf` in the manifest is the only fork provenance.
- `gitpm-ffi` carries the library version (0.6.0 with this release) and promises no
  stability before 1.0.
- A local catalog location that does not exist (`--catalog ./catlog`, `GITPM_CATALOG`,
  `Options.catalog`) is refused with `invalid_argument` ("catalog location PATH does not
  exist") before anything is created; it no longer counts as "no catalog" for `install`.
  `Manager::open_with` validates its Options first. The free catalog readers do the same:
  `check`, `info`/`browse`, `catalog`, `catalog_release`/`lookup` on a missing local
  location fail with `invalid_argument` (was `catalog` "catalog not found"). Migration:
  create the catalog folder (`catalog-update`/`register` create it) or drop the setting.
- `gitpm catalog` prints the library's typed directory links `{"schema": 2,
  "directories": {…}}` (`catalog::links`, the same as FFI `catalogLinks`) instead of the
  raw `catalog.json`; a v1 or otherwise invalid root fails with `catalog`.
- `uninstall` and `forget` of a recorded package whose clone is gone drop its record and
  `dist/` without confirmation (was `repository_not_found`; `forget` keeps no `forgotten`
  record then); the CLI warning for such a package says `uninstall or forget ID to drop
  its record` (was `run \`gitpm sync-dist\``, which could not deploy it); `restore` of a forgotten package whose shared clone is gone fails
  with `repository_not_found` (was `workspace`), like any restore without a clone.
- Active packages below a file or symlink at `dist/<namespace>` are now reported in
  `DistSync.skipped` with reason `dist/<namespace> is not a folder`. Mutating commands
  warn `Warning: ID: dist not deployed (dist/<namespace> is not a folder)`;
  `sync-dist` now exits 1 with `not_deployed` for these packages (previously 0), with a
  hint to remove or rename the blocked path and run `gitpm sync-dist` again.
- `sync-dist` leaving active packages undeployed is the library error `not_deployed` (new
  kind; `Manager::sync_dist` returns `Err(Error::NotDeployed { sync, .. })` after
  committing what it could) instead of `Ok` with `skipped` that the CLI turned into a
  failure. Migration: match the kind.
- `list` no longer opens the workspace (no lock, no creation, no crash recovery); it is
  the library's `list_with(root, &ListOptions { provides })`, `--provides` included.
- Library messages name no CLI command or hosting service; the CLI adds a `Hint:` line
  per error kind (for example after `already_installed`: `gitpm restore ID` /
  `gitpm uninstall ID`). `init NAME` without a namespace is the library's `invalid_id`
  ("package ID needs a namespace: …", now with `Error kind`). Prompts (`[y/N]` and what
  they list) go to stderr.
- `update ID` returns the broken release tags it passed over in `RevisionChange.skipped`,
  `update all` in `UpdateAllPlan.skipped` (for every installed package, updated or not),
  and the CLI warns `Warning: skipped release tag NAME: reason` as install does.
- A refused confirmation of a `fork --replace` replacement (`n` on the TTY) no longer
  ends the command with "replacement cancelled; nothing was changed": the fork stays
  released and installed, the replacement pending, and the CLI says how to finish.
- Library API: `Manager::set_enabled`/`set_enabled_with`/`enable`/`disable`/`forget`/
  `forget_with`/`uninstall` return the `DistSync` report (were `()`);
  `InstallResult`, `RevisionChange`, `ReplacementPlan` and `UpdateAllPlan` gain `dist`;
  `RevisionChange` gains `skipped`; `UpdateAllPlan` gains `unchanged` (`{id, version}`:
  every installed package is in `updates`, `kept_back` or `unchanged`);
  `ReplacementPlan` gains `installed_version` (the provider's installed version);
  `ForkReplacement` gains `pending` (a replacement that needs
  confirmation; apply it with `Manager::fork_replace_resume`, see
  `PackageForkReplacement::pending`, which returns `ForkResume { applied, failed }`:
  the plans applied, each with its dist report, and the first failure);
  `UpdateAllPlan` gains `skipped`;
  `scaffold::commit`/`commit_package` return `Commit { package, version, tag }` (were the
  tag); `check::check(root, catalog: Option<&str>)` returns `CheckReport { rows,
  replacements, unsatisfied }` (was the rows; an empty workspace needs no catalog).
  New: `list::list_with`, `catalog::links`, `catalog::validate_location`,
  `identity::split`, `scaffold::suggests_multi`.
- FFI: free functions `list(root, provides)` and `readState(root)` (both `ListView`: the
  raw state with `packages`, `replacements`, `forgotten`, plus `resolved`), `check(root,
  catalog)` (`CheckReport`) and `catalogLinks(location)`; `readInstalled` and
  `PackageManager.check` are removed; `enable`/`disable`/`forget`/`uninstall` return
  `DistSync`; `InstallResult` gains `skipped`, `warnings`, `dist`; `RevisionChange` gains
  `warnings`, `skipped`, `dist`; `UpdateAllPlan` gains `unchanged`, `dist`;
  `ReplacementPlan` gains `installedVersion`, `dist`; `ForkReplacement`
  gains `pending` and `PackageForkReplacement` gains `pending` (`PendingReplacement`
  list for the new `PackageManager.forkReplaceResume(pending, confirm)`, which returns
  `ForkResume { applied, failed }`); `UpdateAllPlan` also gains `skipped`;
  `PmError.NotDeployed { message, sync }` (the whole `DistSync` report) is new.
  Migration: call the free functions, read the new fields.
- `check` works without a catalog: a package the catalog does not list, or every package
  when no catalog is given, gets an origin row from `git ls-remote --tags` of its recorded
  source. `CheckStatus::NotInCatalog` (`not_in_catalog`, "not in catalog") is removed; new
  statuses `newer_than_origin`, `no_releases`, `origin_unreachable`. `CheckRow` gains
  `source` (`CheckSource::Catalog`/`Origin`) and `problem`, `Upstream` gains `source`
  (`Option<CheckSource>`) and `problem`; `check::compare`/`compare_in` are no longer pure:
  they run `git ls-remote` for packages the given catalog does not list (was `not in
  catalog`); `check::Row` (alias of `CheckRow`) is removed; `check::check`
  and `Manager::check` no longer fail with "no catalog configured". The CLI table gains
  the `SOURCE` column, prints `Catalog: none (each package's origin)` without a catalog,
  and the fork line says `original in catalog: V` or `original at origin: V`
  (`original: unknown` instead of `not in catalog`). FFI: `CheckStatus.notInCatalog`
  removed, `newerThanOrigin`/`noReleases`/`originUnreachable` and enum `CheckSource` new,
  `CheckRow` gains `source`/`problem`, `Upstream` gains `source`/`problem`. Migration: handle the
  new statuses and drop `not_in_catalog`.
- `fork` writes `forkOf.source` (the original's recorded source) into the fork's
  `plugin.json`; `forkOf` is `{id, version, commit, source?}` (`source` optional when
  reading; `invalid_manifest` when empty, relative or a remote helper such as `ext::…`;
  `fork` leaves it out when the original's source is not a URL or an absolute path). FFI
  `ForkOf` gains `source`. A gitpm before this
  change refuses such a manifest or state record (unknown key in `forkOf`).
- `--channel stable|dev` is removed, without an alias (`unknown option: --channel; use
  --help`); one `--ref` decides the mode, recorded per package (see [Releases, pinned refs
  and branches](#releases-pinned-refs-and-branches)): `--ref TAG|COMMIT` pins (`update`
  then prints `ID pinned at V (REF)` and changes nothing), `--ref BRANCH` follows that
  origin branch, the new `--dev` follows the origin's default branch, and the new `update
  ID --release` returns to releases. A tag given to `install`/`update --ref` used to be a
  one-off move; it now pins. `update` without options follows the recorded mode (a
  `--dev` install no longer fails `update ID` with `version_conflict`); `update all`
  follows every package's mode and refuses `--ref`/`--dev`/`--release` ("update all takes
  no --ref, --dev or --release; it follows each package's recorded mode"). `--ref`,
  `--dev` and `--release` exclude each other (usage errors `--dev conflicts with --ref`,
  `--release conflicts with --dev`, `--release conflicts with --ref`). Migration: replace
  `--channel dev` with `--dev`, drop `--channel stable`, run `update ID --release` where a
  ref should not stay pinned.
- `install --ref BRANCH` resolves against the origin like `update --ref`, by exact name: a
  tag, an origin branch (`origin/BRANCH` of the fresh clone, so a branch other than the
  default one installs), then a commit ID of 7 to 64 lowercase hex digits; revision
  expressions (`HEAD~1`, `TAG^`), shorter IDs and `refs/heads/*` are refused with
  `invalid_argument` (an unknown ref too, was `git`).
- `update ID --release` may go down to the newest release (from a branch commit or a pin
  above it); `rollback` swaps the mode with the revision (`previous` records its `mode`);
  a change of mode alone is a change (`RevisionChange.changed` at the same commit, CLI
  `ID pinned at V (REF)`, `ID follows branch B at V`, `ID follows releases at V`); a
  branch the origin no longer has is `version_conflict` for `update ID` and reason
  `branch_gone` in `update all`; `ID already installed (V, pinned at REF)` names the
  recorded mode, and `--ref`/`--dev` asking for another mode adds the update hint.
- `state.json` records gain `mode` (omitted in release mode, else `{"pinned": REF}` or
  `{"branch": NAME}`); the format stays 3 (no released gitpm wrote format 3 without it).
  `forgotten` now keeps the record of every forgotten package, own clones included.
- `restore` records the forgotten package as it was: revisions (`previous` kept),
  `catalog`, `enabled` and mode, for own and shared clones alike (an own clone used to be
  registered at its HEAD, enabled, without `catalog` or `previous`). Release and pinned
  mode take the recorded commit, branch mode a published HEAD after it. Lost work refuses
  with `local_changes`: file changes, unpublished commits (`UnpublishedCommit`, in every
  mode) and a moved HEAD (`HeadMoved`). A clone without a `forgotten` record is not
  restorable (`repository_not_found`); `restore` of a recorded package is
  `already_installed`, even when its clone is gone (was `repository_not_found`).
- `check`: a pinned package's row is the new status `pinned` ("pinned"; nothing is
  queried); a branch-mode row compares the installed commit with the branch HEAD on the
  origin (`up_to_date`/`update_available`; `origin_unreachable` "branch NAME not found on
  the origin"). The origin is read with `git ls-remote --tags --heads`. `CheckRow` gains
  `mode`; the CLI table adds `(REF)`/`(branch NAME)` to the status.
- Library API: `Manager::install_ref`, `install_channel`, `update_ref` and
  `update_channel` are removed; `install_package(source, package, InstallTarget)`
  (`InstallTarget::{Release, Ref(String), Dev}`) and `update_to(id, Target)`
  (`Target::{Recorded, Release, Ref(String), Dev}`) replace them (`install(source)` and
  `update(id)` stay). New `Mode::{Release, Pinned(String), Branch(String)}` in
  `Installed.mode`, `InstalledPackage.mode`, `RevisionChange.mode`, `InstallResult.mode`
  and `Revision.mode` (only in `previous`); `InstallResult.requested_mode`;
  `RevisionChange.unchanged_reason` and `Unchanged.reason` (`UnchangedReason::{Pinned {
  reference }, Diverged { branch }, BranchGone { branch }}`).
- FFI: enum `Channel` is removed; `PackageManager.install(source, target: InstallTarget,
  package)` (`release`, `ref(reference)`, `dev`) and `update(id, target: Target)`
  (`release`, `ref(reference)`, `dev`, `recorded`). `InstalledPackage`, `PackageRecord`,
  `CheckRow`, `RevisionChange`, `InstallResult` and `Revision` gain `mode` (`Mode`:
  `release`, `pinned(reference)`, `branch(name)`), `InstallResult` gains `requestedMode`,
  `RevisionChange` gains `unchangedReason` and `Unchanged` gains `reason`
  (`UnchangedReason`: `pinned(reference)`, `diverged(branch)`, `branchGone(branch)`), `CheckStatus` gains
  `pinned`. Migration: pass a target instead of `reference` and `channel`.
- A disabled package satisfies no dependency of an active package in `install`,
  `install-catalog`, `update ID`, `enable`, `rollback` and `restore` (direct dependencies
  only): `install`/`install-catalog` and `update ID` of an enabled package fail with `dependency_conflict` ("DEP is disabled;
  enable it") when a dependency resolves only to a disabled package (they used to accept
  it; `update all`, `list` and `check` already counted it unsatisfied). Migration: enable
  the dependency first, or use `download` and `enable --force`.
- `update all` follows the dnf model: a broken consumer (a dependency unsatisfied now,
  for example after a forced uninstall) is skipped, outside the solve, and listed in
  `unchanged` with `UnchangedReason::BrokenDependency { dependency, problem }` (CLI `ID
  skipped at V (dependency DEP is not installed)`); the removed dependency is never
  reinstalled for it and a broken state no longer fails the run with
  `dependency_conflict`. A disabled consumer takes a disabled dependency as given.
  `HeldBack` gains `reason` (`HeldBackReason`: `Constraints`, `MissingDependency {
  dependency, requirement }`, `NoLongerReplaces { original }`; `Display` is the
  message), and the CLI prints it: `ID kept back at V (latest L: REASON)` and `ID updated
  A → B (latest L kept back: REASON)` (was `kept back by dependency constraints`). FFI:
  `HeldBack` gains `reason` (`HeldBackReason` enum) and `message`, `Unchanged` gains
  `message`, `UnchangedReason` gains `brokenDependency(dependency, problem)`. Migration:
  read `unchanged[].reason` for skipped consumers and `keptBack[].reason`.
- `enable ID` is strict: it refuses with `dependency_conflict` naming every dependency no
  active package satisfies (disabled: "DEP is disabled; enable it"; missing, replaced
  away or of another version: "DEP REQUIREMENT: problem") and never enables
  dependencies itself; `enable ID --force` enables it anyway. An already enabled package
  is not checked again. Migration: enable dependencies first, or pass `--force`.
- `rollback` of an active package and `restore` of an enabled record refuse with
  `dependency_conflict` ("DEP is disabled; enable it") while a dependency is disabled.
- `install --force` and `install-catalog --force` enable the named package over a
  disabled dependency (dependencies are pulled as usual); `install` over a package
  downloaded (disabled) from the same source enables it and prints `installed …`
  (`already installed` only for an enabled package). The `--force` usage error is now
  `--force is only valid with uninstall/disable/enable/install/install-catalog/forget/replace/fork --replace`.
- New `download SOURCE [ID] [--package ID] [--ref REF | --dev]` and `download-catalog
  ID [--version V]`: clone, verify and record like `install`/`install-catalog`, but
  disabled and without deploying; missing dependencies are downloaded disabled. Output
  `downloaded ID@V from SOURCE (disabled)`, one line per package. `install` is download
  plus enable in one transaction and now also prints one `installed …` line per
  dependency it pulled from the catalog.
- Library API: `Manager::download(source, package, InstallTarget)`,
  `download_from_catalog(id, version)`, `enable_with(id, force)`,
  `install_package_with(source, package, target, force)` and
  `install_from_catalog_with(id, version, force)` are new;
  `set_enabled_with(id, true, false)` and `enable` refuse inactive dependencies;
  `InstallResult` gains `enabled` and `dependencies` (`InstalledDependency { id, source,
  current, enabled, warnings }`, the packages pulled from the catalog).
- FFI: `PackageManager.download(source, target, package)` and
  `downloadFromCatalog(id, version)` are new; `install` and `installFromCatalog` gain
  `force` (default false); `enable(id, force)` takes `force`
  (default false); `InstallResult` gains `enabled` and `dependencies`
  (`InstalledDependency`). Migration: pass `force` where needed, read the new fields.
- One rule for removing an active fork ([Removing a replacement](#removing-a-replacement)):
  `disable`, `forget` and `uninstall` of it drop the binding and deploy the original
  again; `has_dependents` only when a consumer would break afterwards (the original
  missing, disabled or of another version), and `dependents` names only those consumers
  (disabling an active fork used to be refused always, naming the originals). The
  reactivated original's dependencies are checked like `enable` (`dependency_conflict`;
  forceable for the removals, not for `replace --back`). Migration: `disable --force` of
  a fork is no longer needed when the original serves its consumers.
- `replace ORIGINAL --with FORK` requires ORIGINAL to be installed (`not_installed`);
  `replace ORIGINAL --back` with ORIGINAL not installed drops the binding (was
  `not_installed`), refusing with `has_dependents` unless `--force` when consumers would
  break, and prints `ORIGINAL switched back from FORK (ORIGINAL is not installed; broken:
  …)`; its `ReplacementPlan` has `installed_version: None` and an empty `version`.
  `Manager::switch_back(original, confirm, force)` (FFI `switchBack(original, confirm,
  force)`): consumers a missing or disabled original does not serve refuse with
  `has_dependents` unless `force`. Migration: install the original before replacing it;
  pass `force` to switch back anyway.
- Uninstall is bound to its plan: `Manager::uninstall(id, confirmed: &[LostWork], force)`
  replaces `uninstall(id, confirm)` and `uninstall_with(id, confirm, force)` (both
  removed) and refuses with `local_changes` listing the work not in `confirmed`;
  `Error::HasDependents` gains `lost_work` (the plan's, for uninstall; empty otherwise)
  and `UninstallPlan::refusal()` returns that error (or, via the new
  `UninstallPlan.dependency_conflict`, the `dependency_conflict` of a reactivated
  original, before any lost-work question), so the CLI prints dependents and
  lost work (`  - item` lines) in one refusal without calling `uninstall`. CLI `--force`
  confirms the plan's items. FFI: `uninstall(id, confirmed, force)` (was `confirm`),
  `PmError.HasDependents.lostWork`, new free function `uninstallRefusal(plan)`.
  Migration: pass `plan.lost_work` (FFI `plan.lostWork`) as `confirmed` after the user
  agreed. The `local_changes` messages of uninstall end `; confirm the listed lost work to
  remove it` (was `…; discarding them requires force`), the other refusals `; commit or
  discard the local changes first` (was `keep or remove them first`).
- `Manager::forget`/`forget_with` return `ForgetResult { clone_kept, dist }` (was
  `DistSync`; FFI `forget` the same record with `cloneKept`); the CLI prints `ID forgotten
  (no clone; record and dist removed)` when the clone was gone. Migration: read `.dist`.
- A replacing package is always enabled: a `state.json` binding an original to a disabled
  package (produced by gitpm ≤0.5 `disable --force` or a hand edit) is refused with
  `invalid_data`, and the `replace --back` line always ends `; FORK stays installed and
  enabled (…)`. Migration: remove the entry from `replacements` in `state.json`.
- No tag policing: a moved (retagged) or deleted release tag no longer fails `update`,
  `update all`, an install or update of a sibling in a shared clone, or `catalog-update`
  with `version_conflict` ("release tag … changed commit", "release commit changed for
  installed version", "latest release is older than installed version; downgrading
  requires an explicit ref", "release ID@V changed; refusing to replace published
  version" are gone). Release-mode updates compare versions only and keep the installed
  commit; a moved or retracted installed release is up to date with a warning in
  `RevisionChange.warnings` and the new `Unchanged.warnings` (FFI `Unchanged.warnings`).
  `catalog-update` never replaces a published record: it keeps it and reports
  `UpdateSourcesReport.warnings` (`release ID@V: tag now points to COMMIT; the published
  record is kept`, `release ID@V differs from the published record; the published record
  is kept`). `Error::NoPreviousVersion` gains `commit` (FFI `PmError.NoPreviousVersion
  .commit`), set when the previous revision's commit is gone from the clone. Rollback
  counts a tag the origin had with the same object at the last fetch as published (clones
  record them under `refs/origin-tags/`), as update does, so a fork's retained upstream
  tags no longer block it. Migration: scripts that expected `version_conflict` on a
  retagged release read the warning instead. An explicit `--ref TAG|COMMIT` with the
  installed version at another commit is accepted and pinned ("upstream changed without
  a version bump" is gone; a moved release tag of that version warns `release tag ID@V
  now points to COMMIT`).
- Protected catalogs ([Verified catalogs](#verified-catalogs)): only a catalog whose
  top-level `catalog.json` has `"verified": true` binds packages. An unverified catalog no
  longer records `packages.<id>.catalog` on `install-catalog` (or for pulled
  dependencies), so its packages update from their origins and the one-catalog rule
  ignores it; the refusal now says "mixing verified catalogs is not supported".
  `catalog-update` into a verified catalog is refused with `invalid_argument`. Packages
  from a verified catalog update only to its versions, hash-checked (`catalog` on a
  mismatch); `update ID --detach` drops the protection. Library: `Manager::update_with(id,
  target, UpdateOptions { detach })`, `RevisionChange.detached`,
  `UnchangedReason::NotInCatalog { version }`, `UnchangedReason::CatalogUnavailable {
  reason }`, `check::CATALOG_NO_LONGER_VERIFIED`, `catalog::is_verified`,
  `CatalogOverview.verified`, `CheckStatus::UpstreamNotInCatalog`,
  `CheckRow.origin_latest`, `CheckRow::message()`, `CheckReport.catalog_verified`; without
  a given catalog `check` compares a protected package with its recorded catalog. FFI:
  `update(id, target, detach)` (default `false`), `RevisionChange.detached`,
  `UnchangedReason.notInCatalog(version)`, `UnchangedReason.catalogUnavailable(reason)`,
  `CatalogOverview.verified`,
  `CheckStatus.upstreamNotInCatalog`, `CheckRow.originLatest`,
  `CheckReport.catalogVerified`. `info` prints `Verified: yes|no …`, the `check` header
  `Catalog: LOCATION (verified)` or `(unverified: discovery only)`. Migration: add
  `"verified": true` to a catalog you vouch for and register its versions with `register`;
  a record that names an unverified catalog (written by an earlier gitpm) is still
  protected and its updates fail with `catalog` ("… is no longer verified") until you run
  `update ID --detach` (or verify that catalog).
- Dependency requirements follow npm, not Cargo ([Dependencies](#dependencies)): a bare
  version `1.2.3` means exactly 1.2.3 (was `^1.2.3`); `^`, `~`, `*` and space-separated
  comparators (`>=1.3.0 <2.0.0`, AND) remain; commas (`>=1.3, <2`), bare partial versions
  (`1.2`), wildcards (`1.*`), `||` and hyphen ranges are refused with
  `invalid_manifest`. Migration: write `^1.2.3` where a bare `1.2.3` meant "compatible",
  `^1.2` for `1.2`, `>=1.3.0 <2.0.0` for `>=1.3, <2`. `fork` keeps writing rewritten
  requirements as `^V`; `init` writes no dependencies.
- Versions may carry a pre-release (`1.3.0-beta.1`, SemVer order: `1.0.0-alpha <
  1.0.0-beta < 1.0.0`) in manifests, `--version`, catalogs and release tags
  (`ID@1.3.0-beta.1`); build metadata (`+build`) stays `invalid_version`. Pre-releases
  are opt-in ([Versions and pre-releases](#versions-and-pre-releases)). Library:
  `catalog::version_key` returns a `semver::Version` (was a `(u64, u64, u64)` tuple);
  `deps::Range` is gitpm's own pre-release-aware version set (was `pubgrub::Ranges`);
  new `deps::follows`, `deps::is_major`, `deps::is_major_text`,
  `catalog::newest_eligible`, `catalog::newest_prerelease`,
  `releases::Discovery::offered`; `PackageSummary.latest` is the newest version that is
  not a pre-release.
- `update ID` and `update all` cross major versions and mark them: `RevisionChange.major`
  (also on each `UpdateAllPlan.updates` entry) and the CLI line `ID updated 1.2.3 → 2.0.0
  (major)`; `check` rows gain `major` (LATEST shown as `2.0.0 (major)`) and `prerelease`
  (a newer pre-release that is not offered; the status ends with `; pre-release V`).
  FFI: `RevisionChange.major`, `CheckRow.major`, `CheckRow.prerelease`. Migration: hosts
  that parse these lines accept the ` (major)` suffix.

## Working without a catalog

A catalog is optional; the quickstart up to the catalog ran without one. `install GIT_SOURCE` (and `download GIT_SOURCE`) records
the source (a local path canonicalized) as `packages.<id>.source`, and `update ID` and
`update all` fetch the release tags of that recorded origin directly. `list`, `rollback`,
`enable`/`disable`, `forget`/`restore`, `uninstall`, `fork`, `replace` and `sync-dist`
never read a catalog. One limit: dependencies are not fetched. A direct install, or an
update whose new version adds a dependency, accepts only dependencies that are already
installed (and enabled, unless downloading) and otherwise fails with
`dependency_conflict` naming the missing one ("no catalog configured").

`check` needs no catalog either. Every installed package gets one row: from the catalog
when one is given and lists the package (column `SOURCE` `catalog`), otherwise from its
recorded origin (`origin`): `git ls-remote --tags --heads` of `packages.<id>.source`, read once per
source, without fetching or changing the clone. The newest `ID@VERSION` tag counts by name
(its manifest is verified only when updating). Origin rows say `up to date`, `update
available`, `newer than origin`, `no releases on origin` or `origin unreachable` (with a
hint such as "The Git server is unreachable. …"; git's own output is never shown), or
`version cannot be compared` for a malformed installed version. An unreachable origin only
marks its own row; `check` still exits 0. Tags are compared by name: an origin tag of the
installed version that now points to another commit still counts as `up to date`. Only a
Git URL (`https`, `http`, `ssh`, `git`, `file`, `host:path`) or an absolute path is queried,
with remote helpers (`ext::`, `fd::`, any `::`) never allowed; any other recorded source
gives `origin unreachable: source not queried: …` without starting git. A pinned package
is `pinned (REF)` and not queried; a branch-mode package is compared by commit with the
HEAD of its branch on the origin.

```console
$ gitpm check
Workspace: gitpm-workspace
Catalog: none (each package's origin)

PACKAGE                  INSTALLED      LATEST         ENABLED  SOURCE   STATUS
acme/greeter             0.1.0          0.2.0 (major)  yes      origin   update available

Updates: 1. Nothing was changed.

Available updates:
  acme/greeter: 0.1.0 → 0.2.0 (major)
```

A fork records the original's source in `forkOf.source`, so `check` shows the original's
newest release without a catalog too: `me/hello: fork of acme/hello (base 1.0.0); original
at origin: 1.1.0 — newer than the fork base` (or `original: unreachable (REASON)`).

Catalog commands (`info`, `catalog`, `catalog-update`, `register`) and
`install-catalog` need a catalog.

## Workspace layout and `dist/`

A new workspace root may be empty or contain only `.DS_Store`; Finder metadata is
preserved. Any other content without `state.json` is refused with `workspace`.

```text
state.json              {"format": 3, "packages": …}: installed packages, replacements, forgotten records
packages/<ns>/<name>/   own clone of a root package
repos/<name>-<hash>/    shared clone of a multi-package source
dist/<ns>/<name>/       deployed files of an active package
.pm-txn/                an interrupted operation (finished or discarded on open)
```

`dist/<namespace>/<name>/` exists exactly when the package is installed, enabled and not
replaced by another package (`State::is_active`). A host loads every package folder under
`dist/` and needs no gitpm knowledge; `list` still reports the full status (installed,
`enabled`, `replacements`, `resolved[].replacing`) for a UI. Each `resolved[]` entry (and
the FFI `InstalledPackage`) has `active`: exactly this rule, so a host using `list` need
not derive it. `enabled` is not `active`: a replaced package stays `enabled: true` with
`active: false`. `list --provides SERVICE` keeps inactive packages (it is a status view).

- Every deploy (install, update, rollback, restore, `replace --back`, enable, reconcile)
  copies `<path>/dist` of the resolved commit from the clone's Git objects (one
  `git archive`), never the working tree: uncommitted edits in a clone are not deployed,
  nested folders and the executable bit are kept, symlinks are refused, and a revision
  without that folder is refused (`invalid_manifest`, "release … has no dist/ folder")
  instead of deploying an empty folder.
- `replace X --with Y` removes `dist/X/`; nothing of `Y` is copied there; `dist/Y/` stays.
  `X` must be installed (`not_installed`) and `Y` enabled (`invalid_argument`: "Y is
  disabled; enable it first"), so a replacement never leaves nothing loaded. `replace X --back` deploys `dist/X/` again from
  `X`'s clone at its installed commit (`packages.X.current.commit`, folder `path` of a
  multi-package clone), so a shared clone whose HEAD is another package's revision needs
  no checkout. `Y` stays deployed: two packages providing the same service is the host's
  concern.
- `disable X` removes `dist/X/`; `enable X` deploys it again the same way. Disabling a
  package that replaces another drops the binding and deploys the original again when
  it is active (a later `enable` does not bring the binding back); see [Removing a
  replacement](#removing-a-replacement) for when it is refused. `enable X` is refused
  (`dependency_conflict`, `dist/X/` not created) while a dependency of X is not
  satisfied by an active package, unless `--force`.
- `download`, and `install`, `update` and `rollback` of a replaced or disabled package,
  change state and clone only; its `dist/` appears on `replace --back` / `enable`. `fork … --replace` ends
  with the fork deployed and the original's `dist/` removed.
- `forget X` removes `dist/X/` (the clone stays); `restore X` deploys it when it is
  active (a package forgotten while disabled stays disabled). Disabling, forgetting or
  uninstalling a replacement drops its bindings, so each active original it replaced is
  deployed again. Uninstalling or forgetting the original `X` keeps the binding `X → Y`:
  installing `X` again leaves it replaced (no `dist/X/`) until `replace X --back`.
- Every change goes through the same transaction as the state, so a crash leaves state
  and `dist/` consistent; empty `dist/<namespace>` folders are removed.

Every mutating command (install, update, rollback, replace, enable, disable, forget,
restore, uninstall, fork `--replace`) reconciles all of `dist/` with the state in its
transaction: it deploys the missing folders of active packages and removes those of
inactive ones (disabled, replaced, forgotten or retained clones); folders of no known
package are left alone (the CLI warns about them on stderr), an existing folder of an
active package is not rewritten. A namespace path under `dist/` that is a symlink or a
file is reported the same way and nothing is written below it. Active packages below
it are skipped with `Warning: ID: dist not deployed (dist/<namespace> is not a folder)`;
unrelated commands succeed, while an operation deploying or removing a package below
that path fails with `workspace`. An active package whose
clone or installed commit is gone is skipped, so it does not block an unrelated command;
the CLI warns `Warning: ID: dist not deployed (clone missing); uninstall or forget ID to
drop its record` (both drop a record whose clone is gone without asking; `sync-dist` never
re-clones), decided by `SkippedDist.clone_missing`; a missing installed commit gets only
`Warning: ID: dist not deployed (installed commit missing from its clone)`.
Any other deploy error fails the command. Every mutating result carries this report as
`dist` (`DistSync`: `deployed`, `removed`, `unmanaged`, `skipped`; `enable`, `disable`,
`forget` and `uninstall` return it directly); the CLI prints its warnings from there and
an FFI host reads the same fields.
Read-only commands, `list` and `check`, never reconcile `dist/` and open nothing: they
read `state.json` without taking the lock, creating the workspace or completing an
interrupted commit (the next command that opens the workspace does), so `list` works
while another process holds the workspace.
`gitpm sync-dist [--dry-run]` (`Manager::sync_dist`, FFI `syncDist`) does only the
reconcile, as an explicit repair. It syncs and commits every package it can; when an
active package stays undeployed it then fails with `not_deployed` ("N package(s) not
deployed: IDS", the report in `Error::NotDeployed.sync`), exit status 1.
This includes packages below a file or symlink at `dist/<namespace>` (previously
`sync-dist` exited 0 for them). The CLI hint tells you to remove or rename that path
and run `gitpm sync-dist` again; uninstall/forget advice applies only to a missing clone.

## Multi-package repositories

One repository can hold several packages. A root `gitpm.json` lists the package folders
as glob patterns (as in npm or Cargo workspaces); each folder has its own `plugin.json`
and committed `dist/`:

```text
gitpm.json                {"packages": ["packages/*"]}
packages/store/plugin.json  {"id": "tilr/store", "version": "1.2.0"}
packages/store/dist/…
packages/theme/plugin.json  {"id": "tilr/theme", "version": "0.4.0"}
packages/theme/dist/…
```

- Patterns are relative folders of `/`-separated segments; `*` matches within one segment
  (`packages/*`, `apps/gitpm-*`, or a literal `tools/cli`). No `**`, `..`, absolute paths
  or hidden folders (no segment may start with `.`, so `.cache` and `.*` are refused, and
  `*` never matches a hidden folder). Without `gitpm.json` the repository root is the only
  package, exactly as before; with it, a root `plugin.json` is not a package.
- Every package needs a `namespace/name` ID, unique in the repository (in any letter
  case), a `plugin.json` file (not a symlink) and a committed `dist/` folder (not a
  symlink); packages cannot nest. Deployed files must be plain files, as for a root
  package. Only the package being installed, updated or registered must be valid at its
  commit: a defective sibling folder (no `dist/`, bad ID, …) does not block it
  (`catalog-update` reports it as a warning and publishes the other packages).
- Releases are per package: tag `ID@VERSION` (`tilr/store@1.2.0`), so the packages of one
  repository release independently. The package set is read from `gitpm.json` at the
  tagged commit, so a package may move to another folder between releases. Other tags
  (`vVERSION` included) are not releases and are ignored without a warning; a newest
  tag whose commit lacks its package fails the install clearly.
- `gitpm install REPO ID` (or `--package ID`) installs one package. Without an ID a
  repository with several packages is refused with the list of its packages (`tilr/store
  (packages/store), tilr/theme (packages/theme)`); one with a single listed package
  needs none; an `--ref ID@VERSION` tag names the package itself. For a single-package
  repository a given ID must match its manifest. `--ref` and `--dev` (the origin's default
  branch) work per package, and each package records its own mode.
- Workspace: the packages of one source share one clone, `repos/<name>-<hash of the
  source>`; state records `clone` (the cache key of that clone) and `path` (the package
  folder) per package, and only `<path>/dist` is deployed to `dist/<namespace>/<name>`.
  Updating, rolling back or uninstalling one package leaves the other packages alone; the
  clone is removed with the last package that uses it (its local work is guarded then,
  as for any clone). Since the clone has one HEAD for all its packages, HEAD only counts
  as moved when it is neither an installed revision of one of them nor a tagged commit;
  when a package is uninstalled or forgotten, HEAD moves to the installed revision of a
  remaining one (working-tree changes are kept; if they conflict with that revision the
  operation is refused). A root package keeps its own `packages/<id>` clone, also when it
  later moves into a package folder (`path` set, `clone` absent).
- Shared clones keep `refs/gitpm/<namespace>/<name>/current` and `.../previous` for every
  package that uses them (registered or forgotten), updated with each change and deleted
  with the package, so an upstream rewrite, a pruned tag or `git gc` never loses another
  package's installed or rollback revision. Fetching takes the origin's tags as they are
  (moved ones overwritten; `update` also drops deleted ones); a moved release tag of an
  installed package never blocks another package of the clone (each keeps its commit).
- Forgetting a package of a shared clone keeps its record in `state.json` (`forgotten`,
  as for every forgotten package): it still holds the clone (its `dist/` is removed by
  `forget`), `restore` records it again as it was (revision, `enabled`, mode; the clone is
  checked out at the recorded commit when its HEAD is another package's revision) and
  deploys it when active, and `uninstall` of the forgotten package
  removes the record and, if it was the last user, the clone. Installing a forgotten ID
  again is refused until it is restored or uninstalled (as for a forgotten root package).
  If the clone is gone, `uninstall` drops the record, and `restore` says so.
- Catalog: a release of a package carries `"path": "packages/store"` next to `source` and
  `commit` (absent for a root package). `catalog-update` publishes every package listed in
  `gitpm.json` at the source's HEAD from its `ID@VERSION` tags (one source line, all
  packages); `register SRC --ref REF --package ID` registers one (an `ID@VERSION` ref names
  it).
- Authoring: `gitpm init NAMESPACE/NAME --into DIR` adds a package with release 0.1.0
  to a repository with `gitpm.json`; an existing repository without `gitpm.json` gets that
  file (`{"packages": ["packages/*"]}`), unless its root already is a package (moving it
  into a folder is up to you); a folder that does not exist becomes a single-package
  repository instead, or with `--multi` a multi-package one (`gitpm.json` and
  `packages/NAME`; also for an empty folder that is not a repository yet), so later
  `init --into DIR` calls add packages to it. `--no-release` makes only the commit, no
  tag; `gitpm commit DIR 0.1.0 [--package ID]` releases it later. The folder is `PREFIX/NAME` for the first `PREFIX/*` pattern, else
  `packages/NAME` (added to `gitpm.json` when no pattern matches it): folders are named
  after the name part of the ID, so `a/ui` and `b/ui` cannot share a repository (the
  second `init` is refused naming the package that has the folder). Only `gitpm.json` and
  the new folder are committed; `gitpm.json` must have no uncommitted changes. If only the
  tag fails, the commit stays and the error names the tag to create. gitpm never writes
  a Git identity into an existing repository: without configured `user.name` and
  `user.email` (or author and committer identity environment variables), `commit`,
  `init` into it, and adding to an existing fork fail with `invalid_argument` and
  `no Git identity configured; set user.name and user.email`. Git's inferred login
  name and host email are not accepted. A retry that only creates a failed release
  tag needs no identity. Only a
  repository it creates itself (a new `init` folder, a new fork) gets a local demo identity
  when the user has none. `gitpm commit REPO VERSION --package ID` releases one
  package: like for a root package it stages ALL changes of the repository (not only
  that folder), commits and tags `ID@VERSION`.
- `gitpm fork ID [ID...] --ns NAMESPACE --into DIR` forks installed packages of one
  repository into the fork repository `DIR`; each gets the ID `NAMESPACE/<same name>`.
  One package is the one-package case of the same command. All IDs must be installed
  from the same repository (otherwise the error lists each ID with its source).
  - `DIR`: has a `gitpm.json` → the packages are added to it; does not exist → the fork
    is created there; anything else is refused, as are a `DIR` inside the workspace, with
    `..`, the source repository itself or a folder inside it. A failed new fork leaves no
    folder behind (parents it created included).
  - New fork: a copy of the source repository with history and local edits (upstream
    remote = the source), on a new `fork` branch (attached HEAD) starting at the newest
    installed revision of the forked packages (the others must be its ancestors); a
    package installed at an older revision gets its folder from that revision (refused
    if that folder has local edits in the clone, which belong to another revision). The
    folders of packages that are not forked are **removed** from the tree, not just
    unlisted, and `gitpm.json` lists exactly the forked folders, so the fork cannot
    release a sibling ID even after someone edits `gitpm.json`. One commit `Fork A, B as
    me/A, me/B` records it (hooks are not run in this repository gitpm creates); local
    edits stay uncommitted. A root package (no
    `gitpm.json`) stays the root of its fork: only its `plugin.json` changes (local
    commits on top of its installed revision are kept), and it cannot be added to a
    multi-package repository.
  - Adding to an existing fork: the package folders (files of the installed revisions,
    plain files only) are added at their original folders (refused if a folder or ID is
    already there) and to `gitpm.json` unless a pattern covers them, in one commit of only
    those paths. `gitpm.json` and the manifests that change must have no uncommitted
    changes. Remotes and other files are left alone; nothing temporary is written into
    the repository, and a failed commit (a hook, say) restores it.
  - Dependencies: a dependency on a package forked in the same command, or (when adding)
    on an original that a package already in the repository forks (`forkOf` or
    `replaces`), is rewritten to the fork; packages already in the repository that depend
    on a newly forked original are rewritten too. The requirement is kept when the fork's
    version (its manifest version, or `--version V`) satisfies it, else it becomes `^V`;
    each rewrite is printed (`Dependency: me/panel now depends on me/theme ^1.0 (was
    tilr/theme)`). Dependencies on packages that are not forked keep the original ID. A
    manifest that depends on both an original and its fork is refused.
  - `--replace [--version V] [--force]`: one release commit of the forked folders (with
    `version` set in each manifest when `--version` is given) and one tag
    `NAMESPACE/NAME@VERSION` per package, then each package is installed (dependencies
    first; a dependency cycle between the forked packages is refused before anything is
    created) and replaces its original. If the release fails, nothing is installed. A
    failed install or replace of one package does not stop the others; the fork is never
    removed, the command prints per package what failed and how to finish or undo, and
    exits with status 1 and the first error's kind.

```console
$ gitpm fork tilr/theme tilr/panel --ns me --into forks/me-packages
Fork created: /…/forks/me-packages
Git history and local edits are kept; the original repository is the upstream remote.
Only the forked packages are left in the tree and in gitpm.json.
me/theme forked from tilr/theme (packages/theme)
me/panel forked from tilr/panel (packages/panel)
Dependency: me/panel now depends on me/theme ^1.0 (was tilr/theme)
…
$ gitpm fork tilr/dock --ns me --into forks/me-packages   # add one more later
Packages added to: /…/forks/me-packages
me/dock forked from tilr/dock (packages/dock)
Dependency: me/dock now depends on me/theme ^1.0 (was tilr/theme)
```

```console
$ git init -q tilr-packages
$ gitpm init tilr/store --into ./tilr-packages
$ gitpm init tilr/theme --into ./tilr-packages
$ gitpm install ./tilr-packages
Error: repository has 2 packages at commit 4b377fe1c0de; choose one by ID: tilr/store (packages/store), tilr/theme (packages/theme)
$ gitpm install ./tilr-packages tilr/store && gitpm install ./tilr-packages tilr/theme
installed tilr/store@0.1.0 from /…/tilr-packages
installed tilr/theme@0.1.0 from /…/tilr-packages
$ echo 'export const v = 2;' > tilr-packages/packages/store/dist/index.js
$ gitpm commit ./tilr-packages 0.2.0 --package tilr/store
tilr/store committed 0.2.0 (local commit and tag tilr/store@0.2.0)
$ gitpm update tilr/store       # tilr/theme stays at 0.1.0
tilr/store updated 0.1.0 → 0.2.0 (major)
```

Settings: each has a flag, an environment variable and a default (`gitpm --help` lists
all commands, `gitpm COMMAND --help` one). There is no config file; the old `PM_*` variables are ignored.

| Flag | Variable | Default |
| --- | --- | --- |
| `--root DIR` | `GITPM_ROOT` | `./gitpm-workspace` |
| `--catalog LOC` (folder, `catalog.json` or HTTP(S) URL) | `GITPM_CATALOG` | none (optional, see [working without a catalog](#working-without-a-catalog)): catalog commands and `install-catalog` ask for it |
| `--sources FILE` | `GITPM_SOURCES` | `./sources.txt` |

Publishing a catalog is up to you: it is a folder of static files, so any static host
works (`catalog-update` and `register` write only to a local folder; HTTP(S) catalogs are
read-only).

When you upload an updated catalog, go in an order that never shows readers a half-updated
tree: new `versions/<V>/release.json` files first, then each package's
`namespaces/<ns>/<name>/plugin.json` index, then the directory files from the deepest level
up, and the root `catalog.json` last. Never overwrite a `release.json` that is already
published, never drop versions from an index, and do not delete objects as part of an update.

## Package metadata

`plugin.json` may describe the package with four optional keys:

| Key | Type |
| --- | --- |
| `title` | string |
| `description` | string |
| `provides` | string or array of strings (services the package provides; a string is read as a one-element array) |
| `tags` | array of strings |

gitpm only stores and reports them; it never interprets them (no exclusivity, no
"one theme at a time": that is up to the host, which can, for example, group installed
packages into slots by what they `provide`, without cloning or running anything).
Authoring is strict, everything else lenient: `commit` refuses a key of another type
(`invalid_manifest`); installing, updating, rolling back, forking, publishing to a
catalog and listing treat such a key (and `null`) as absent, so such packages keep
working (`install`/`update` warn: `Warning: ID VERSION: plugin.json tags
must be an array of strings; ignored`; the library reports it in
`InstallResult.warnings`/`RevisionChange.warnings`, and
`Manifest::metadata_problems()`/`metadata_strict()` give the check). The keys stay in the file as
they were: `commit`, `fork` and `fork --replace` rewrite `version`, `id`, `forkOf`,
`replaces` and dependencies but keep the metadata (also the string form of `provides`).
`init` writes `"title": "NAME"`.

Shape rule, everywhere gitpm reports metadata (`list` `resolved[]` and
`packages.<id>.metadata`, `info`/`browse` `PackageSummary`, `CatalogRelease`, the FFI
records): all four keys are always present; an absent string (`title`, `description`) is
`null` (`Option<String>`, Swift `String?`), an absent list (`provides`, `tags`) is `[]`
(`Vec<String>`, `[String]`).

- `gitpm list`: each `resolved[]` entry has `title`, `description`, `provides` and
  `tags` of the installed revision, taken from the state record only (the clone is
  never read); state keeps them per package (`packages.<id>.metadata`, same shape). A
  record without `metadata` reports the absent shape.
- `gitpm list --provides SERVICE`: only the packages whose `provides` contains `SERVICE`.
  It filters `packages` and `resolved` only; `replacements` and `forgotten` are printed
  unfiltered (they describe the workspace, not single packages).
- Catalog: a release (`release.json`) has the whole manifest, so `catalog-update` and
  `register` publish the metadata with it. `info` and `browse` (`PackageSummary`) show
  the metadata of each package's latest release; a missing or broken latest
  `release.json` only leaves that package without metadata. The catalog package index
  (`namespaces/<ns>/<name>/plugin.json`) carries no metadata. Cost: the overview reads
  one `release.json` per listed package (at most 40), which on an HTTP(S) catalog is one
  extra request each.
- FFI: `InstalledPackage`, `PackageSummary` and `CatalogRelease` have `title`,
  `description` (`String?`), `provides` and `tags` (`[String]`).

```console
$ cat packages/theme/plugin.json
{"id": "tilr/theme", "version": "1.0.0", "title": "Theme", "provides": "theme", "tags": ["ui"]}
$ gitpm list --provides theme | jq '.resolved[] | {id, title, description, provides, tags, clone}'
{
  "id": "tilr/theme",
  "title": "Theme",
  "description": null,
  "provides": ["theme"],
  "tags": ["ui"],
  "clone": "tilr-packages-4b377fe1c0de1234"
}
```

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
- Catalog HTTP responses are limited to 16 MiB, including streams without
  `Content-Length`; oversized responses fail with `http`.
- All subprocesses (system `git`, `curl`) go through one internal module (`exec`), the
  place to switch to gix/ureq later.
- No backward compatibility before 1.0: package IDs are always `namespace/name`, release
  tags always `ID@VERSION`, catalogs always the v2 tree; there are no aliases,
  migrations or compatibility readers, and the FFI promises no stability. Every breaking
  change is listed under "Changed in X.Y".
- `state.json` carries `"format"` (`gitpm::STATE_FORMAT`). A gitpm reads and writes
  exactly its own format: any other one (or a file without `format`) is refused with
  `unsupported` and never rewritten, in either direction (no downgrade, no migration). A
  workspace of another format is re-created: install its packages again into a new
  workspace root. The format is bumped with every change an older gitpm would misread.

  | gitpm | state format |
  | --- | --- |
  | 0.3 and earlier | none (no `format` key), packages under `plugins` |
  | 0.4, 0.5 | 2 (`clone`/`path`, `forgotten`), packages under `plugins`, clones in `plugins/` |
  | 0.6 | 3: packages under `packages`, own clones in `packages/` |

## API overview

```rust
use gitpm::{Manager, Options};

let pm = Manager::open_with("/path/to/workspace", Options { catalog: Some(url) })?;
pm.install("git@github.com:acme/demo1.git")?;          // latest release tag
use gitpm::{InstallTarget, Target};
pm.install_package(src, None, InstallTarget::Ref("acme/demo1@0.2.0".into()))?; // pinned
pm.install_package(src, None, InstallTarget::Dev)?;    // follows the origin's default branch
pm.install_package(src, Some("tilr/store"), InstallTarget::Release)?; // one package of a gitpm.json repo
pm.install_from_catalog("acme/demo1", None)?;          // exact catalog release, hashes verified
pm.download(src, None, InstallTarget::Release)?;       // recorded disabled, deps too, nothing deployed
pm.download_from_catalog("acme/demo1", None)?;         // the same from the catalog
pm.enable_with(id, /* force: enable over inactive dependencies */ false)?;
pm.update("acme/demo1")?;                              // by the recorded mode (pinned: unchanged_reason)
pm.update_to(id, Target::Release)?;  pm.update_to(id, Target::Ref("feature".into()))?;
pm.update_with(id, Target::Recorded, UpdateOptions { detach: true })?; // leave a verified catalog
pm.rollback(id)?;  pm.enable(id)?;  pm.disable(id)?;  pm.forget(id)?;  pm.restore(id)?;
let plan = pm.uninstall_plan(id)?;                    // deletes + lost_work + dependents
if let Some(refusal) = plan.refusal() { return Err(refusal) } // has_dependents with lost_work
let confirmed = if user_said_yes { plan.lost_work.clone() } else { Vec::new() };
let dist = pm.uninstall(id, &confirmed, /* force: ignore dependents */ false)?;
// Every mutating result carries its dist report (`dist`, or the returned `DistSync`).
// Forks: the fork repository is always explicit (new folder, or a repo with gitpm.json).
pm.fork_in_namespace(id, Path::new("/work/demo1-fork"), "me")?;   // sets forkOf + replaces
let done = pm.fork_replace(id, Path::new("/work/demo1-fork"), "me", None, confirm)?; // + release, install, replace
if let Some(step) = done.failed { /* fork folder kept; done.error says why */ }
let repo = Path::new("/work/forks/me-packages");               // created, or added to
let fork = pm.fork_packages(&["tilr/theme", "tilr/panel"], repo, "me")?; // one repo, deps rewritten
let done = pm.fork_packages_replace(&ids, repo, "me", Some("2.0.0"), confirm)?; // per-package results
if let Some(error) = done.error() { /* fork kept; done.packages[i].failed says where */ }
let pending = done.pending();   // replacements that need confirmation (incompatible consumers)
pm.fork_replace_resume(&pending, /* confirm */ user_said_yes)?;
let plan = pm.replace_plan("vendor/base", "mine/base")?;       // consumers + incompatible
pm.replace("vendor/base", "mine/base", /* confirm */ !plan.incompatible.is_empty() && yes)?;
pm.switch_back("vendor/base", confirm, /* force: consumers left broken */ false)?;
pm.set_enabled_with(id, false, force)?;  pm.forget_with(id, force)?;
let sync = pm.sync_dist()?;   // dist/ = active packages; Err(NotDeployed) if some cannot be
pm.list()?;  pm.state()?;  pm.check()?;  pm.browse_catalog()?;  pm.catalog_release(id, None)?;
// Read-only views without a Manager (no lock, nothing created):
gitpm::list_with(root, &gitpm::ListOptions { provides: Some("theme".into()) })?;
gitpm::check::check(root, Some(catalog))?;
```

Modules with free functions (no workspace lock needed):

| Module | Functions |
| --- | --- |
| `list` | `list_with(root, &ListOptions)` → `ListView` (raw state + `resolved`; `provides` filter) |
| `catalog` | `browse` (`CatalogOverview.verified`), `is_verified`, `list`, `links`, `lookup`, `selected`, `register`, `register_build`, `register_package`, `update_sources` (progress callback), `hash_files`, `version_key`, `validate_location`, `ensure_publishable` |
| `check` | `read_state` (does not create/lock), `compare`/`compare_in` (run `git ls-remote` for packages the catalog does not list), `check(root, catalog: Option)` → `CheckReport` (`catalog_verified`); `CheckStatus::message()`, `CheckRow::message()`, `CheckSource` |
| `auth` | `check(source, &AuthOptions)` — `git ls-remote` only; refuses credentials in the URL, stores nothing; failures are `auth` |
| `scaffold` | `init(dir, namespace, name)` (the `init --into` rule; the only way to create a package besides `create_package`), `init_with` (`InitOptions { multi, release }`), `create_package` (into a multi-package repo), `commit`, `commit_package` (`Commit { package, version, tag }`), `release_current`, `set_remote`, `shell_quote` |
| `packages` | `config_at`, `layout_at` (packages and defective folders of a commit), `packages_at` (strict), `find_at` (one package of a commit), `read_config`, `packages_in` (working tree), `matches`, `validate_pattern` |
| `identity` | `valid_part`, `qualified`, `split` (`NAMESPACE/NAME`, else `invalid_id`), `release_tag` (no default namespace, no namespace config) |
| `releases` | `discover`, `discover_all` (with the skipped broken tags), `latest` |

### Uninstall: confirmation by plan

`uninstall_plan(id)` is read-only and returns `UninstallPlan { id, registered, deletes,
lost_work, dependents }`. `lost_work` lists `LostWork` items: `Modified`, `Untracked`,
`Ignored`, `HeadMoved` (HEAD is not the installed revision), `UnpublishedCommit`,
`UnpublishedBranch`, `UnpublishedTag`, `Stash`. `uninstall(id, confirmed, force)`
deletes the work listed in `confirmed` (the plan's items the user saw) and refuses with
`Error::LocalChanges { items, .. }` listing any other item, so work created while the
user was asked is never deleted unseen. `dependents` are the consumers that would break
(see [Dependencies](#dependencies)); they refuse with `Error::HasDependents {
dependents, lost_work, .. }` unless `force`, and `plan.refusal()` returns that same error
without calling `uninstall`. A recorded package whose clone is gone has no lost work:
`uninstall` and `forget` drop its record and `dist/` without confirmation (`forget`
keeps no `forgotten` record then). The caller decides how to ask the user. IDs match exactly: on a
case-insensitive file system `Vendor/Module` never resolves to `vendor/module`.

### Versions and pre-releases

A version is SemVer `MAJOR.MINOR.PATCH` with an optional pre-release `-PRE`
(`1.3.0-beta.1`); build metadata is refused. Versions are ordered by SemVer precedence
(`1.0.0-alpha < 1.0.0-beta < 1.0.0-beta.2 < 1.0.0`), and `ID@1.3.0-beta.1` is a release
tag like any other (`gitpm commit REPO 1.3.0-beta.1` creates it, `catalog-update`
publishes it). Pre-releases are opt-in, as in npm:

- `install`, `install-catalog` without `--version`, `update` and `update all` never pick
  one; `check` and `info` show it (`check`: `up to date; pre-release 1.3.0-beta.1`).
- Install one by name: `install-catalog ID --version 1.3.0-beta.1` (release mode), or
  `install SRC --ref ID@1.3.0-beta.1` (pinned; `update ID --release` returns it to
  release mode).
- An installed pre-release follows its track: `update` moves to newer pre-releases of
  the same `1.3.0` or to the newest release (`1.3.0`, or `1.4.0` when that is out),
  never to `1.4.0-beta.1`.

`update` goes to the newest release across a major version, as plugin managers do, and
marks it; the author marks a breaking release only by its number (a new MAJOR; for 0.x a
new MINOR; for 0.0.x any new version). Dependents stay protected by their own ranges: an
update that breaks one is refused with `dependency_conflict`.

```console
$ gitpm update acme/greeter
acme/greeter updated 1.2.3 → 2.0.0 (major)
```

### Joint upgrades

```sh
gitpm update all --dry-run        # resolve and validate, without applying
gitpm update all                  # apply the compatible set atomically
```

The library exposes `Manager::update_all_plan()` and `Manager::update_all()`;
Swift/Kotlin bindings expose the same operations. Both return `UpdateAllPlan`
with `updates`, new dependency `installs`, `kept_back` entries (selected and
latest versions and a `reason`: `constraints`, `missing_dependency` or
`no_longer_replaces`, whose `Display` is the message the CLI prints) and `unchanged`
packages (`{id, version, reason}`; `reason` is `pinned`, `diverged` or `branch_gone`
when the mode keeps a package, `broken_dependency` for a broken consumer), so every
installed package is listed; `update_all` adds the `dist` report. Apply resolves again under the workspace lock; a preview is
not a saved plan and upstream changes can change the result.

The solver considers all installed packages together, using the release tags
from each repository's origin (pre-releases only on an installed pre-release's track) and the installed revision as a fallback. Missing
dependencies are resolved from the configured catalog. It prefers newer versions,
prioritizes installed packages in lexical ID order, and backtracks when a choice
cannot satisfy the whole graph. This is a deterministic compatible solution,
not a guarantee of maximizing the number of upgraded packages. A package may
advance to an intermediate compatible release while its newest release is kept
back. Existing single-package install/update behavior remains unchanged.

No packages are removed or downgraded. Active fork replacements constrain consumers
using each candidate's declared `replaces.version`; disabled flags are preserved
and disabled packages cannot satisfy dependencies of active packages (a disabled
consumer takes them as given). Like dnf, `update all` updates what is healthy and never
fails because of a broken state: a broken consumer (a dependency unsatisfied now, for
example after `uninstall --force` of it) stays at its installed revision, outside the
solve, and is reported in `unchanged` with reason `broken_dependency`; the removed
dependency is never installed again for it, with or without a catalog (`list` and
`check` keep reporting it unsatisfied; `update ID` of it still fails with
`dependency_conflict` when the dependency cannot be installed). The broken-dependency
reason takes precedence over `pinned`, `diverged` and `branch_gone`. A new dependency is
installed enabled only when an active package pulls it; one only disabled packages pull
is recorded disabled and takes disabled providers as given:

```console
$ gitpm uninstall acme/lib --force
acme/lib uninstalled
$ gitpm update all
acme/app skipped at 1.0.0 (dependency acme/lib is not installed)
acme/other updated 1.0.0 → 1.1.0
acme/theme kept back at 1.0.0 (latest 2.0.0: dependency constraints)
1 updates, 0 installs, 1 kept back.
```

A package whose newest release is not selected is kept back with a reason: `dependency
constraints`; `needs DEP REQ, which is not installed and no catalog is configured`; or
`no longer replaces ORIGINAL` (a replacing fork whose newest release or branch commit
dropped `replaces`).
Local changes and unpublished work block the operation; moved or deleted release tags
never do (the package stays, listed in `unchanged` with its `warnings`).
Each package follows its recorded mode: release-mode packages are solved over their
release tags, a branch-mode package gets its branch HEAD as its only new candidate (its
manifest version takes part in the solve, so dependents stay protected; a diverged
history keeps it with reason `diverged`, a branch gone from the origin with reason
`branch_gone`), and a pinned package stays (reason `pinned`).
Explicit refs are for single-package update only.

All selected repositories and payloads are prepared and validated before the
transaction commits. A preparation failure leaves the workspace unchanged;
interrupted commits use the existing journal recovery. A dry run also stages and
validates payloads temporarily, so it can require network access and disk space.

The policy follows the non-removing approach of
[APT upgrade](https://manpages.debian.org/testing/apt/apt.8.en.html), allowing new
dependencies. GitPM uses its existing PubGrub dependency solver; it does not invoke
APT or implement `full-upgrade` removals.

### Dependencies

`dependencies` maps package IDs to npm-style ranges:

| Requirement | Means |
| --- | --- |
| `1.2.3` (or `=1.2.3`) | exactly 1.2.3 |
| `^1.2.3` | `>=1.2.3 <2.0.0` (compatible); 0.x: `^0.2.3` = `>=0.2.3 <0.3.0`, `^0.0.3` = only 0.0.3 |
| `^1.2`, `^1` | `>=1.2.0 <2.0.0`, `>=1.0.0 <2.0.0` |
| `~1.2.3`, `~1.2` | `>=1.2.3 <1.3.0`, `>=1.2.0 <1.3.0` (patch-level) |
| `>=1.3.0 <2.0.0` | comparators (`>`, `>=`, `<`, `<=`) separated by spaces: AND |
| `*` | any release |
| `^1.3.0-beta.1` | `>=1.3.0-beta.1 <2.0.0`, the only way a range admits pre-releases (those of 1.3.0) |

As in npm, a pre-release satisfies a range only when one of its comparators names a
pre-release of the same `MAJOR.MINOR.PATCH`: `^1.2.0` never matches `1.3.0-beta.1`.
Commas, `||`, hyphen ranges (`1.0.0 - 2.0.0`), wildcards other than `*` and bare partial
versions (`1.2`) are refused with `invalid_manifest`. Install resolves them with PubGrub
(`deps` module): installed packages stay at their versions, missing ones are installed
from the configured catalog in the same transaction (`InstallResult.dependencies`);
only active packages satisfy a dependency of an enabled package (a disabled one refuses
install, `enable` and `update ID` with `dependency_conflict`, "DEP is disabled; enable
it"), while `download` takes disabled ones as given (a direct Git install without a
catalog only accepts already installed dependencies). Update and rollback are refused
with `dependency_conflict` when the new version breaks a dependent; new dependencies of
an update are installed. Conflicts carry the solver's explanation naming the packages.
State keeps the edges (`packages.<id>.dependencies`); `list()` reports for each
dependency its provider, version and whether it is satisfied, plus each package's
`dependents`. Disable, forget and uninstall of a package refuse with `has_dependents`
(names in `dependents`) unless `force` when a consumer would break: one whose dependency
resolves to the package, or, for an active fork, one the original does not satisfy once
the binding is dropped (see [Removing a replacement](#removing-a-replacement));
`UninstallPlan.dependents` lists them up front. Already broken consumers (after a forced removal) are reported,
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
confirmation rule). The original must be installed to be replaced (`not_installed`)
and the fork enabled to become the provider; a fork update must keep declaring
`replaces`. Uninstalling or forgetting the original keeps the binding (a reinstalled
original stays replaced until `switch_back`). A replaced original has no `dist/` (see
[Workspace layout](#workspace-layout-and-dist)).

#### Removing a replacement

One rule for `disable`, `forget` and `uninstall` of an active fork: each drops the
binding and deploys the original again when it is active (a later `enable` of the fork
does not restore the binding). They refuse with `has_dependents` unless `force` only
when a consumer would break afterwards: the original is not installed, disabled, or of a
version the consumer does not accept. `dependents` names exactly those consumers; when
the installed original satisfies everyone, no force is needed. The reactivated original
is checked like `enable`: a direct dependency of it that no active package satisfies
refuses with `dependency_conflict` unless `force`. `switch_back` (`replace ORIGINAL
--back`) follows the same rule: consumers a missing or disabled original does not serve
refuse with `has_dependents` unless `force` (CLI `--force`; `confirm` covers only
incompatible versions). For an original that is not installed only the binding is
dropped, and its
`ReplacementPlan` has no `installed_version` and an empty `version` (CLI: `ORIGINAL
switched back from FORK (ORIGINAL is not installed; broken: consumers)`).

### Catalog

A catalog is optional ([working without a catalog](#working-without-a-catalog)):
without one, `browse_catalog`, `catalog_release` and `install_from_catalog` fail with
`invalid_argument` ("no catalog configured"); `check` then reads each package's origin. A configured local location that
does not exist is not "no catalog": `Manager::open_with` and every catalog reader refuse
it with `invalid_argument` ("catalog location PATH does not exist") before anything is
created; only `catalog-update` and `register` create a missing local catalog. A workspace uses one verified catalog
([Verified catalogs](#verified-catalogs), `catalog::is_verified`). `install_from_catalog`
records the canonical location of a verified catalog per package (`packages.<id>.catalog`,
the package is then protected) and refuses to install from a second verified catalog;
an unverified catalog records nothing. `update_to`/`update_with` and `update_all` move a
protected package only to the versions its catalog lists (`UnchangedReason::NotInCatalog`
when the origin is ahead), `UpdateOptions { detach: true }` drops the protection. `check`
compares a protected package with its own catalog when none is given (status
`upstream_not_in_catalog` with `CheckRow.origin_latest` when its origin is ahead), reports
packages from another catalog as `other_catalog` instead of comparing them, and says in
`CheckReport.catalog_verified` whether the given catalog is verified; a package the catalog does
not list gets an origin row (`CheckRow.source` is `CheckSource::Catalog` or `Origin`,
`CheckRow.problem` explains `origin_unreachable`). For a fork it adds
`upstream { id, base, latest, newer, source }`: the original's latest release against the
version the fork started from, from the catalog when it lists the original, otherwise from
the tags of `forkOf.source` (`source` `None` when neither is known; `problem` when
`forkOf.source` cannot be read). `compare`/`compare_in` and `check` run `git ls-remote` for
every package (and fork original) the catalog does not list.

### Error kinds

`not_installed`, `already_installed`, `local_changes`, `locked`, `invalid_id`,
`invalid_manifest`, `invalid_version`, `invalid_argument`, `version_conflict`,
`no_previous_version`, `no_releases`, `repository_not_found`, `workspace`, `catalog`,
`git`, `http`, `auth`, `unsupported`, `dependency_conflict`, `has_dependents`,
`incompatible_replacement`, `not_deployed`, `invalid_data`, `io`.

## Swift bindings

`crates/gitpm-ffi` wraps the library with UniFFI: the Swift module `Gitpm` over the C module
`GitpmFFI` (names in `crates/gitpm-ffi/uniffi.toml`; Kotlin package `gitpm`). Link the static
library and use the generated API:

```swift
import Gitpm

let pm = try PackageManager.open(root: workspace, options: ManagerOptions(catalog: catalogURL))
let plan = try pm.uninstallPlan(id: "acme/demo1")
try uninstallRefusal(plan: plan)   // throws HasDependents (with lostWork) when consumers would break
if plan.lostWork.isEmpty || userConfirmed(plan.lostWork.map(describeLostWork)) {
    try pm.uninstall(id: "acme/demo1", confirmed: plan.lostWork)
}
```

FFI surface: `PackageManager` (open, root, list, browseCatalog, catalogRelease,
install(source, target: `InstallTarget`, package; defaults to nil), installFromCatalog,
download(source, target, package), downloadFromCatalog,
update(id, target: `Target`, `.recorded` follows the recorded mode), updateAllPlan,
updateAll (`UpdateAllPlan` with `unchanged`, each with its `reason`, and `dist`), rollback, enable(force), disable(force),
forget(force), restore, uninstallPlan, uninstall(confirmed, force), fork
(one package; `ForkResult`), forkAndReplace(version, confirm; returns a
`ForkReplacement`: a failure after the fork exists is reported in `failedStep` (the
`ForkStep` enum: release, install, replace), `errorKind`, `errorMessage`, and a
replacement that needs confirmation in `pending`), forkPackages
(`PackageFork`), forkPackagesAndReplace (`PackageForkReplacement`: the fork plus one
`ForkReplacement` per package and the `pending` list), forkReplaceResume(pending,
confirm; `ForkResume`: `applied` plans and the first `failed` one), replacePlan, replace, switchBackPlan, switchBack(confirm, force) (`ReplacementPlan` with
`installedVersion`, `dist`), syncDistPlan, syncDist (`DistSync`: deployed,
removed, unmanaged, skipped)); every mutating call returns its `DistSync` report
(`enable`/`disable`/`uninstall` return it, `forget` returns `ForgetResult { cloneKept,
dist }`, `InstallResult`, `RevisionChange`,
`ReplacementPlan` and `UpdateAllPlan` carry it as `dist`); `InstallResult` and
`RevisionChange` carry `skipped` (broken release tags) and `warnings`, as the CLI prints
them; `RevisionChange.unchangedReason` says why an update left a package (`pinned`);
`InstalledPackage.mode`, `PackageRecord.mode` and `CheckRow.mode` carry the recorded
`Mode`; `force` defaults to false; `InstalledPackage.path` and `CatalogRelease.path` name
the package folder; `InstalledPackage.clone` names the shared clone; `InstalledPackage`,
`PackageSummary` and `CatalogRelease` carry the [metadata](#package-metadata); the fork
records have the names of the library types `gitpm::ForkReplacement`, `PackageFork`,
`PackageForkReplacement`, `PendingReplacement`, `ForkedPackage`, `DependencyRewrite`,
`ForkStep`. Free functions, which read without opening, locking or creating the
workspace: `list(root, provides)` and `readState(root)` (`ListView`: `format`,
`packages`, `replacements`, `forgotten`, `resolved`), `check(root, catalog)`
(`CheckReport`: rows with `source` `.catalog`/`.origin` and `problem`, fork `upstream`
with its `source`, replacements, unsatisfied dependencies; `catalog` may be `nil`), `catalogLinks(location)`,
`authCheck(source, customSsh)`, `browseCatalog`, `catalogRelease`, `describeLostWork`,
`uninstallRefusal(plan)`.
Every CLI command that reads or changes a workspace, reads a catalog or checks access has
its FFI call; `init`, `commit`, `remote`, `catalog-update` and `register` stay library and
CLI functions. Errors are the `PmError` enum (one case per kind, each with `message`;
`id` where the library error has one, `LocalChanges.items`, `HasDependents.dependents`
and `.lostWork`, `IncompatibleReplacement.consumers`, `NotDeployed.sync` (the dist report), and
`Other { kind, message }` for kinds added later). `gitpm-ffi` carries the library's
version; before 1.0 its bindings promise no stability (renames are listed under
"Changed in X.Y").

## Hosts

Tilr's Mac app consumes the library through the UniFFI xcframework: `scripts/build-apple.sh`
builds `GitpmFFI.xcframework` (universal static library + C module) and `Gitpm.swift`, which
the app adds as a SwiftPM `binaryTarget` and a source target (see [Build](#build)).
The host loads every package folder under `dist/` (only active packages are deployed,
see [Workspace layout](#workspace-layout-and-dist)); `syncDist()` repairs `dist/`
explicitly (mutating calls also reconcile).

## Tests

```text
crates/gitpm/src/*.rs                    unit tests in each module's #[cfg(test)] mod tests
crates/gitpm/src/test_support.rs         shared fixtures (temp dirs, hermetic git,
                                         package repos, tree hash), compiled only for tests
crates/gitpm/tests/                      integration tests of the public API
                                         (robustness, uninstall, no_env, dependencies)
crates/gitpm-ffi/src/lib.rs              FFI conversion tests (errors, records, free functions)
crates/gitpm-cli/tests/                  end-to-end tests of the `gitpm` binary
                                         (lifecycle, requirements, monorepo, metadata,
                                         dist_active)
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
removed unless `GITPM_KEEP_TEST_ARTIFACTS` is set (to inspect fixtures after a run).

`#[ignore = "known defect: …"]` tests assert the intended contract and currently fail;
they keep known defects visible (`cargo test -- --ignored` lists them). Remove the
attribute together with the fix.

### Integration tests (Dagger)

`itests/` holds a black-box suite in a Dagger module (engine and CLI v0.21.10): every
libtest of the workspace, clippy, and 638 scenario cases driving the `gitpm` binary built
from the working tree (only `GITPM_*`, loopback HTTP, local Git repositories), each in its
own container. It is an additional, slower suite; `cargo test --workspace` stays the CI gate.

```sh
./itests/dt                                   # everything, on the pb engine (dagger-pb)
./itests/dt --only scenarios --filter uc_015  # one use case
DT_ENGINE=dagger ./itests/dt                  # the same on a local engine (Docker)
./itests/dt coverage                          # spec coverage report, no Dagger
```

With the 1Password SSH agent run `env -u SSH_AUTH_SOCK ./itests/dt`. Results land in
`itests/results/<timestamp>/` (`summary.txt`, `report.html`, logs). Every case is tagged
with the spec requirement it verifies; `./itests/dt coverage` writes
`openspec/coverage/coverage.md` and `coverage.json`. Details, statuses (`XFAIL` for known
defects) and how to write cases: [itests/README.md](itests/README.md).

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
