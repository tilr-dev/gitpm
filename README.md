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

The shared specification authorization checker for dev pull requests is documented
in [Specification authorization checker](docs/spec-change.md), including local use.

A package is a folder in a Git repository with a `plugin.json` manifest (`id`, `version`,
`dependencies`, optional `forkOf` and `replaces`, descriptive
[metadata](#package-metadata); other keys are kept as they are) and a
ready output folder (`dist/` by default): the repository root, or one of several folders a root `gitpm.json`
lists ([multi-package repositories](#multi-package-repositories)). `dist` selects the output
folder (default `"dist"`, `"."` means the package root); `buildCommand` is descriptive only;
with `buildRequired` (default false), a missing folder is refused with `invalid_manifest`
naming the build command when supplied. Git installs clone the
repository and copy the package's declared output folder. A catalog release with
`build: {url, sha256}` installs its ready `.tar.gz` output without cloning; nothing is built or executed.
All packages in one Git repository share one version and one annotated release tag `vVERSION`. Git honors `tag.gpgSign` when creating it.
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

A package is a Git repository with `plugin.json` and a ready output folder (`dist/` by default); a release is
a repository `vVERSION` tag. Package IDs are immutable UUIDs; `name` is a nonunique human alias.
Repositories are plain folders named by you (`./greeter`); gitpm never derives a folder
from an ID:

```console
$ mkdir -p greeter/dist && cd greeter && git init -q -b main
$ echo '{"id": "d135a7cf-1c63-549a-93cb-8f89656b2ced", "name": "greeter", "version": "0.1.0"}' > plugin.json
$ echo 'export const greeting = "greeter v1";' > dist/index.js
$ git add . && git commit -qm "greeter 0.1.0" && git tag -m "Release v0.1.0" v0.1.0 && cd ..
$ gitpm install ./greeter
installed d135a7cf-1c63-549a-93cb-8f89656b2ced@0.1.0 from /…/greeter
$ cat gitpm-workspace/dist/d135a7cf-1c63-549a-93cb-8f89656b2ced/index.js
export const greeting = "greeter v1";
$ gitpm list
{
  "format": 1,
  "packages": {
    "d135a7cf-1c63-549a-93cb-8f89656b2ced": {
      "source": "/…/greeter",
      "current": {
        "version": "0.1.0",
        "commit": "4b377fe…"
…
```

Release 0.2.0, update, roll back (one previous version is kept):

```console
$ echo 'export const greeting = "greeter v2";' > greeter/dist/index.js
$ echo '{"id": "d135a7cf-1c63-549a-93cb-8f89656b2ced", "name": "greeter", "version": "0.2.0"}' > greeter/plugin.json
$ git -C greeter commit -qam "greeter 0.2.0" && git -C greeter tag -m "Release v0.2.0" v0.2.0
$ gitpm update d135a7cf-1c63-549a-93cb-8f89656b2ced
d135a7cf-1c63-549a-93cb-8f89656b2ced updated 0.1.0 → 0.2.0 (major)
$ cat gitpm-workspace/dist/d135a7cf-1c63-549a-93cb-8f89656b2ced/index.js
export const greeting = "greeter v2";
$ gitpm rollback d135a7cf-1c63-549a-93cb-8f89656b2ced
d135a7cf-1c63-549a-93cb-8f89656b2ced rolled back 0.2.0 → 0.1.0
$ cat gitpm-workspace/dist/d135a7cf-1c63-549a-93cb-8f89656b2ced/index.js
export const greeting = "greeter v1";
$ gitpm update d135a7cf-1c63-549a-93cb-8f89656b2ced
d135a7cf-1c63-549a-93cb-8f89656b2ced updated 0.1.0 → 0.2.0 (major)
$ gitpm update d135a7cf-1c63-549a-93cb-8f89656b2ced
d135a7cf-1c63-549a-93cb-8f89656b2ced up to date (0.2.0)
$ gitpm install ./greeter         # installed already: nothing changes, exit 0
d135a7cf-1c63-549a-93cb-8f89656b2ced already installed (0.2.0)
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
ID` of an enabled consumer refuse with `dependency_conflict` ("757fd5ca-2b6a-5b2f-8a5d-60980b028665 is disabled;
enable it") while a dependency resolves only to a disabled package, `list` and `check`
report it unsatisfied, and `update all` skips such a consumer (`dependency DEP: DEP is
disabled`). `enable ID` never enables dependencies itself;
`enable ID --force` enables it anyway (it is then reported broken), and `install
--force` (`install-catalog --force`) is a download plus that forced enable of the named
package. `install` over a package downloaded from the same source enables it (with the
same check) and prints the install line. `rollback` of an active package and `restore`
of an enabled record refuse a disabled dependency as well:

```console
$ gitpm download-catalog b879842f-16c2-558f-9d03-a271dc901f57
downloaded b879842f-16c2-558f-9d03-a271dc901f57@1.0.0 from /…/app (disabled)
downloaded 757fd5ca-2b6a-5b2f-8a5d-60980b028665@1.2.0 from /…/core (disabled)
$ gitpm enable b879842f-16c2-558f-9d03-a271dc901f57
Error: cannot enable b879842f-16c2-558f-9d03-a271dc901f57: 757fd5ca-2b6a-5b2f-8a5d-60980b028665 is disabled; enable it; force does it anyway
Error kind: dependency_conflict
Hint: `gitpm enable b879842f-16c2-558f-9d03-a271dc901f57 --force` enables it anyway
$ gitpm enable 757fd5ca-2b6a-5b2f-8a5d-60980b028665 && gitpm enable b879842f-16c2-558f-9d03-a271dc901f57
757fd5ca-2b6a-5b2f-8a5d-60980b028665 enabled
b879842f-16c2-558f-9d03-a271dc901f57 enabled
```

### Live author folders

```sh
gitpm link ./hello                   # link this package's ready dist/ output
gitpm link ./repo/packages/theme     # select one package, leaving siblings alone
gitpm check
gitpm update all --dry-run
gitpm uninstall 7e6e8392-7be8-55ae-bd03-2b430b1c5d59           # remove the workspace entry; keep the author tree
```

`link LOCAL_PACKAGE_PATH` (library `Manager::link`, FFI `PackageManager.link`)
validates the local manifest and publishes `dist/<uuid>` as a symlink
to its canonical absolute installation folder. As for managed installs,
`plugin.json.dist` defaults to `"dist"`; `"."` selects the package root and any other
valid package-relative folder selects that output. Prepare missing output yourself:
gitpm never builds or watches it. Author edits become visible through the deployed
path immediately, including uncommitted files. The host may still need to reload.

Linking snapshots identity, declared version, dependencies, metadata and the
replacement offer. Editing the external manifest does not change that snapshot;
run `link` again to validate and refresh it, including an explicitly changed output
folder. New dependencies must already be satisfied by installed providers. A
managed installation of the same ID must be explicitly uninstalled first; linking
never silently converts it. Workspace-owned folders, cycles and non-UTF-8 canonical paths are refused with `workspace`.

A live record has no installed revision or owned clone:

```json
{
  "mode": {"link": {"packageRoot": "/work/hello", "target": "/work/hello/dist"}},
  "enabled": true,
  "version": "0.1.0",
  "dependencies": {},
  "metadata": {"title": null, "description": null, "provides": [], "tags": []}
}
```

`source` is recorded when a usable origin is available. Link records omit
`current`, `previous`, `clone`, `path`, `catalog` and top-level `forkOf`. The library's
resolved `InstalledPackage.current` and FFI `InstalledPackage.current` /
`PackageRecord.current` are optional: links instead carry `version`. A version on
a link is a declared snapshot, not a guarantee of reproducible bytes. State format
stays 1 before the first public release; recreate an incompatible workspace by
installing its packages again in a new root. There is no migration reader; foreign
formats are refused unchanged with `unsupported`.

Check and updates query releases without fetching into the author's repository.
Update results carry `dev_linked` disposition with a structured availability row
and separate dependency problems. A newer release, no releases or an unreachable
source is reported explicitly alongside link mode. Even an explicit `--ref`,
`--dev`, `--release` or `--detach` keeps a link unchanged. Joint updates hold its
recorded version and dependencies fixed while healthy managed packages can update.
Dry-run reports the same disposition without writing anything.

Disable, enable, forget, restore, replacement and reconciliation manage only the
workspace symlink and state. A forgotten link retains its descriptor; missing
output is reported with its link descriptor in `DistSync.skipped`. Uninstall plans
list the owned workspace entry and never list the author folder for deletion.
Dirty files, staged work, ignored output, stashes and unpublished refs in that
folder remain untouched, including forced removal. Rollback fails with
`no_previous_version`. Transactions stage and recover symlink entries with state
published last; external files are never snapshotted or rolled back. Atomicity
covers workspace entries and state, not a concurrent external build's whole output.

Fork preparation and live installation are separate:

```sh
gitpm fork 7e6e8392-7be8-55ae-bd03-2b430b1c5d59 --into hello-fork
# Build the manifest-selected output yourself if needed.
gitpm link ./hello-fork
gitpm replace 7e6e8392-7be8-55ae-bd03-2b430b1c5d59 --with ce6c8a3b-b2f8-5391-8ef8-4cbb7873f5d4
```

Fork alone changes no workspace installation, deployment or release tags. Link
installs only the selected fork package, without a release or replacement binding.
The last explicit step uses the validated top-level `replaces` offer and existing
compatibility/dependency checks. Consumers then load the live fork deployment.
Its original-upstream discovery uses the validated
`mode.link.discovery.forkOf` snapshot until relink refreshes it; changing the
external provenance or offer alone has no effect.

### Releases, pinned refs and branches

Every installed package has a mode, recorded in `state.json` as `mode` (omitted in
release mode), and `update ID` follows it:

| Install | Mode (`state.json`) | `update ID` |
| --- | --- | --- |
| `install SRC` | release (no `mode`) | newest release |
| `install SRC --ref v0.1.0` or `--ref COMMIT` | `{"pinned": "v0.1.0"}` (a commit in full) | stays: `d135a7cf-1c63-549a-93cb-8f89656b2ced pinned at 0.1.0 (v0.1.0)` |
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
another commit (an explicit `--ref` of a moved release tag warns `release tag vV now
points to COMMIT`); a
branch whose history no longer contains the installed commit, or that the origin no
longer has, is refused (`version_conflict`, "ID: history of main diverged; automatic update
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
of the installed version prints `ID up to date (V)` with `Warning: release tag vV now
points to COMMIT; the installed commit is kept`, and a retracted installed release (its
tag deleted, only older ones left) `ID up to date (V)` with `Warning: installed release
ID@V no longer exists on the origin`; a tag moved onto a commit of another version is a
broken tag (`… now points to COMMIT (skipped: REASON) …`; as the newest tag it still
fails `update ID`, "latest release tag is broken"). Integrity of published versions is the job of a
verified catalog. `rollback` to a previous commit that is no longer in the clone (pruned
after its tag went) fails with `no_previous_version` ("previous revision COMMIT of ID is
no longer available").

```console
$ gitpm install ./greeter --ref v0.1.0
installed d135a7cf-1c63-549a-93cb-8f89656b2ced@0.1.0 from /…/greeter
$ gitpm update d135a7cf-1c63-549a-93cb-8f89656b2ced
d135a7cf-1c63-549a-93cb-8f89656b2ced pinned at 0.1.0 (v0.1.0)
$ gitpm update d135a7cf-1c63-549a-93cb-8f89656b2ced --release
d135a7cf-1c63-549a-93cb-8f89656b2ced updated 0.1.0 → 0.2.0 (major)
```

`forget ID` keeps the whole record in `state.json` under `forgotten`, together with
its bare source repository and individual Git worktree (or cached build output).
`restore ID` records it again with its revisions, catalog binding, enabled flag and
mode unchanged. In every Git mode, the package worktree must be clean and its HEAD
must equal the recorded commit. Edited files, unpublished work or a different HEAD
refuse with `local_changes`.

### Catalogs

A catalog is generated from the release tags of the repositories listed in a sources
file (paths are relative to that file). `gitpm init NAME --into DIR`
scaffolds the package with its first release: a new `DIR` becomes a package
repository, a repository with `gitpm.json` gets a package folder. The alias and `DIR` are required; init generates a fresh UUID. `gitpm commit` bumps every package to the same repository version, commits
everything and tags:

```console
$ export GITPM_CATALOG=$PWD/catalog
$ echo './greeter' > sources.txt
$ gitpm catalog update
Reading releases: /…/greeter
Catalog ready: 2 release(s) checked from sources.
$ gitpm catalog show
Catalog: /…/catalog
Type: local folder/JSON
Verified: no (discovery only)
Directories and packages:
  packages/
    d135a7cf-1c63-549a-93cb-8f89656b2ced/
      d135a7cf-1c63-549a-93cb-8f89656b2ced — latest: 0.2.0, versions: 2
          Git: /…/greeter
$ gitpm init hello --into hello

Created: /…/hello
ID: 7e6e8392-7be8-55ae-bd03-2b430b1c5d59
Version 0.1.0: Git commit and tag v0.1.0.
…
$ echo 'export const greeting = "hello v2";' > hello/dist/index.js
$ gitpm commit hello 0.1.1
7e6e8392-7be8-55ae-bd03-2b430b1c5d59 committed 0.1.1 (local commit and tag v0.1.1)
$ echo './hello' >> sources.txt
$ gitpm catalog update
Reading releases: /…/greeter
Reading releases: /…/hello
Catalog ready: 4 release(s) checked from sources.
$ gitpm install-catalog 7e6e8392-7be8-55ae-bd03-2b430b1c5d59
installed 7e6e8392-7be8-55ae-bd03-2b430b1c5d59@0.1.1 from /…/hello
$ cat gitpm-workspace/dist/7e6e8392-7be8-55ae-bd03-2b430b1c5d59/index.js
export const greeting = "hello v2";
$ gitpm check
Workspace: gitpm-workspace
Catalog: /…/catalog (unverified: discovery only)

PACKAGE                  INSTALLED      LATEST         ENABLED  SOURCE   STATUS
d135a7cf-1c63-549a-93cb-8f89656b2ced             0.2.0          0.2.0          yes      catalog  up to date
7e6e8392-7be8-55ae-bd03-2b430b1c5d59               0.1.1          0.1.1          yes      catalog  up to date

Updates: 0. Nothing was changed.
```

#### Verified catalogs

A catalog is a guarantor only when its owner says so: `"verified": true` in the
top-level `catalog.json`, set by hand (no command sets it; `catalog register` keeps it). Any other
catalog serves discovery only: `install-catalog` still installs exactly the release record
(commit, manifest and file hashes are checked), but records nothing, and the package then
updates from its origin like a direct install.

- Versions enter a verified catalog only one by one through `catalog register` (a manual act of
  verification); `catalog update` into it fails with `invalid_argument` ("a verified
  catalog takes versions only one by one through register") and changes nothing. An ID
  stays bound to the source that first registered it, which preserves UUID ownership.
- A package installed from a verified catalog (and each dependency pulled from it) is
  protected: its record has `catalog` (the canonical location; `list` shows it only for
  protected packages). `update ID` and `update all` move it only to versions that catalog
  lists, read from the recorded location whatever `--catalog` says, after checking that
  the origin has the release commit and that manifest, package folder and dist file hashes
  equal the record (`catalog`: "catalog file hashes do not match git dist of ID@V: the
  release differs from the one added to the catalog"; nothing changes). A newer origin
  release the catalog lacks is not installed: `ID kept at V (upstream W not in catalog)`,
  and `check` uses the catalog snapshot without querying origin. A ref or branch target (`--ref`, `--dev`)
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
  and `catalog show` a `Verified:` line.

```console
$ gitpm install-catalog 7e6e8392-7be8-55ae-bd03-2b430b1c5d59            # catalog.json has "verified": true
installed 7e6e8392-7be8-55ae-bd03-2b430b1c5d59@0.1.1 from /…/hello
$ gitpm update 7e6e8392-7be8-55ae-bd03-2b430b1c5d59                     # the origin also has 0.2.0, not registered
7e6e8392-7be8-55ae-bd03-2b430b1c5d59 kept at 0.1.1 (upstream 0.2.0 not in catalog)
$ gitpm update 7e6e8392-7be8-55ae-bd03-2b430b1c5d59 --detach
7e6e8392-7be8-55ae-bd03-2b430b1c5d59 detached from catalog /…/catalog/catalog.json; it now follows its origin
7e6e8392-7be8-55ae-bd03-2b430b1c5d59 updated 0.1.1 → 0.2.0 (major)
```

`gitpm fork ID --into DIR` clones the recorded source of an installed
package with Git history at its installed revision into `DIR`. All packages in that
repository receive IDs fresh UUIDs while preserving their names, so the original and the fork can be installed side by
side. `DIR` must be a new folder; it is never derived from the ID. The output lists the next steps (commit and
tag, install, replace):

```console
$ gitpm fork 7e6e8392-7be8-55ae-bd03-2b430b1c5d59 --into hello-fork
Fork created: /…/hello-fork
Git history is cloned from the recorded source; the original repository is the upstream remote.
ce6c8a3b-b2f8-5391-8ef8-4cbb7873f5d4 forked from 7e6e8392-7be8-55ae-bd03-2b430b1c5d59 (repository root)
forkOf records each origin; replaces says the fork can stand in for it (used only
after an explicit replace). The original installations are unchanged.

Next steps (choose the packages to install and replace):
  gitpm commit '/…/hello-fork' NEW_VERSION   commit local edits, release all packages with tag vNEW_VERSION
  gitpm install '/…/hello-fork' ce6c8a3b-b2f8-5391-8ef8-4cbb7873f5d4
  gitpm replace 7e6e8392-7be8-55ae-bd03-2b430b1c5d59 --with ce6c8a3b-b2f8-5391-8ef8-4cbb7873f5d4
Set your own origin with: gitpm remote '/…/hello-fork' GIT_URL
$ ls -d hello*
hello
hello-fork
$ gitpm uninstall 7e6e8392-7be8-55ae-bd03-2b430b1c5d59
7e6e8392-7be8-55ae-bd03-2b430b1c5d59 uninstalled
```

To fork a Git path or URL without installing it or using a workspace:

```sh
gitpm init hello --into ./hello
gitpm fork --from ./hello --into ./hello-fork
```

`--from SOURCE` clones the latest stable repository release (`vVERSION`) containing
the first ID; every supplied ID must exist at that revision. Tags without that ID
and legacy `ID@VERSION` tags are ignored. Add `--ref REF` to select any Git revision,
including a branch or an untagged commit. Omit IDs only for a single-package source.
All repository packages are retained and assigned fresh UUIDs with their names
preserved, including siblings not named on the command line. Internal dependencies are rewritten. The destination must be new
and outside the local source repository. The source's uncommitted edits are excluded.
No workspace is opened, created or locked, even with `--root` set. `--ref` requires
`--from` for fork; `--replace`, `--version` and `--force` remain invalid fork options.

`fork ID --into DIR` only prepares an editable Git repository.
It preserves inherited tags and origin metadata, creates no release tag, installs
nothing and leaves workspace state, replacement bindings and dist unchanged.
Release the fork with `commit`, install the desired package explicitly, then call
`replace` for each original you want to replace. For example:

```sh
gitpm --root ./workspace fork d63a8a6b-85d6-509c-87db-9a6096daa66c --into ./my-fork
gitpm commit ./my-fork 0.3.0 --package 87fe9d68-adda-5200-92ad-a5c59d2264b9
gitpm --root ./workspace install ./my-fork 87fe9d68-adda-5200-92ad-a5c59d2264b9
gitpm --root ./workspace replace d63a8a6b-85d6-509c-87db-9a6096daa66c --with 87fe9d68-adda-5200-92ad-a5c59d2264b9
```

An explicit `commit` still refuses an existing release tag. Fork does not delete
inherited tags or choose a new version automatically. Installing `87fe9d68-adda-5200-92ad-a5c59d2264b9` does not
automatically install a sibling `e4e7959f-6315-5611-a719-75c5c2a920e3` unless dependency resolution requires it.
`replace` refuses incompatible consumers without changing the workspace. Use
`replace-check` for diagnostics and `replace-resolve` to explicitly install missing
dependencies; replace has no TTY confirmation or `--force` bypass. Successful output
prints the provider's installed and declared versions.

Output: stdout carries the result lines (one per affected package for workspace-mutating
commands, JSON for `list`, `catalog links` and `catalog register`); everything else goes to stderr:
`Warning: …` lines taken from the result (ignored metadata keys, skipped release tags,
the dist report), `[y/N]` prompts with what they list, and failures. A failure prints
`Error: MESSAGE`, for a library error also `Error kind: KIND` (and one `  - item` line
per lost item for `local_changes` outside update and rollback, whose message already
lists them, and for an uninstall's `has_dependents`) and, where a gitpm command helps, one `Hint: …` line
naming it; the exit status is 1. The CLI checks only the command line (unknown, repeated
or misplaced options, value sets, argument counts); domain rules and option
combinations are the library's, which also returns everything the CLI prints, so a host
gets the same through the FFI.

```console
$ gitpm forget 7e6e8392-7be8-55ae-bd03-2b430b1c5d59 && gitpm install ./hello
7e6e8392-7be8-55ae-bd03-2b430b1c5d59 forgotten (clone kept; restore re-registers it)
Error: 7e6e8392-7be8-55ae-bd03-2b430b1c5d59 was forgotten and its clone is kept
Error kind: already_installed
Hint: if 7e6e8392-7be8-55ae-bd03-2b430b1c5d59 was forgotten, `gitpm restore 7e6e8392-7be8-55ae-bd03-2b430b1c5d59` records it again and `gitpm uninstall 7e6e8392-7be8-55ae-bd03-2b430b1c5d59` removes its clone
```

Changed in 0.4: the form `fork ID NAMESPACE/NAME [--into DIR]` (fork folder
`DIR/NAMESPACE/NAME`, default `./NAMESPACE/NAME`) and `init NAMESPACE/NAME` without a
folder are gone; `init --in REPO` is now `init --into DIR`.

Changed in 0.5:

- [Plugin metadata](#package-metadata): `title`, `description`, `provides`, `tags` in
  `plugin.json`, reported by `list` (and `list --provides SERVICE`), `catalog show` and the FFI;
  `init` writes a `title`. Catalog overviews read each listed package's latest
  `release.json`.
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
  | `replace ORIGINAL --with ORIGINAL` | `ORIGINAL switched back from FORK (ORIGINAL VERSION for: consumers); FORK stays installed and enabled (disable or uninstall it if not wanted)` |
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
  type (`invalid_manifest`); `install`, `update`, `rollback`, `fork`, `catalog update`
  and `list` accept such manifests (published before 0.5) and treat those keys as
  absent, with a warning on stderr for install and update.
- `init --multi` makes a new (or empty) folder a multi-package repository;
  `init --no-release` commits without the release tag.
- `list`: `resolved[]` entries carry `clone` (as `plugins.<id>.clone`).
- Removing a package also removes the folders it leaves empty (`dist`, `repos`).
- `dist/` holds only active packages (installed, enabled, not replaced; see
  [Workspace layout](#workspace-layout-and-dist)): `replace` removes the original's
  `dist/`, `replace ORIGINAL --with ORIGINAL` and `enable` deploy it again, `disable` and `forget` remove
  it. Hosts no longer need `resolved[].replacing`/`.enabled` to pick what to load: load
  everything under `dist/` (or use the new `resolved[].active`). A workspace written by 0.4 keeps every installed package
  there: the first mutating command fixes it; until then (only read-only commands since
  upgrading) run `gitpm sync-dist` (FFI `syncDist`) once.

Changed in 0.6 (no backward compatibility before 1.0; every entry is **BREAKING**):

- **BREAKING:** Fork preserves dependency IDs and requirements. The unreleased inferred
  rewrite API is removed. Use library `repository_packages` / `edit_dependency`, FFI
  `repositoryPackages` / `editDependency`, or CLI `packages` / `dependency` to inspect
  packages and explicitly choose each dependency ID and requirement before releasing.
  `PackageFork` contains no dependency edits. Installed `list` entries now include
  `name` and `installed: true`; state records require the installed manifest name.
  Recreate earlier unreleased workspaces by installing their packages again.

- **BREAKING:** Source clones in `repos/<key>` are bare repositories. Each installed
  Git package has a detached worktree in `worktrees/<uuid>` at its own
  recorded commit. Updating or removing one package leaves sibling worktrees intact.
  Update, rollback, restore and unconfirmed uninstall reject local changes or a moved
  HEAD in the affected worktree. Builds have no worktree; Git objects needed for
  offline rollback remain in the bare repository. Forgotten Git packages retain their
  worktree. State remains format 1; recreate old workspaces. Opening does not migrate
  or automatically delete legacy checked-out clones.

- **BREAKING:** Explicit `link` installs live author output with `Mode::Link` and a
  discriminated state record. Resolved library and FFI package views make `current`
  optional and expose a declared `version` for links. `dev_linked` update reasons
  carry release availability and dependency problems; skipped deployment reports
  retain link descriptors. See [live author folders](#live-author-folders).

- **BREAKING:** Catalog releases with `build {url, sha256}` now install from that archive,
  rather than ignoring it. Artifacts are `.tar.gz`, with output files at the archive root;
  only regular files and directories are accepted. Both archive SHA-256 and the exact
  release `files` hashes and sizes are verified before workspace changes. Build requests
  have the catalog's 16 MiB download limit and timeout, with at most five HTTPS-only
  redirects (initial loopback HTTP is accepted for local testing).
  Build revisions retain their catalog location and release metadata in state, separately
  from verified-catalog protection, and store current/previous verified output under
  `builds/<id>/<archive-sha256>/`. Updates use the recorded catalog; rollback, enable and
  `sync-dist` use local output without network access. Ref/dev/detach updates switch to
  Git. Fork continues cloning the recorded source repository. Existing library and FFI
  install/update/rollback entrypoints support both sources; the CLI only presents results.

- **BREAKING:** Manifests accept `dist` (a package-relative folder, default `"dist"`; `"."` selects
  the package root), `buildCommand` (a descriptive string) and `buildRequired`
  (boolean, default false). Deploys and catalog hashes use the committed selected
  folder. Missing required build output fails with `invalid_manifest` and names
  the build command when supplied. gitpm never runs build commands.

- **BREAKING:** Catalog directory files (`catalog.json`) and package indexes use
  schema 1 (gitpm 0.5 wrote schema 2). Any other schema is refused with `catalog`.
  Migration: regenerate the catalog with `gitpm catalog update` into an empty folder.
- Catalog package indexes now carry a `latest` release summary without changing the
  schema number. Regenerate catalogs with `gitpm catalog update`. Overviews read one
  index per listed package and no `release.json`; installs still verify release records.

- Catalog commands: `info` → `catalog show`, bare `catalog` → `catalog links`,
  `catalog-update` → `catalog update`, `register` → `catalog register`. Old spellings
  have no aliases.


- `commit` and `init` into an existing repository now
  refuse a missing configured Git identity with `invalid_argument` instead of using
  a name or email derived by Git. Migration: set `user.name` and `user.email` in Git
  config (or provide both author and committer identity environment variables).
- `fork` clones the recorded source at installed revisions and keeps submodule links.
  Local installation edits are excluded. An existing `--into` repository now fails
  with `invalid_argument` (exit 1, previously success with exit 0). Fork the entire
  repository into a new folder instead. Fork now needs access to the recorded
  source. A relative or remote-helper recorded source is refused with `invalid_argument`
  (exit 1, previously success with exit 0 without `forkOf.source`).
- `auth::check` now takes `&Options`; the separate `AuthOptions` was removed.
  `Options` now includes `custom_ssh` (UniFFI `ManagerOptions.customSsh`, default false).
  Set it when the host supplies `GIT_SSH` or `GIT_SSH_COMMAND`; the CLI does this for
  every command. Auth checks and workspace Git commands share one batch SSH policy
  with keepalives and `StrictHostKeyChecking=accept-new`. The library never reads
  these environment variables. Standalone catalog and check operations offer
  `*_with_options` variants; their existing entry points use default options. FFI
  `check` takes `customSsh`.
- Submodules (gitlinks) inside a package's `dist/` become empty directories without
  fetching their contents. Registration, catalog update and every deploy warn that
  their files will not be installed, probably a mistake: commit built files to
  `dist/` instead of a submodule.
  Migration: include the dependency's files in the committed `dist/` tree.
- Package IDs are canonical lowercase UUIDs, generated by `init NAME --into DIR`.
  Names are nonunique aliases and are never resolved by the CLI. Legacy scoped IDs
  are refused with `invalid_id`; no migration or compatibility reader exists.
- Release tags are only `vVERSION`, in single- and multi-package repositories. All
  package manifests share that version at each release. Legacy `ID@VERSION` tags
  are ignored without a warning; there is no compatibility reader. `init` starts a
  new repository at `v0.1.0`; adding a package inherits the current repository version
  without tagging. Run `gitpm commit REPO NEW_VERSION` to release every package.
  `--package ID` selects the result ID only. Existing tags, including upstream tags
  retained by a fork, are never moved: choose a new version for a fork release.
- `state.json` format 1: installed packages are under `packages` (was `plugins`), Git
  clones in the workspace folder `repos/<name-hash>/`. Any other format, or a
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
  | `catalog::CatalogFormat`, `CatalogOverview.format` | removed (only the static file tree exists) |
  | `State::installed_in(root)` (read metadata from the clone) | removed; use `State::installed()` |
  | `Installed.metadata: Option<Metadata>` | `Installed.metadata: Metadata`, required in `state.json` (`invalid_data` without it) |
  | FFI `PluginManager` | `PackageManager` |
  | FFI `InstalledPlugin`, `PluginSummary` | `InstalledPackage`, `PackageSummary` |
  | FFI `OverviewItem.plugin(depth, plugin)` | `OverviewItem.package(depth, package)` |
  | FFI `CatalogFormat`, `CatalogOverview.format` | removed |
  | workspace `plugins/<ns>/<name>/` | `repos/<name-hash>/` |

- v1 catalogs are refused: a monolithic `catalog.json` (`{"plugins": …}`) fails every
  read and write with `catalog`; `legacy/<id>/` folders and releases without file
  hashes are gone (`catalog release of ID has no file hashes`). `migrate-catalog` and
  `catalog::migrate` are removed; `catalog show` no longer prints a `Format:` line. Migration:
  regenerate the catalog with `catalog update` into an empty folder.
- The CLI alias `create` is removed (`init` only); `create` and `migrate-catalog` fail
  with `unknown command: X; use --help`.
- `list` takes metadata only from the state record; `metadata` is required in every format-1 record (a record without it is `invalid_data`).
- A new fork no longer gets the Git config keys `pm.forkBase`/`pm.forkVersion` (nothing
  read them); `forkOf` in the manifest is the only fork provenance.
- `gitpm-ffi` carries the library version (0.6.0 with this release) and promises no
  stability before 1.0.
- A local catalog location that does not exist (`--catalog ./catlog`, `GITPM_CATALOG`,
  `Options.catalog`) is refused with `invalid_argument` ("catalog location PATH does not
  exist") before anything is created; it no longer counts as "no catalog" for `install`.
  `Manager::open_with` validates its Options first. The free catalog readers do the same:
  `check`, `catalog show`/`browse`, `catalog links`, `catalog_release`/`lookup` on a missing local
  location fail with `invalid_argument` (was `catalog` "catalog not found"). Migration:
  create the catalog folder (`catalog update`/`catalog register` create it) or drop the setting.
- `gitpm catalog links` prints the library's typed directory links `{"schema": 1,
  "directories": {…}}` (`catalog::links`, the same as FFI `catalogLinks`) instead of the
  raw `catalog.json`; a monolithic (v1) catalog or otherwise invalid root fails with `catalog`.
- `uninstall` and `forget` of a recorded package whose clone is gone drop its record and
  `dist/` without confirmation (was `repository_not_found`; `forget` keeps no `forgotten`
  record then); the CLI warning for such a package says `uninstall or forget ID to drop
  its record` (was `run \`gitpm sync-dist\``, which could not deploy it); `restore` of a forgotten package whose shared clone is gone fails
  with `repository_not_found` (was `workspace`), like any restore without a clone.
- Active packages below a file or symlink at `dist/<uuid>` are now reported in
  `DistSync.skipped` with reason `dist/<uuid> is not a folder`. Mutating commands
  warn `Warning: ID: dist not deployed (dist/<uuid> is not a folder)`;
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
  `gitpm uninstall ID`). `init NAME` creates a package with a generated UUID. Prompts (`[y/N]` and what
  they list) go to stderr.
- `update ID` returns the broken release tags it passed over in `RevisionChange.skipped`,
  `update all` in `UpdateAllPlan.skipped` (for every installed package, updated or not),
  and the CLI warns `Warning: skipped release tag NAME: reason` as install does.
- Fork is preparation only. The composed fork release/install/replace operations,
  resume methods and outcome records have been removed; use explicit operations.
- Library API: `Manager::set_enabled`/`set_enabled_with`/`enable`/`disable`/`forget`/
  `forget_with`/`uninstall` return the `DistSync` report (were `()`);
  `InstallResult`, `RevisionChange`, `ReplacementPlan` and `UpdateAllPlan` gain `dist`;
  `RevisionChange` gains `skipped`; `UpdateAllPlan` gains `unchanged` (`{id, version}`:
  every installed package is in `updates`, `kept_back` or `unchanged`);
  `ReplacementPlan` gains `installed_version` (the provider's installed version);
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
  `ReplacementPlan` gains `installedVersion`, `dist`; `UpdateAllPlan` also gains `skipped`;
  `PmError.NotDeployed { message, sync }` (the whole `DistSync` report) is new.
  Migration: call the free functions, read the new fields.
- `check` works without a catalog: a package the catalog does not list, or every package
  when no catalog is given, gets an origin row from `git ls-remote` followed by shallow, blobless
  release snapshots of its recorded source. `CheckStatus::NotInCatalog` (`not_in_catalog`, "not in catalog") is removed; new
  statuses `newer_than_origin`, `no_releases`, `origin_unreachable`. `CheckRow` gains
  `source` (`CheckSource::Catalog`/`Origin`) and `problem`, `Upstream` gains `source`
  (`Option<CheckSource>`) and `problem`; `check::compare`/`compare_in` are no longer pure:
  they query tag names and shallow-fetch candidate manifests for packages the
  given catalog does not list; `check::Row` (alias of `CheckRow`) is removed; `check::check`
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
  `fork` refuses an unsupported recorded source with `invalid_argument` rather than
  omitting `forkOf.source`). FFI
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
  `{"branch": NAME}`); the format stays 1 (no released gitpm wrote format 1 without it).
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
  `--force is only valid with uninstall/disable/enable/install/install-catalog/forget`.
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
  forceable for the removals, not for `replace ORIGINAL --with ORIGINAL`). Migration: `disable --force` of
  a fork is no longer needed when the original serves its consumers.
- `replace ORIGINAL --with FORK` requires ORIGINAL to be installed (`not_installed`);
  Returning now uses `replace ORIGINAL --with ORIGINAL` with the same replacement dependency
  check: an absent or disabled original refuses without a force bypass. The former
  `switch_back` API and replacement confirmation have been removed.
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
  `invalid_data`, and the `replace ORIGINAL --with ORIGINAL` line always ends `; FORK stays installed and
  enabled (…)`. Migration: remove the entry from `replacements` in `state.json`.
- No tag policing: a moved (retagged) or deleted release tag no longer fails `update`,
  `update all`, an install or update of a sibling in a shared clone, or `catalog update`
  with `version_conflict` ("release tag … changed commit", "release commit changed for
  installed version", "latest release is older than installed version; downgrading
  requires an explicit ref", "release ID@V changed; refusing to replace published
  version" are gone). Release-mode updates compare versions only and keep the installed
  commit; a moved or retracted installed release is up to date with a warning in
  `RevisionChange.warnings` and the new `Unchanged.warnings` (FFI `Unchanged.warnings`).
  `catalog update` never replaces a published record: it keeps it and reports
  `UpdateSourcesReport.warnings` (`release ID@V: tag now points to COMMIT; the published
  record is kept`, `release ID@V differs from the published record; the published record
  is kept`). `Error::NoPreviousVersion` gains `commit` (FFI `PmError.NoPreviousVersion
  .commit`), set when the previous revision's commit is gone from the clone. Rollback
  counts a tag the origin had with the same object at the last fetch as published (clones
  record them under `refs/origin-tags/`), as update does, so a fork's retained upstream
  tags no longer block it. Migration: scripts that expected `version_conflict` on a
  retagged release read the warning instead. An explicit `--ref TAG|COMMIT` with the
  installed version at another commit is accepted and pinned ("upstream changed without
  a version bump" is gone; a moved release tag of that version warns `release tag vV
  now points to COMMIT`).
- Protected catalogs ([Verified catalogs](#verified-catalogs)): only a catalog whose
  top-level `catalog.json` has `"verified": true` binds packages. An unverified catalog no
  longer records `packages.<id>.catalog` on `install-catalog` (or for pulled
  dependencies), so its packages update from their origins and the one-catalog rule
  ignores it; the refusal now says "mixing verified catalogs is not supported".
  `catalog update` into a verified catalog is refused with `invalid_argument`. Packages
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
  `CheckReport.catalogVerified`. `catalog show` prints `Verified: yes|no …`, the `check` header
  `Catalog: LOCATION (verified)` or `(unverified: discovery only)`. Migration: add
  `"verified": true` to a catalog you vouch for and register its versions with `catalog register`;
  a record that names an unverified catalog (written by an earlier gitpm) is still
  protected and its updates fail with `catalog` ("… is no longer verified") until you run
  `update ID --detach` (or verify that catalog).
- Dependency requirements follow npm, not Cargo ([Dependencies](#dependencies)): a bare
  version `1.2.3` means exactly 1.2.3 (was `^1.2.3`); `^`, `~`, `*` and space-separated
  comparators (`>=1.3.0 <2.0.0`, AND) remain; commas (`>=1.3, <2`), bare partial versions
  (`1.2`), wildcards (`1.*`), `||` and hyphen ranges are refused with
  `invalid_manifest`. Migration: write `^1.2.3` where a bare `1.2.3` meant "compatible",
  `^1.2` for `1.2`, `>=1.3.0 <2.0.0` for `>=1.3, <2`. Dependency editing preserves supplied
  requirements exactly; `init` writes no dependencies.
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
- `auth check` (`auth::check`, FFI `authCheck`) is bounded: its `git ls-remote` and the
  `core.sshCommand` lookup it makes first are each killed at 10 s and fail with `git`
  ("git ls-remote -- SOURCE: timed out after 10s"). Before, a server that accepted the
  connection and sent nothing failed with `auth` after Git's own 20-second low-speed
  limit on HTTP(S), and was waited for without end on `git://` and on a stalled SSH
  connection. It uses the same batch SSH policy as clones and fetches; the 10-second
  kill also bounds SSH stalls. Other failures of the check are still `auth`. Migration: hosts that treat every failed check as `auth` also handle `git`
  (the server did not answer in time).

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
source. Catalog data is accepted as current: a catalog hit does not query origin.
Fallback fetches candidates newest first with `--depth=1 --filter=blob:none` into a
disposable repository shared per source, reads package metadata only, and stops at the
first eligible matching release. It never checks out dist or changes installed clones
or workspace state. `check --no-catalog` bypasses selected and recorded catalogs for
Git-installed packages; build installations still use their recorded catalog. This
option changes the report only and leaves update protection intact. Only `vVERSION` tags containing the
requested package ID with the matching version count as its releases; tags without that ID
are ignored silently. Origin rows say `up to date`, `update
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
d135a7cf-1c63-549a-93cb-8f89656b2ced             0.1.0          0.2.0 (major)  yes      origin   update available

Updates: 1. Nothing was changed.

Available updates:
  d135a7cf-1c63-549a-93cb-8f89656b2ced: 0.1.0 → 0.2.0 (major)
```

A fork records the original's source in `forkOf.source`, so `check` shows the original's
newest release without a catalog too: `ce6c8a3b-b2f8-5391-8ef8-4cbb7873f5d4: fork of 7e6e8392-7be8-55ae-bd03-2b430b1c5d59 (base 1.0.0); original
at origin: 1.1.0 — newer than the fork base` (or `original: unreachable (REASON)`).

Catalog commands (`catalog show`, `catalog links`, `catalog update`, `catalog register`) and
`install-catalog` need a catalog.

## Workspace layout and `dist/`

```text
state.json              {"format": 1, "packages": …}: installed packages, replacements, forgotten records
repos/<name>-<hash>/    bare source repository shared by Git packages
worktrees/<uuid>/  detached worktree at this Git package's recorded commit
dist/<uuid>/       deployed files of an active package
.pm-txn/                an interrupted operation (finished or discarded on open)
```

A new workspace root may be empty or contain only `.DS_Store`; Finder metadata is
preserved. New workspaces publish `state.json` by writing `.state.json-<pid>-<n>.tmp`
inside the root and renaming it into place before creating `dist/`,
so the root may be a mount point of its own. An interrupted write leaves no partial
state: when the root holds nothing but such files (and `.DS_Store`), the next open
removes them under the workspace lock and finishes creation. Any other content
without `state.json` is refused with `workspace`, untouched.

`dist/<uuid>/` exists exactly when the package is installed, enabled and not
replaced by another package (`State::is_active`). A host loads every package folder under
`dist/` and needs no gitpm knowledge; `list` still reports the full status (installed,
`enabled`, `replacements`, `resolved[].replacing`) for a UI. Each `resolved[]` entry (and
the FFI `InstalledPackage`) has `active`: exactly this rule, so a host using `list` need
not derive it. `enabled` is not `active`: a replaced package stays `enabled: true` with
`active: false`. `list --provides SERVICE` keeps inactive packages (it is a status view).

- Every deploy (install, update, rollback, restore, `replace ORIGINAL --with ORIGINAL`, enable, reconcile)
  copies the folder its committed manifest selects with `dist` (default `<path>/dist`)
  from the clone's Git objects using
  `git ls-tree` and `git cat-file`, never the working tree: uncommitted edits in a clone are not deployed,
  nested folders and the executable bit are kept. Symlinks are refused with
  `unsupported`; submodules become empty directories and produce an actionable
  warning (also during catalog registration and update). A revision
  without that folder is refused (`invalid_manifest`, "release … has no <folder>/ folder")
  instead of deploying an empty folder.
  Git attributes such as `export-ignore` and `export-subst` are not applied: all
  committed files keep their committed bytes. Catalog hashes are calculated from
  the staged export, covering exactly the bytes deployed; verified catalog
  installs and protected updates check that export before committing the transaction.
- `replace X --with Y` removes `dist/X/`; nothing of `Y` is copied there; `dist/Y/` stays.
  `X` must be installed (`not_installed`) and `Y` enabled (`invalid_argument`: "Y is
  disabled; enable it first"), so a replacement never leaves nothing loaded. `replace X --with X` deploys `dist/X/` again from
  `X`'s bare repository at its installed commit (`packages.X.current.commit`, folder
  `path` for a nested package), without changing any worktree. `Y` stays deployed: two packages providing the same service is the host's
  concern.
- `disable X` removes `dist/X/`; `enable X` deploys it again the same way. Disabling a
  package that replaces another drops the binding and deploys the original again when
  it is active (a later `enable` does not bring the binding back); see [Removing a
  replacement](#removing-a-replacement) for when it is refused. `enable X` is refused
  (`dependency_conflict`, `dist/X/` not created) while a dependency of X is not
  satisfied by an active package, unless `--force`.
- `download`, and `install`, `update` and `rollback` of a replaced or disabled package,
  change state and clone only; its `dist/` appears on `replace ORIGINAL --with ORIGINAL` / `enable`.
  An explicit `replace` deploys the fork and removes the original's `dist/`.
- `forget X` removes `dist/X/` (the clone stays); `restore X` deploys it when it is
  active (a package forgotten while disabled stays disabled). Disabling, forgetting or
  uninstalling a replacement drops its bindings, so each active original it replaced is
  deployed again. Uninstalling or forgetting the original `X` keeps the binding `X → Y`:
  installing `X` again leaves it replaced (no `dist/X/`) until `replace X --with X`.
- Every change goes through the same transaction as the state, so a crash leaves state
  and `dist/` consistent; empty `dist` folders are removed.

Every mutating command (install, update, rollback, replace, enable, disable, forget,
restore, uninstall) reconciles all of `dist/` with the state in its
transaction: it deploys the missing folders of active packages and removes those of
inactive ones (disabled, replaced, forgotten or retained clones); folders of no known
package are left alone (the CLI warns about them on stderr), an existing folder of an
active package is not rewritten. A UUID path under `dist/` that is a symlink or a
file is reported the same way and nothing is written below it. Active packages below
it are skipped with `Warning: ID: dist not deployed (dist/<uuid> is not a folder)`;
unrelated commands succeed, while an operation deploying or removing a package below
that path fails with `workspace`. An active package whose
clone or installed commit is gone is skipped, so it does not block an unrelated command;
the CLI warns `Warning: ID: dist not deployed (clone missing); uninstall or forget ID to
drop its record` (both drop a record whose clone is gone without asking; `sync-dist` never
re-clones), decided by `SkippedDist.clone_missing`; a missing installed commit gets only
`Warning: ID: dist not deployed (installed commit missing from its clone)`.
Any other deploy error fails the command. Every mutating result carries this report as
`dist` (`DistSync`: `deployed`, `removed`, `unmanaged`, `skipped`, `warnings`; `enable`, `disable`,
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
This includes packages below a file or symlink at `dist/<uuid>` (previously
`sync-dist` exited 0 for them). The CLI hint tells you to remove or rename that path
and run `gitpm sync-dist` again; uninstall/forget advice applies only to a missing clone.

## Multi-package repositories

One repository can hold several packages. A root `gitpm.json` lists the package folders
as glob patterns (as in npm or Cargo workspaces); each folder has its own `plugin.json`
and a committed output folder:

```text
gitpm.json                {"packages": ["packages/*"]}
packages/store/plugin.json  {"id": "76a9b591-60f3-5c7f-9e1c-31648fc73bfd", "name": "fixture", "version": "1.2.0"}
packages/store/dist/…
packages/theme/plugin.json  {"id": "f851f609-bb81-52d6-bfef-33798624782c", "name": "fixture", "version": "0.4.0"}
packages/theme/dist/…
```

- Patterns are relative folders of `/`-separated segments; `*` matches within one segment
  (`packages/*`, `apps/gitpm-*`, or a literal `tools/cli`). No `**`, `..`, absolute paths
  or hidden folders (no segment may start with `.`, so `.cache` and `.*` are refused, and
  `*` never matches a hidden folder). Without `gitpm.json` the repository root is the only
  package, exactly as before; with it, a root `plugin.json` is not a package.
- Every package needs a canonical lowercase UUIDs ID, unique in the repository (in any letter
  case), a `plugin.json` file (not a symlink) and a committed output folder selected by manifest `dist` (not a
  symlink); packages cannot nest. Deployed files must be plain files, as for a root
  package. Only the package being installed, updated or registered must be valid at its
  commit: a defective sibling folder (no `dist/`, bad ID, …) does not block it
  (`catalog update` reports it as a warning and publishes the other packages).
- Releases belong to the repository: one `vVERSION` tag releases every package at
  that version. The package set is read from `gitpm.json` at the tagged commit,
  so a package may move between releases. Legacy `ID@VERSION` tags are ignored.
  Installing or updating one package leaves its siblings at their installed revisions.
- `gitpm install REPO ID` (or `--package ID`) installs one package. Without an ID a
  repository with several packages is refused with the list of its packages (`76a9b591-60f3-5c7f-9e1c-31648fc73bfd
  (packages/store), f851f609-bb81-52d6-bfef-33798624782c (packages/theme)`); one with a single listed package
  needs none; `--ref vVERSION` still requires a package ID when several are listed. For a single-package
  repository a given ID must match its manifest. `--ref` and `--dev` (the origin's default
  branch) work per package, and each package records its own mode.
- Workspace: packages of one source share one bare repository in
  `repos/<name>-<hash of the source>`. State records its `clone` key and the package
  folder `path` (absent for a root package). Every Git package has its own detached
  worktree at `worktrees/<uuid>`, with HEAD at its recorded commit.
  Update, rollback and restore require that worktree to be clean and at that commit;
  uninstall reports its local work for confirmation. These rules apply equally to
  root and nested packages, even if other packages use the same source.
  Removing or forgetting a package leaves sibling HEADs and files untouched.
  Forget retains the worktree; uninstall removes it and removes the bare repository
  only with its last user. Builds have no worktree, retaining source objects only
  for a previous Git revision. A root package that moves into a folder keeps its
  source key and worktree identity, changing only `path`.
- Shared clones keep `refs/gitpm/<uuid>/current` and `.../previous` for every
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
  `commit` (absent for a root package). `catalog update` publishes every package listed in
  `gitpm.json` at the source's HEAD from its repository `vVERSION` tags (one source line, all
  packages); `catalog register SRC --ref REF --package ID` registers one (the repository ref does not select a package).
- Authoring: `gitpm init NAME --into DIR` adds a package at the current shared version without a tag
  to a repository with `gitpm.json`; an existing repository without `gitpm.json` gets that
  file (`{"packages": ["packages/*"]}`), unless its root already is a package (moving it
  into a folder is up to you); a folder that does not exist becomes a single-package
  repository instead, or with `--multi` a multi-package one (`gitpm.json` and
  `packages/UUID`; also for an empty folder that is not a repository yet), so later
  `init --into DIR` calls add packages to it. `--no-release` makes only the commit, no
  tag; `gitpm commit DIR NEW_VERSION [--package ID]` releases all packages later;
  the first commit after a new `init --no-release` may use 0.1.0. The folder is `PREFIX/UUID` for the first `PREFIX/*` pattern, else
  `packages/UUID` (added to `gitpm.json` when no pattern matches it). Equal aliases are allowed because folders use generated UUIDs. Only `gitpm.json` and
  the new folder are committed; `gitpm.json` must have no uncommitted changes. Adding a package never creates a tag. gitpm never writes
  a Git identity into an existing repository: without configured `user.name` and
  `user.email` (or author and committer identity environment variables), `commit`,
  and `init` into it fail with `invalid_argument` and
  `no Git identity configured; set user.name and user.email`. Git's inferred login
  name and host email are not accepted. A retry that only creates a failed release
  tag requires identity for the annotated or signed tag too. Only a
  repository it creates itself (a new `init` folder, a new fork) gets a local demo identity
  when the user has none. `gitpm commit REPO VERSION [--package ID]` sets every package
  version, stages ALL changes of the repository, commits and creates `vVERSION`.
  The selector chooses only the package ID in the result.
- `gitpm fork ID [ID...] --into DIR` forks the entire source
  repository into `DIR`; every package gets a fresh UUID and keeps its alias, including
  packages that are not installed. The IDs identify the source, never a subset of
  packages. All input IDs must be installed
  from the same repository (otherwise the error lists each ID with its source).
  - `DIR` must be a new folder outside the workspace, without `..`, and outside
    the source repository. Existing destinations are refused. A failed fork leaves no
    folder behind, including parents it created.
  - Git clones the source recorded in the installation metadata with its history,
    using the original source as `upstream`. The new attached `fork` branch starts at
    the newest installed revision of packages from that source (the others must be
    its ancestors); each installed package's folder comes from its recorded revision.
    Other packages come from the starting revision. Local commits and edits of
    the installed clone are ignored and left unchanged. All sibling packages and
    other repository files are retained; `gitpm.json` preserves the original layout. One commit records
    the new IDs and metadata; user hooks are not run for this initial commit.
    A root package stays at the repository root.
  - Submodules remain Git links at their recorded commits with `.gitmodules`
    configuration. Run `git -C DIR submodule update --init` to populate them.
    Installation deploys the committed package output selected by `dist`; fork does not build sources
    or enable dev mode.
  - Fork preserves dependency IDs and requirements. Inspect with `gitpm packages DIR`,
    then use `gitpm dependency update PACKAGE OLD NEW REQUIREMENT --repo DIR` to
    choose a different dependency explicitly. No mapping or version requirement is
    inferred from `forkOf`. Author edits do not stage, commit, install or replace.
  - Fork preserves inherited tags and creates no release tag. It leaves packages,
    replacement bindings, workspace state and dist unchanged. Run `commit` with a new
    version, then `install DIR ID` and `replace ORIGINAL --with ID` explicitly for
    each desired package. Fork does not accept `--replace`, `--version` or `--force`.

```console
$ gitpm fork f851f609-bb81-52d6-bfef-33798624782c 955bf29f-2893-56ef-9439-369c50455ae7 --into forks/me-packages
Fork created: /…/forks/me-packages
Git history is cloned from the recorded source; the original repository is the upstream remote.
All repository packages are forked; the complete tree and gitpm.json are preserved.
d4377c45-ac7f-55f6-97f8-bd1c9ae7ca50 forked from f851f609-bb81-52d6-bfef-33798624782c (packages/theme)
09e5894a-6e33-5a1e-91e0-730a2f8cb569 forked from 955bf29f-2893-56ef-9439-369c50455ae7 (packages/panel)
…
$ gitpm packages forks/me-packages
$ gitpm dependency update 09e5894a-6e33-5a1e-91e0-730a2f8cb569 f851f609-bb81-52d6-bfef-33798624782c d4377c45-ac7f-55f6-97f8-bd1c9ae7ca50 '^1.0' --repo forks/me-packages
…
```

```console
# Example UUIDs below stand for the IDs generated by init.
$ git init -q tilr-packages
$ gitpm init store --into ./tilr-packages
$ gitpm init theme --into ./tilr-packages
$ gitpm commit ./tilr-packages 0.1.1
$ gitpm install ./tilr-packages
Error: repository has 2 packages at commit 4b377fe1c0de; choose one by ID: 76a9b591-60f3-5c7f-9e1c-31648fc73bfd (packages/76a9b591-60f3-5c7f-9e1c-31648fc73bfd), f851f609-bb81-52d6-bfef-33798624782c (packages/f851f609-bb81-52d6-bfef-33798624782c)
$ gitpm install ./tilr-packages 76a9b591-60f3-5c7f-9e1c-31648fc73bfd && gitpm install ./tilr-packages f851f609-bb81-52d6-bfef-33798624782c
installed 76a9b591-60f3-5c7f-9e1c-31648fc73bfd@0.1.1 from /…/tilr-packages
installed f851f609-bb81-52d6-bfef-33798624782c@0.1.1 from /…/tilr-packages
$ echo 'export const v = 2;' > tilr-packages/packages/76a9b591-60f3-5c7f-9e1c-31648fc73bfd/dist/index.js
$ gitpm commit ./tilr-packages 0.2.0 --package 76a9b591-60f3-5c7f-9e1c-31648fc73bfd
76a9b591-60f3-5c7f-9e1c-31648fc73bfd committed 0.2.0 (local commit and tag v0.2.0)
$ gitpm update 76a9b591-60f3-5c7f-9e1c-31648fc73bfd       # f851f609-bb81-52d6-bfef-33798624782c stays at 0.1.1
76a9b591-60f3-5c7f-9e1c-31648fc73bfd updated 0.1.1 → 0.2.0 (major)
```

Settings: each has a flag, an environment variable and a default (`gitpm --help` lists
all commands, `gitpm COMMAND --help` one). There is no config file; the old `PM_*` variables are ignored.

| Flag | Variable | Default |
| --- | --- | --- |
| `--root DIR` | `GITPM_ROOT` | `./gitpm-workspace` |
| `--catalog LOC` (folder, `catalog.json` or HTTP(S) URL) | `GITPM_CATALOG` | none (optional, see [working without a catalog](#working-without-a-catalog)): catalog commands and `install-catalog` ask for it |
| `--sources FILE` | `GITPM_SOURCES` | `./sources.txt` |

Publishing a catalog is up to you: it is a folder of static files, so any static host
works (`catalog update` and `catalog register` write only to a local folder; HTTP(S) catalogs are
read-only).

When you upload an updated catalog, go in an order that never shows readers a half-updated
tree: new `versions/<V>/release.json` files first, then each package's
`packages/<uuid>/plugin.json` index, then the directory files from the deepest level
up, and the root `catalog.json` last. Never overwrite a `release.json` that is already
published, never drop versions from an index, and do not delete objects as part of an update.

Each package index has `schema: 1`, `id`, `source`, `versions` (version-to-release
links), and `latest`: `{version, title, description, provides, tags}`. The summary
selects the newest stable release, or the newest prerelease when no stable release
exists. Missing strings are `null`, missing lists are `[]`. Writers publish the summary
and version links together under the catalog lock with an atomic rename. Directory
files remain links only.

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
(`invalid_manifest`); installing,
updating, rolling back, forking, publishing to a
catalog and listing treat such a key (and `null`) as absent, so such packages keep
working (`install`/`update` warn: `Warning: ID VERSION: plugin.json tags
must be an array of strings; ignored`; the library reports it in
`InstallResult.warnings`/`RevisionChange.warnings`, and
`Manifest::metadata_problems()`/`metadata_strict()` give the check). The keys stay in the file as
they were: `commit` and `fork` rewrite version and identity fields, while
`dependency` explicitly edits dependencies. All operations preserve metadata
(also the string form of `provides`).
`init` writes `"title": "NAME"`.

Shape rule, everywhere gitpm reports metadata (`list` `resolved[]` and
`packages.<id>.metadata`, `catalog show`/`browse` `PackageSummary`, `CatalogRelease`, the FFI
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
- Catalog: `catalog update` and `catalog register` publish the latest release's metadata
  in the package index. `catalog show` and `browse` (`PackageSummary`, FFI `browseCatalog`)
  read that summary: at most one index request per listed package, no `release.json`
  requests, and at most 40 overview entries. A missing, malformed, or mismatched summary
  leaves only that package without metadata. Installs still verify `release.json`.
- FFI: `InstalledPackage`, `PackageSummary` and `CatalogRelease` have `title`,
  `description` (`String?`), `provides` and `tags` (`[String]`).

```console
$ cat packages/theme/plugin.json
{"id": "f851f609-bb81-52d6-bfef-33798624782c", "name": "fixture", "version": "1.0.0", "title": "Theme", "provides": "theme", "tags": ["ui"]}
$ gitpm list --provides theme | jq '.resolved[] | {id, title, description, provides, tags, clone}'
{
  "id": "f851f609-bb81-52d6-bfef-33798624782c",
  "title": "Theme",
  "description": null,
  "provides": ["theme"],
  "tags": ["ui"],
  "clone": "tilr-packages-4b377fe1c0de1234"
}
```

## Rules of the library

- A crash means process death or power loss, including interruption before a call
  returns. Workspace commit syncs staged `state.json`, then the journal and its
  containing directory and workspace root before moving managed paths. Publishing
  state syncs its source and destination directories; transaction cleanup syncs the
  workspace root. A published journal is rolled forward on reopen after process
  interruption. Repository, worktree and dist files and directory trees are not
  synced by gitpm: their durability after power loss depends on Git and the OS.
  Workspace creation does not sync ancestor directories. Sync errors for gitpm
  metadata are returned to the caller.

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
- All Git operations share batch SSH with keepalives and
  `StrictHostKeyChecking=accept-new` unless `Options.custom_ssh` is true or Git config
  defines `core.sshCommand`. Hosts pass `custom_ssh` explicitly; Git subprocesses
  inherit the host environment.
- No backward compatibility before 1.0: package IDs are always canonical lowercase UUIDs, release
  tags always `vVERSION`, catalogs always the static file tree; there are no aliases,
  migrations or compatibility readers, and the FFI promises no stability. Every breaking
  change is listed under "Changed in X.Y".
- `state.json` carries `"format"` (`gitpm::STATE_FORMAT`). A gitpm reads and writes
  exactly its own format: any other one (or a file without `format`) is refused with
  `unsupported` and never rewritten, in either direction (no downgrade, no migration). A
  workspace of another format is re-created: install its packages again into a new
  workspace root. Before the first public release, all gitpm file formats stay at version 1
  and evolve in place without backward compatibility or bumps. Afterward, bump a format
  only when gitpm from the previous public release would misread the change. Bump it as
  a separate step in the release milestone, at most once per release.

  | gitpm | state format |
  | --- | --- |
  | Before the first public release | 1: packages under `packages`, Git clones in `repos/` |

- A complete workspace can be copied or moved to another root. Opening it through a
  Manager (for example, `gitpm --root NEW_ROOT sync-dist`) repairs stale Git worktree
  links under the workspace lock, including retained forgotten packages. Read-only
  commands such as `list` read state without repairing links. Repair preserves local
  work and the source workspace; external links and recorded source locations keep
  their existing targets.

## API overview

```rust
use gitpm::{Manager, Options};

let pm = Manager::open_with("/path/to/workspace", Options { catalog: Some(url), ..Default::default() })?;
pm.install("git@github.com:acme/demo1.git")?;          // latest release tag
use gitpm::{InstallTarget, Target};
pm.install_package(src, None, InstallTarget::Ref("v0.2.0".into()))?; // pinned
pm.install_package(src, None, InstallTarget::Dev)?;    // follows the origin's default branch
pm.install_package(src, Some("76a9b591-60f3-5c7f-9e1c-31648fc73bfd"), InstallTarget::Release)?; // one package of a gitpm.json repo
pm.install_from_catalog("645eb52e-b765-5211-a9b5-d9e7e23dbd85", None)?;          // exact catalog release, hashes verified
pm.download(src, None, InstallTarget::Release)?;       // recorded disabled, deps too, nothing deployed
pm.download_from_catalog("645eb52e-b765-5211-a9b5-d9e7e23dbd85", None)?;         // the same from the catalog
pm.enable_with(id, /* force: enable over inactive dependencies */ false)?;
pm.update("645eb52e-b765-5211-a9b5-d9e7e23dbd85")?;                              // by the recorded mode (pinned: unchanged_reason)
pm.update_to(id, Target::Release)?;  pm.update_to(id, Target::Ref("feature".into()))?;
pm.update_with(id, Target::Recorded, UpdateOptions { detach: true })?; // leave a verified catalog
pm.rollback(id)?;  pm.enable(id)?;  pm.disable(id)?;  pm.forget(id)?;  pm.restore(id)?;
let plan = pm.uninstall_plan(id)?;                    // deletes + lost_work + dependents
if let Some(refusal) = plan.refusal() { return Err(refusal) } // has_dependents with lost_work
let confirmed = if user_said_yes { plan.lost_work.clone() } else { Vec::new() };
let dist = pm.uninstall(id, &confirmed, /* force: ignore dependents */ false)?;
// Every mutating result carries its dist report (`dist`, or the returned `DistSync`).
// Forks: the fork repository is always explicit (new folder).
pm.fork_into(id, Path::new("/work/demo1-fork"))?;   // sets forkOf + replaces
let repo = Path::new("/work/forks/me-packages");               // created as a new folder
let fork = pm.fork_packages(&["f851f609-bb81-52d6-bfef-33798624782c", "955bf29f-2893-56ef-9439-369c50455ae7"], repo)?; // one repo, dependencies preserved
let packages = gitpm::repository_packages(repo, Some(pm.root()))?;
// The caller chooses IDs and requirements; no forkOf mapping is inferred.
let edited = gitpm::edit_dependency(repo, consumer_id, &gitpm::DependencyEdit::Update {
    dependency: original_dependency_id.into(),
    new_dependency: fork_dependency_id.into(),
    requirement: "^1.0".into(),
})?;
// Explicitly release all fork packages, then install only the desired package.
gitpm::scaffold::commit_package(repo, "2.0.0", Some("d4377c45-ac7f-55f6-97f8-bd1c9ae7ca50"))?;
pm.install_package(repo.to_str().unwrap(), Some("d4377c45-ac7f-55f6-97f8-bd1c9ae7ca50"), InstallTarget::Release)?;
let check = pm.replace_check("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "db33e43e-e652-5767-8e21-27fb3cef9723")?; // issues + missing catalog releases
// Only an explicit caller decision installs the missing dependencies:
if !check.missing.is_empty() { pm.replace_resolve("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "db33e43e-e652-5767-8e21-27fb3cef9723")?; }
pm.replace("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "db33e43e-e652-5767-8e21-27fb3cef9723")?; // rechecks; no confirm, force or auto-install
pm.replace("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9")?; // remove binding with the same check
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
| `check` | `read_state` (does not create/lock), `compare`/`compare_in` (read remote refs and verify release manifests in a temporary clone for packages the catalog does not list), `check(root, catalog: Option)` → `CheckReport` (`catalog_verified`); `CheckStatus::message()`, `CheckRow::message()`, `CheckSource` |
| `auth` | `check(source, &Options)` — `git ls-remote` only; refuses credentials in the URL, stores nothing (SSH itself records a new host's key, `accept-new`); failures are `auth`, except `git` when the check does not finish in 10 s |
| `scaffold` | `init(dir, name)` (the `init --into` rule; the only way to create a package besides `create_package`), `init_with` (`InitOptions { multi, release }`), `create_package` (into a multi-package repo), `commit`, `commit_package` (`Commit { package, version, tag }`), `release_current`, `set_remote`, `shell_quote` |
| `packages` | `config_at`, `layout_at` (packages and defective folders of a commit), `packages_at` (strict), `find_at` (one package of a commit), `read_config`, `packages_in` (working tree), `matches`, `validate_pattern` |
| `identity` | `valid_part` (safe alias), `new_id` (fresh UUID), `release_tag` |
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
keeps no `forgotten` record then). The caller decides how to ask the user. IDs must use canonical
lowercase UUIDs: even on a case-insensitive file system, `D135A7CF-1C63-549A-93CB-8F89656B2CED`
is rejected with `invalid_id` rather than resolving to `d135a7cf-1c63-549a-93cb-8f89656b2ced`.

### Versions and pre-releases

A version is SemVer `MAJOR.MINOR.PATCH` with an optional pre-release `-PRE`
(`1.3.0-beta.1`); build metadata is refused. Versions are ordered by SemVer precedence
(`1.0.0-alpha < 1.0.0-beta < 1.0.0-beta.2 < 1.0.0`), and `ID@1.3.0-beta.1` is a release
tag like any other (`gitpm commit REPO 1.3.0-beta.1` creates it, `catalog update`
publishes it). Pre-releases are opt-in, as in npm:

- `install`, `install-catalog` without `--version`, `update` and `update all` never pick
  one; `check` and `catalog show` show it (`check`: `up to date; pre-release 1.3.0-beta.1`).
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
$ gitpm update d135a7cf-1c63-549a-93cb-8f89656b2ced
d135a7cf-1c63-549a-93cb-8f89656b2ced updated 1.2.3 → 2.0.0 (major)
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
$ gitpm uninstall cfbc4d17-8707-5571-853d-c9136a7f9f21 --force
cfbc4d17-8707-5571-853d-c9136a7f9f21 uninstalled
$ gitpm update all
b879842f-16c2-558f-9d03-a271dc901f57 skipped at 1.0.0 (dependency cfbc4d17-8707-5571-853d-c9136a7f9f21 is not installed)
cde71d60-4de0-5f8d-a3c9-26b40d4af02a updated 1.0.0 → 1.1.0
0e4950ff-9cb6-54f3-bf22-f5a9f71129c1 kept back at 1.0.0 (latest 2.0.0: dependency constraints)
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

A fork declares `"replaces": {"id": "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "version": "1.4.0"}` (`fork` sets
it to the forked version). `replace(original, fork)` sets the single global binding
`state.replacements[original] = fork`. Every consumer resolves the original through
the fork at its declared version. The original must be installed; the fork must be
installed, enabled and declare that it replaces the original. Cycles are refused.

`replace_check(original, provider)` is a dry run over the entire resulting active
workspace. It returns `ReplacementCheck { plan, issues, missing, resolution_error }`:
issues name each consumer and its unsatisfied dependency; missing maps absent package
IDs to selected catalog versions for the proposed provider, including transitives. Unrelated
existing broken consumers remain in issues but do not block replace or resolve; their
missing dependencies are not reinstalled. When no catalog
solution exists, missing is empty and resolution_error explains the failure.
Installed versions stay fixed. Disabled or incompatible installed providers require
explicit user action. This is separate from the existing update `check`.

`replace_resolve(original, provider)` recomputes the check, then explicitly installs
missing dependencies from the configured catalog in one transaction. Its result contains
`installed`, the updated `check`, and `dist`. It does not enable or update installed
packages or apply the binding. If no consistent solution exists it changes nothing.
A subsequent `replace` checks current state again, installs nothing, and refuses incompatibility in the provider dependency closure or requirements of
active consumers on the original without changing workspace state, clones or dist. Consumer version
mismatches use `incompatible_replacement`; other dependency problems use
`dependency_conflict`. There is no confirmation or force bypass.

Passing the original as provider without an existing binding succeeds after the same
checks and prints `ORIGINAL already uses original VERSION`.

The CLI equivalents are:

```sh
gitpm replace-check c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 --with db33e43e-e652-5767-8e21-27fb3cef9723   # JSON diagnostics, no changes
gitpm replace-resolve c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 --with db33e43e-e652-5767-8e21-27fb3cef9723 # explicit missing installs
gitpm replace c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 --with db33e43e-e652-5767-8e21-27fb3cef9723         # apply only when consistent
gitpm replace c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 --with c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9       # return to the original
```

Returning removes the binding with the same consistency check. The original must be
installed and enabled; the fork stays installed and enabled. `--back`, `switch_back`
and `switch_back_plan` have been removed. Replace accepts no `--force`.

#### Removing a replacement

`disable`, `forget` and `uninstall` of an active fork drop its binding and deploy the
original again when active. These removal operations retain their existing force
rules: `has_dependents` when consumers would break, and `dependency_conflict` when the
reactivated original has inactive dependencies. A later enable does not restore the
binding. Uninstalling or forgetting the original keeps the binding; reinstalling it
leaves it replaced until `replace ORIGINAL --with ORIGINAL`.

### Catalog

A catalog is optional ([working without a catalog](#working-without-a-catalog)):
without one, `browse_catalog`, `catalog_release` and `install_from_catalog` fail with
`invalid_argument` ("no catalog configured"); `check` then reads each package's origin. A configured local location that
does not exist is not "no catalog": `Manager::open_with` and every catalog reader refuse
it with `invalid_argument` ("catalog location PATH does not exist") before anything is
created; only `catalog update` and `catalog register` create a missing local catalog. A workspace uses one verified catalog
([Verified catalogs](#verified-catalogs), `catalog::is_verified`). `install_from_catalog`
records the canonical location of a verified catalog per package (`packages.<id>.catalog`,
the package is then protected) and refuses to install from a second verified catalog;
an unverified catalog records nothing. `update_to`/`update_with` and `update_all` move a
protected package only to the versions its catalog lists (`UnchangedReason::NotInCatalog`
when the origin is ahead), `UpdateOptions { detach: true }` drops the protection. `check`
compares a protected package with its own catalog when none is given, without querying
origin. `check --no-catalog` instead reports origin release data for Git packages. It reports
packages from another catalog as `other_catalog` instead of comparing them, and says in
`CheckReport.catalog_verified` whether the given catalog is verified; a package the catalog does
not list gets an origin row (`CheckRow.source` is `CheckSource::Catalog` or `Origin`,
`CheckRow.problem` explains `origin_unreachable`). For a fork it adds
`upstream { id, base, latest, newer, source }`: the original's latest release against the
version the fork started from, from the catalog when it lists the original, otherwise from
the tags of `forkOf.source` (`source` `None` when neither is known; `problem` when
`forkOf.source` cannot be read). `compare`/`compare_in` and `check` run `git ls-remote` for
every Git package (and fork original) the catalog does not list, then shallow-fetch
candidate tags with blob filtering to verify package manifests.

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
let plan = try pm.uninstallPlan(id: "645eb52e-b765-5211-a9b5-d9e7e23dbd85")
try uninstallRefusal(plan: plan)   // throws HasDependents (with lostWork) when consumers would break
if plan.lostWork.isEmpty || userConfirmed(plan.lostWork.map(describeLostWork)) {
    try pm.uninstall(id: "645eb52e-b765-5211-a9b5-d9e7e23dbd85", confirmed: plan.lostWork)
}
```

FFI surface: `PackageManager` (open, root, list, browseCatalog, catalogRelease,
install(source, target: `InstallTarget`, package; defaults to nil), installFromCatalog,
download(source, target, package), downloadFromCatalog,
update(id, target: `Target`, `.recorded` follows the recorded mode), updateAllPlan,
updateAll (`UpdateAllPlan` with `unchanged`, each with its `reason`, and `dist`), rollback, enable(force), disable(force),
forget(force), restore, uninstallPlan, uninstall(confirmed, force), fork
(one source ID, whole repository; `ForkResult`), forkPackages (`PackageFork`),
replacePlan, replace (without confirmation), replaceCheck (`ReplacementCheck`),
replaceResolve (`ReplacementResolution`); replacement plans carry
`installedVersion`, `dist`; syncDistPlan, syncDist (`DistSync`: deployed,
removed, unmanaged, skipped, warnings)); every mutating call returns its `DistSync` report
(`enable`/`disable`/`uninstall` return it, `forget` returns `ForgetResult { cloneKept,
dist }`, `InstallResult`, `RevisionChange`,
`ReplacementPlan` and `UpdateAllPlan` carry it as `dist`); `InstallResult` and
`RevisionChange` carry `skipped` (broken release tags) and `warnings`, as the CLI prints
them; `RevisionChange.unchangedReason` says why an update left a package (`pinned`);
`InstalledPackage.mode`, `PackageRecord.mode` and `CheckRow.mode` carry the recorded
`Mode`; `force` defaults to false; `InstalledPackage.path` and `CatalogRelease.path` name
the package folder; `InstalledPackage.clone` names the shared clone; `InstalledPackage`,
`PackageSummary` and `CatalogRelease` carry the [metadata](#package-metadata); the fork
records `PackageFork`, `ForkedPackage` and `PackageDetails` mirror the library
types of the same names. The FFI-only `ForkResult` wraps the repository path returned
by library `fork_into`. Free functions, which read without opening, locking or creating the
workspace: `list(root, provides)` and `readState(root)` (`ListView`: `format`,
`packages`, `replacements`, `forgotten`, `resolved`), `check(root, catalog, customSsh)`
(`CheckReport`: rows with `source` `.catalog`/`.origin` and `problem`, fork `upstream`
with its `source`, replacements, unsatisfied dependencies; `catalog` may be `nil`), `catalogLinks(location)`,
`authCheck(source, customSsh)`, `browseCatalog`, `catalogRelease`, `describeLostWork`,
`uninstallRefusal(plan)`.
Every CLI command that reads or changes a workspace, reads a catalog or checks access has
its FFI call; `init`, `commit`, `remote`, `catalog update` and `catalog register` stay library and
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

Unit tests cover pure parts directly (UUID and alias rules, version keys and ordering,
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
`openspec/coverage/coverage.md` and `coverage.json` locally (the directory is ignored by Git;
nothing in it is committed). Details, statuses (`XFAIL` for known defects) and how to write
cases: [itests/README.md](itests/README.md).

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

### Inspect packages and edit dependencies

`gitpm list` (library `Manager::list` / `list_with`, FFI `list` / `readState`) returns
installed snapshots, including each package's `id`, `name`, `installed: true`,
optional `forkOf`, and dependency IDs and requirements with resolution status.

`gitpm packages DIR` inspects current author manifests in a repository, including
root and nested packages, originals and forks. Its JSON entries have `id`, `name`,
`version`, `path`, `forkOf` (null when absent), `dependencies` (an ID-to-requirement
object, empty when absent), and `installed`. The configured `--root` determines
membership by ID without opening, locking or creating the workspace. Library
`repository_packages(directory, root)` and FFI `repositoryPackages(directory, root)`
accept an optional workspace root; omitting it reports all entries uninstalled.
No built output is required to inspect or edit an author manifest.

```sh
gitpm dependency add PACKAGE DEPENDENCY '^1.0' --repo DIR
gitpm dependency update PACKAGE OLD_DEPENDENCY NEW_DEPENDENCY '~2.1' --repo DIR
gitpm dependency remove PACKAGE DEPENDENCY --repo DIR
```

Omit `--repo DIR` to edit an installed Git package's editable worktree under the
workspace lock. Library `edit_dependency(directory, package, &DependencyEdit)` and
`Manager::edit_dependency(id, &DependencyEdit)` correspond to the FFI free function
`editDependency(directory, package, edit)` and method `PackageManager.editDependency`.
`DependencyEdit` has `Add`, `Update` and `Remove` cases. Update accepts the same ID to
change only its requirement. Add refuses an existing dependency; update/remove refuse
a missing dependency; update refuses an occupied destination ID (`invalid_argument`).
Invalid IDs, requirements and self-dependencies fail validation before writing.

Each edit returns the current package description, preserving unrelated manifest
fields and unknown keys. Author-path edits report `installed: false` because they
have no workspace input; manager edits report true. Installed build-only packages
and links do not own editable Git worktrees and return `unsupported`. Edits change
only the selected manifest, leaving recorded installation snapshots and deployed
output intact. Release and update/install remain explicit; normal local-changes
checks continue to protect edited worktrees.
