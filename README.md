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
[metadata](#plugin-metadata); other keys are kept as they are) and a
ready `dist/` folder: the repository root, or one of several folders a root `gitpm.json`
lists ([multi-package repositories](#multi-package-repositories)). Installing clones the
repository into a workspace and copies the package's `dist/`; nothing is built or executed.
Releases are Git tags (`vVERSION` or `namespace/name@VERSION`).
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

Everything below works on local folders: plugin repositories and the catalog are plain
directories, nothing is pushed or uploaded.

A plugin is a Git repository with `plugin.json` and a ready `dist/` folder; a release is
a `namespace/name@VERSION` tag (or `vVERSION` for a legacy plugin without a namespace).
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
  "plugins": {
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
acme/greeter updated 0.1.0 → 0.2.0
$ cat gitpm-workspace/dist/acme/greeter/index.js
export const greeting = "greeter v2";
$ gitpm rollback acme/greeter
acme/greeter rolled back 0.2.0 → 0.1.0
$ cat gitpm-workspace/dist/acme/greeter/index.js
export const greeting = "greeter v1";
$ gitpm update acme/greeter
acme/greeter updated 0.1.0 → 0.2.0
$ gitpm update acme/greeter
acme/greeter up to date (0.2.0)
$ gitpm install ./greeter         # installed already: nothing changes, exit 0
acme/greeter already installed (0.2.0)
```

Installing an installed package again from the same source changes nothing and exits 0:
`ID already installed (VERSION)`, with `; run update` when the source resolves to another
revision (a newer release; `update` moves to it; with `--ref REF` or `--channel dev` it
says `requested --ref REF — run update ID --ref REF`). The same ID from another source is
still refused (`already_installed`). "Same source" means the same string as recorded at
install, after a local path is canonicalized: a `file://` URL against a plain path, or a
URL with and without `.git`, counts as another source.

A catalog is generated from the release tags of the repositories listed in a sources
file (paths are relative to that file). `gitpm init NAMESPACE/NAME --into DIR` (same as
`create`) scaffolds the package with its first release: a new `DIR` becomes a plugin
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
Format: v2
Directories and plugins:
  namespaces/
    acme/
      greeter/
        acme/greeter — latest: 0.2.0, versions: 2
            Git: /…/greeter
$ gitpm init hello --into hello
Error: plugin id needs a namespace: use NAMESPACE/NAME, for example acme/hello
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
Catalog: /…/catalog

PLUGIN                   INSTALLED      LATEST         ENABLED  STATUS
acme/greeter             0.2.0          0.2.0          yes      up to date
acme/hello               0.1.1          0.1.1          yes      up to date

Updates: 0. Nothing was changed.
```

`gitpm fork ID --ns NAMESPACE --into DIR` copies an installed plugin (with its Git
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
me/hello 1.5.0 (as acme/hello 1.0.0) for: …`).

Changed in 0.4: the form `fork ID NAMESPACE/NAME [--into DIR]` (fork folder
`DIR/NAMESPACE/NAME`, default `./NAMESPACE/NAME`) and `init NAMESPACE/NAME` without a
folder are gone; `init --in REPO` is now `init --into DIR`.

Changed in 0.5:

- [Plugin metadata](#plugin-metadata): `title`, `description`, `provides`, `tags` in
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
  | `update ID`, `update all` | `ID updated A → B`, `ID up to date (A)` (`update all --dry-run`: `would be updated`) |
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

## Working without a catalog

A catalog is optional; the quickstart up to the catalog ran without one. `install GIT_SOURCE` records
the source (a local path canonicalized) as `plugins.<id>.source`, and `update ID` and
`update all` fetch the release tags of that recorded origin directly. `list`, `rollback`,
`enable`/`disable`, `forget`/`restore`, `uninstall`, `fork`, `replace` and `sync-dist`
never read a catalog. Two limits:

- Dependencies are not fetched: a direct install, or an update whose new version adds a
  dependency, accepts only dependencies that are already installed and otherwise fails
  with `dependency_conflict` naming the missing one ("no catalog configured").
- `check` compares with a catalog only: with plugins installed it fails without one
  (`check needs a catalog`), and with one it reports a plugin the catalog does not list
  as `not in catalog`.
  To see which newer releases the origins have, run `gitpm update all --dry-run` (it
  changes nothing), or `gitpm install SOURCE` again (`; run update` when the source has
  another revision).

Catalog commands (`info`, `catalog`, `catalog-update`, `migrate-catalog`, `register`) and
`install-catalog` need a catalog.

## Workspace layout and `dist/`

```text
state.json              installed plugins, replacements, forgotten records
plugins/<ns>/<name>/    own clone of a root package
repos/<name>-<hash>/    shared clone of a multi-package source
dist/<ns>/<name>/       deployed files of an active package
.pm-txn/                an interrupted operation (finished or discarded on open)
```

`dist/<namespace>/<name>/` exists exactly when the package is installed, enabled and not
replaced by another package (`State::is_active`). A host loads every package folder under
`dist/` and needs no gitpm knowledge; `list` still reports the full status (installed,
`enabled`, `replacements`, `resolved[].replacing`) for a UI. Each `resolved[]` entry (and
the FFI `InstalledPlugin`) has `active`: exactly this rule, so a host using `list` need
not derive it. `enabled` is not `active`: a replaced package stays `enabled: true` with
`active: false`. `list --provides SERVICE` keeps inactive packages (it is a status view).

- Every deploy (install, update, rollback, restore, `replace --back`, enable, reconcile)
  copies `<path>/dist` of the resolved commit from the clone's Git objects (one
  `git archive`), never the working tree: uncommitted edits in a clone are not deployed,
  nested folders and the executable bit are kept, symlinks are refused, and a revision
  without that folder is refused (`invalid_manifest`, "release … has no dist/ folder")
  instead of deploying an empty folder.
- `replace X --with Y` removes `dist/X/`; nothing of `Y` is copied there; `dist/Y/` stays.
  `Y` must be enabled (`invalid_argument`: "Y is disabled; enable it first"), so a
  replacement never leaves nothing loaded. `replace X --back` deploys `dist/X/` again from
  `X`'s clone at its installed commit (`plugins.X.current.commit`, folder `path` of a
  multi-package clone), so a shared clone whose HEAD is another package's revision needs
  no checkout. `Y` stays deployed: two packages providing the same service is the host's
  concern.
- `disable X` removes `dist/X/`; `enable X` deploys it again the same way. Disabling a
  package that replaces another is refused (`has_dependents`, naming what it replaces)
  unless `--force`, as for a package other packages depend on.
- `install`, `update` and `rollback` of a replaced or disabled package change state and
  clone only; its `dist/` appears on `replace --back` / `enable`. `fork … --replace` ends
  with the fork deployed and the original's `dist/` removed.
- `forget X` removes `dist/X/` (the clone stays); `restore X` deploys it. Forgetting or
  uninstalling a replacement drops its bindings, so each original it replaced is
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
file is reported the same way and nothing is written below it. An active package whose
clone or installed commit is gone is skipped, so it does not block an unrelated command;
the CLI warns `Warning: ID: dist not deployed (clone missing); run \`gitpm sync-dist\``
(`DistSync.skipped` from `sync_dist_plan`). Any other deploy error fails the command.
Read-only commands such as `list` and `check` never reconcile `dist/`; `list` does open (lock, and create if missing) the workspace, so it completes an interrupted commit it finds, while `check` opens nothing.
`gitpm sync-dist [--dry-run]` (`Manager::sync_dist`, FFI `syncDist`) does only the
reconcile: for a workspace written by gitpm 0.4 or earlier (which deployed every
installed package) that has only seen read-only commands since upgrading, or as an
explicit repair. It syncs every package it can, then exits with status 1 naming the ones it
could not deploy.

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
  tagged commit, so a package may move to another folder between releases. Legacy
  `vVERSION` tags are ambiguous in such a repository and are skipped with a warning
  (`legacy tag … is ambiguous …; tag ID@VERSION instead`); a newest release that is
  only such a tag, or a tag whose commit lacks its package, fails the install clearly.
- `gitpm install REPO ID` (or `--package ID`) installs one package. Without an ID a
  repository with several packages is refused with the list of its packages (`tilr/store
  (packages/store), tilr/theme (packages/theme)`); one with a single listed package
  needs none; an `--ref ID@VERSION` tag names the package itself. For a single-package
  repository a given ID must match its manifest. `--ref` and `--channel dev` (HEAD of the
  default branch) work per package.
- Workspace: the packages of one source share one clone, `repos/<name>-<hash of the
  source>`; state records `clone` (the cache key of that clone) and `path` (the package
  folder) per plugin, and only `<path>/dist` is deployed to `dist/<namespace>/<name>`.
  Updating, rolling back or uninstalling one package leaves the other packages alone; the
  clone is removed with the last package that uses it (its local work is guarded then,
  as for any clone). Since the clone has one HEAD for all its packages, HEAD only counts
  as moved when it is neither an installed revision of one of them nor a tagged commit;
  when a package is uninstalled or forgotten, HEAD moves to the installed revision of a
  remaining one (working-tree changes are kept; if they conflict with that revision the
  operation is refused). A root package keeps its own `plugins/<id>` clone, also when it
  later moves into a package folder (`path` set, `clone` absent).
- Shared clones keep `refs/gitpm/<namespace>/<name>/current` and `.../previous` for every
  package that uses them (registered or forgotten), updated with each change and deleted
  with the package, so an upstream rewrite, a pruned tag or `git gc` never loses another
  package's installed or rollback revision. One tag policy: fetching takes upstream's tags
  as they are (moved ones overwritten; `update` also drops deleted ones), but is refused
  (`version_conflict`, naming the tag) when the release tag of an installed revision of
  any package of the clone would move.
- Forgetting a package of a shared clone keeps a record in `state.json` (`forgotten`): it
  still holds the clone (its `dist/` is removed by `forget`), `restore` registers it
  again at its recorded revision and deploys it, and `uninstall` of the forgotten package
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
  a Git identity into an existing repository (no identity: refused as by `commit`); only a
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
$ gitpm fork tilr/theme tilr/panel --ns me --into forks/me-plugins
Fork created: /…/forks/me-plugins
Git history and local edits are kept; the original repository is the upstream remote.
Only the forked packages are left in the tree and in gitpm.json.
me/theme forked from tilr/theme (packages/theme)
me/panel forked from tilr/panel (packages/panel)
Dependency: me/panel now depends on me/theme ^1.0 (was tilr/theme)
…
$ gitpm fork tilr/dock --ns me --into forks/me-plugins   # add one more later
Packages added to: /…/forks/me-plugins
me/dock forked from tilr/dock (packages/dock)
Dependency: me/dock now depends on me/theme ^1.0 (was tilr/theme)
```

```console
$ git init -q tilr-plugins
$ gitpm init tilr/store --into ./tilr-plugins
$ gitpm init tilr/theme --into ./tilr-plugins
$ gitpm install ./tilr-plugins
Error: repository has 2 packages at commit 4b377fe1c0de; choose one by ID: tilr/store (packages/store), tilr/theme (packages/theme)
$ gitpm install ./tilr-plugins tilr/store && gitpm install ./tilr-plugins tilr/theme
installed tilr/store@0.1.0 from /…/tilr-plugins
installed tilr/theme@0.1.0 from /…/tilr-plugins
$ echo 'export const v = 2;' > tilr-plugins/packages/store/dist/index.js
$ gitpm commit ./tilr-plugins 0.2.0 --package tilr/store
tilr/store committed 0.2.0 (local commit and tag tilr/store@0.2.0)
$ gitpm update tilr/store       # tilr/theme stays at 0.1.0
tilr/store updated 0.1.0 → 0.2.0
```

Settings: each has a flag, an environment variable and a default (`gitpm --help` lists
all commands, `gitpm COMMAND --help` one). There is no config file; the old `PM_*` variables are ignored.

| Flag | Variable | Default |
| --- | --- | --- |
| `--root DIR` | `GITPM_ROOT` | `./gitpm-workspace` |
| `--catalog LOC` (folder, `catalog.json` or HTTP(S) URL) | `GITPM_CATALOG` | none (optional, see [working without a catalog](#working-without-a-catalog)): catalog commands, `install-catalog` and `check` ask for it |
| `--sources FILE` | `GITPM_SOURCES` | `./sources.txt` |

Publishing a catalog is up to you: it is a folder of static files, so any static host
works (`catalog-update` and `register` write only to a local folder; HTTP(S) catalogs are
read-only).

## Plugin metadata

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
catalog and listing treat such a key (and `null`) as absent, so packages published
before 0.5 keep working (`install`/`update` warn: `Warning: ID VERSION: plugin.json tags
must be an array of strings; ignored`; the library reports it in
`InstallResult.warnings`/`RevisionChange.warnings`, and
`Manifest::metadata_problems()`/`metadata_strict()` give the check). The keys stay in the file as
they were: `commit`, `fork` and `fork --replace` rewrite `version`, `id`, `forkOf`,
`replaces` and dependencies but keep the metadata (also the string form of `provides`).
`init` writes `"title": "NAME"`.

Shape rule, everywhere gitpm reports metadata (`list` `resolved[]` and
`plugins.<id>.metadata`, `info`/`browse` `PluginSummary`, `CatalogRelease`, the FFI
records): all four keys are always present; an absent string (`title`, `description`) is
`null` (`Option<String>`, Swift `String?`), an absent list (`provides`, `tags`) is `[]`
(`Vec<String>`, `[String]`).

- `gitpm list`: each `resolved[]` entry has `title`, `description`, `provides` and
  `tags` of the installed revision; state keeps them per plugin
  (`plugins.<id>.metadata`, since 0.5, same shape). For a plugin installed by an older
  gitpm (no `metadata` in its state record, so `plugins.<id>` has none either) they are
  read from the manifest of the installed commit in its clone.
- `gitpm list --provides SERVICE`: only the packages whose `provides` contains `SERVICE`.
  It filters `plugins` and `resolved` only; `replacements` and `forgotten` are printed
  unfiltered (they describe the workspace, not single packages).
- Catalog: a release (`release.json`, or a v1 `catalog.json` entry) has the whole
  manifest, so `catalog-update` and `register` publish the metadata with it. `info` and
  `browse` (`PluginSummary`) show the metadata of each plugin's latest release; a
  missing or broken latest `release.json` only leaves that plugin without metadata. The
  plugin index format is unchanged (0.4 reads it with `deny_unknown_fields`, so it cannot
  carry the metadata), so older gitpm versions read the catalog as before. Cost: the
  overview reads one `release.json` per listed plugin (at most 40), which on an HTTP(S)
  catalog is one extra request each.
- FFI: `InstalledPlugin`, `PluginSummary` and `CatalogRelease` have `title`,
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
  "clone": "tilr-plugins-4b377fe1c0de1234"
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
- All subprocesses (system `git`, `curl`) go through one internal module (`exec`), the
  place to switch to gix/ureq later.
- File formats (plugin.json, catalog v1/v2) are unchanged from the stand prototype apart
  from optional additions (`path` in catalog releases, the new `gitpm.json`, the
  [metadata](#plugin-metadata) keys of plugin.json and `metadata` per plugin in
  `state.json`, which 0.4 ignores);
  `deny_unknown_fields` is kept where it was. `state.json` carries `"format": 2`
  (`gitpm::STATE_FORMAT`) since 0.4: written always, a file without it is format 1 (0.3
  and earlier) and still read, a newer format is refused (`unsupported`). Format 2 adds
  `clone`/`path` per plugin and `forgotten`.
- Compatibility: 0.4 is not downgrade-safe. A workspace written by 0.4 with packages of
  multi-package repositories cannot be used by 0.3 (it ignores `clone`/`path` and looks
  for `plugins/<id>`), and 0.3 clients cannot install catalog releases that have a `path`
  (packages of multi-package repositories); root-package releases stay compatible.

## API overview

```rust
use gitpm::{Manager, Options};

let pm = Manager::open_with("/path/to/workspace", Options { catalog: Some(url) })?;
pm.install("git@github.com:acme/demo1.git")?;          // latest release tag
pm.install_channel(src, Some("acme/demo1@0.2.0"), false)?; // explicit ref; dev=true follows HEAD
pm.install_package(src, Some("tilr/store"), None, false)?;  // one package of a gitpm.json repo
pm.install_from_catalog("acme/demo1", None)?;          // exact catalog release, hashes verified
pm.update("acme/demo1")?;  pm.update_channel(id, None, true)?;
pm.rollback(id)?;  pm.enable(id)?;  pm.disable(id)?;  pm.forget(id)?;  pm.restore(id)?;
let plan = pm.uninstall_plan(id)?;                    // deletes + lost_work
pm.uninstall(id, /* confirm */ !plan.lost_work.is_empty() && user_said_yes)?;
// Forks: the fork repository is always explicit (new folder, or a repo with gitpm.json).
pm.fork_in_namespace(id, Path::new("/work/demo1-fork"), "me")?;   // sets forkOf + replaces
let done = pm.fork_replace(id, Path::new("/work/demo1-fork"), "me", None, confirm)?; // + release, install, replace
if let Some(step) = done.failed { /* fork folder kept; done.error says why */ }
let repo = Path::new("/work/forks/me-plugins");               // created, or added to
let fork = pm.fork_packages(&["tilr/theme", "tilr/panel"], repo, "me")?; // one repo, deps rewritten
let done = pm.fork_packages_replace(&ids, repo, "me", Some("2.0.0"), confirm)?; // per-package results
if let Some(error) = done.error() { /* fork kept; done.packages[i].failed says where */ }
let plan = pm.replace_plan("vendor/base", "mine/base")?;       // consumers + incompatible
pm.replace("vendor/base", "mine/base", /* confirm */ !plan.incompatible.is_empty() && yes)?;
pm.switch_back("vendor/base", confirm)?;                       // back to the original
pm.uninstall_with(id, confirm, /* force: ignore dependents */ false)?;
pm.set_enabled_with(id, false, force)?;  pm.forget_with(id, force)?;
let sync = pm.sync_dist()?;   // dist/ = active plugins (sync_dist_plan: read-only preview)
pm.list()?;  pm.state()?;  pm.check()?;  pm.browse_catalog()?;  pm.catalog_release(id, None)?;
```

Modules with free functions (no workspace lock needed):

| Module | Functions |
| --- | --- |
| `catalog` | `browse`, `list`, `lookup`, `selected`, `register`, `register_build`, `register_package`, `update_sources` (progress callback), `migrate`, `hash_files`, `version_key`, `ensure_publishable` |
| `check` | `read_state` (does not create/lock), `compare`, `check`; `CheckStatus::message()` |
| `auth` | `check(source, &AuthOptions)` — `git ls-remote` only; refuses credentials in the URL, stores nothing; failures are `auth` |
| `scaffold` | `init(dir, namespace, name)` (the `init --into` rule; the only way to create a package besides `create_package`), `init_with` (`InitOptions { multi, release }`), `create_package` (into a multi-package repo), `commit`, `commit_package` (version tag), `release_current`, `set_remote`, `shell_quote` |
| `packages` | `config_at`, `layout_at` (packages and defective folders of a commit), `packages_at` (strict), `find_at` (one package of a commit), `read_config`, `packages_in` (working tree), `matches`, `validate_pattern` |
| `identity` | `valid_part`, `qualified`, `release_tag` (no default namespace, no namespace config) |
| `releases` | `discover`, `latest` |

### Uninstall: confirmation by plan

`uninstall_plan(id)` is read-only and returns `UninstallPlan { id, registered, deletes,
lost_work }`. `lost_work` lists `LostWork` items: `Modified`, `Untracked`, `Ignored`,
`HeadMoved` (HEAD is not the installed revision), `UnpublishedCommit`,
`UnpublishedBranch`, `UnpublishedTag`, `Stash`. `uninstall(id, false)` refuses with
`Error::LocalChanges { items, .. }` when anything would be lost; `uninstall(id, true)`
deletes anyway. The caller decides how to ask the user. IDs match exactly: on a
case-insensitive file system `Vendor/Module` never resolves to `vendor/module`.

### Joint upgrades

```sh
gitpm update all --dry-run        # resolve and validate, without applying
gitpm update all                  # apply the compatible set atomically
```

The library exposes `Manager::update_all_plan()` and `Manager::update_all()`;
Swift/Kotlin bindings expose the same operations. Both return `UpdateAllPlan`
with `updates`, new dependency `installs`, and `kept_back` entries (selected and
latest versions). Apply resolves again under the workspace lock; a preview is
not a saved plan and upstream changes can change the result.

The solver considers all installed plugins together, using stable release tags
from each repository's origin and the installed revision as a fallback. Missing
dependencies are resolved from the configured catalog. It prefers newer versions,
prioritizes installed packages in lexical ID order, and backtracks when a choice
cannot satisfy the whole graph. This is a deterministic compatible solution,
not a guarantee of maximizing the number of upgraded packages. A plugin may
advance to an intermediate compatible release while its newest release is kept
back. Existing single-plugin install/update behavior remains unchanged.

No plugins are removed or downgraded. Active fork replacements constrain consumers
using each candidate's declared `replaces.version`; disabled flags are preserved
and disabled plugins cannot satisfy dependencies. If the existing state is broken
and no compatible solution repairs it, resolution fails with an explanation.
Local changes, unpublished work and moved release tags block the operation.
Dev branches and explicit refs are supported by single-plugin update only.

All selected repositories and payloads are prepared and validated before the
transaction commits. A preparation failure leaves the workspace unchanged;
interrupted commits use the existing journal recovery. A dry run also stages and
validates payloads temporarily, so it can require network access and disk space.

The policy follows the non-removing approach of
[APT upgrade](https://manpages.debian.org/testing/apt/apt.8.en.html), allowing new
dependencies. GitPM uses its existing PubGrub dependency solver; it does not invoke
APT or implement `full-upgrade` removals.

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
keep declaring `replaces`. Uninstalling or forgetting the fork drops its binding;
uninstalling or forgetting the original keeps it (a reinstalled original stays replaced
until `switch_back`). The fork must be enabled to become the provider, and disabling an
active fork needs `force`. A replaced original has no `dist/` (see
[Workspace layout](#workspace-layout-and-dist)).

### Catalog

A catalog is optional ([working without a catalog](#working-without-a-catalog)):
without one, `check`, `browse_catalog` and `catalog_release` fail with `invalid_argument`
("no catalog configured"). A workspace uses one catalog. `install_from_catalog` records the canonical catalog
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
let plan = try pm.uninstallPlan(id: "acme/demo1")
if plan.lostWork.isEmpty || userConfirmed(plan.lostWork.map(describeLostWork)) {
    try pm.uninstall(id: "acme/demo1", confirm: !plan.lostWork.isEmpty)
}
```

FFI surface: `PluginManager` (open, root, list, check, browseCatalog, catalogRelease,
install(package; defaults to nil), installFromCatalog, update, updateAllPlan, updateAll
(`UpdateAllPlan`), rollback, enable,
disable(force), forget(force), restore, uninstallPlan, uninstall(confirm, force), fork
(one package; `ForkResult`), forkAndReplace(version, confirm; returns a
`ForkReplacement`: a failure after the fork exists is reported in `failedStep` (the
`ForkStep` enum: release, install, replace), `errorKind`, `errorMessage`), forkPackages
(`PackageFork`), forkPackagesAndReplace (`PackageForkReplacement`: the fork plus one
`ForkReplacement` per package), replacePlan, replace, switchBackPlan, switchBack,
syncDistPlan, syncDist (`DistSync`: deployed, removed, unmanaged, skipped); `force`
defaults to false; `InstalledPlugin.path` and `CatalogRelease.path` name the package
folder; `InstalledPlugin.clone` names the shared clone; `InstalledPlugin`,
`PluginSummary` and `CatalogRelease` carry the [metadata](#plugin-metadata); the fork records have the names of the library types `gitpm::ForkReplacement`,
`PackageFork`, `PackageForkReplacement`, `ForkedPackage`, `DependencyRewrite`,
`ForkStep`), free functions `authCheck(source, customSsh)`, `browseCatalog`, `catalogRelease`,
`readInstalled` (reads `state.json` without opening or locking the workspace; every
`PluginManager` method, `check` included, opens it), `describeLostWork`; errors are the
`PmError` enum (one case per kind, each with `message`; `id` where the library error has
one, `LocalChanges.items`, `HasDependents.dependents`, `IncompatibleReplacement.consumers`,
and `Other { kind, message }` for kinds added later). Author/catalog-generation commands
are not exposed.

## Hosts

Tilr's Mac app consumes the library through the UniFFI xcframework: `scripts/build-apple.sh`
builds `GitpmFFI.xcframework` (universal static library + C module) and `Gitpm.swift`, which
the app adds as a SwiftPM `binaryTarget` and a source target (see [Build](#build)).
The host loads every package folder under `dist/` (only active packages are deployed,
see [Workspace layout](#workspace-layout-and-dist)); after upgrading from gitpm 0.4 it
calls `syncDist()` once before loading (mutating calls also reconcile).

## Tests

```text
crates/gitpm/src/*.rs                    unit tests in each module's #[cfg(test)] mod tests
crates/gitpm/src/test_support.rs         shared fixtures (temp dirs, hermetic git,
                                         plugin repos, tree hash), compiled only for tests
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
