# install-uninstall Specification

## Purpose

Defines how packages enter and leave a workspace: installing from a Git source or the catalog, idempotent re-install, the three removal levels (disable, forget, uninstall), restore, and the plan-and-confirm protocol that protects local work.

## Requirements

### Requirement: Install the latest release of a Git source
`install SOURCE` (`Manager::install`) SHALL clone the Git source (path or URL), select the numerically newest valid release tag (see package-ids), check it out detached, verify the manifest and deploy its package dist; missing dependencies are installed as in dependencies. Untagged commits SHALL be ignored; a source without release tags SHALL fail with `no_releases`. Verified by `stable_install_and_update_ignore_untagged_head_and_sort_versions`.

#### Scenario: Newest numeric tag wins
- **WHEN** a repository has tags `acme/greeter@0.9.0` and `acme/greeter@0.10.0` and a newer untagged commit
- **THEN** install checks out `0.10.0` and prints `installed acme/greeter@0.10.0 from SOURCE`

#### Scenario: Repository without releases
- **WHEN** the source has no release tags
- **THEN** install fails with `no_releases` and the workspace is unchanged

### Requirement: Install an explicit ref or a branch
`install SOURCE --ref REF` SHALL resolve REF against the origin (tag, origin branch, commit) and record the mode (workspace-layout): a tag or commit pins the package, a branch makes it follow that branch; `--dev` SHALL follow the origin's default branch. Without either, the newest release is installed in release mode. A ref starting with `-` is refused as in git-safety. Verified by `workspace::install_records_the_ref_mode` and `model::resolve_ref_refuses_option_like_refs_and_resolves_commits`.

#### Scenario: One install target in the library
- **WHEN** the user runs `gitpm install SOURCE --ref x --dev`, or `--channel dev`
- **THEN** the first is a CLI usage error (`--dev conflicts with --ref`) and `--channel` is an unknown option; the library takes one install target, so the combination cannot reach it

#### Scenario: Install by exact commit
- **WHEN** the user runs `install SOURCE --ref <40-hex commit>`
- **THEN** that commit is installed, recorded as `current.commit`, and the record says the package is pinned to that commit

#### Scenario: Install a branch
- **WHEN** the origin has the branch `feature` and the user runs `install SOURCE --ref feature`
- **THEN** the fresh clone resolves the fetched `origin/feature`, installs its HEAD and records branch mode `feature`

#### Scenario: Install the default branch
- **WHEN** the origin's default branch is `main` and the user runs `install SOURCE --dev`
- **THEN** the HEAD of `origin/main` is installed and the record says branch mode `main`

### Requirement: Install is idempotent for the same source
Installing an installed package again from the same source SHALL change nothing and succeed, reporting `ID already installed (VERSION)` (with a mode: `(VERSION, pinned at REF)`), with `; run update` when the source resolves to another revision or mode (`; requested --ref REF — run update ID --ref REF`). "Same source" SHALL mean the same string as recorded after canonicalizing a local path. `InstallResult` SHALL carry `already_installed`, `available`, the recorded `mode` and `requested_mode`.

#### Scenario: Repeat install
- **WHEN** `acme/greeter` 0.2.0 is installed from `./greeter` and the user runs `gitpm install ./greeter`
- **THEN** it prints `acme/greeter already installed (0.2.0)`, exits 0 and the workspace is byte-identical
- **AND** this is covered by `install_again_reports_already_installed_and_changes_nothing`

#### Scenario: Newer release available
- **WHEN** the source has a newer release than the installed one
- **THEN** it prints `acme/greeter already installed (0.2.0); run update` and changes nothing

### Requirement: Same ID from another source is refused
Installing an ID that is installed from a different source (including a `file://` URL against a plain path, or a URL with and without `.git`) SHALL fail with `already_installed`. Installing an ID that is forgotten but retained SHALL be refused with `already_installed` until it is restored or uninstalled. Verified by `forgotten_repository_blocks_reinstall_until_restored`.

#### Scenario: Other source
- **WHEN** `acme/greeter` is installed from `./greeter` and the user installs `file:///…/greeter`
- **THEN** the command fails with `already_installed`

#### Scenario: Forgotten but retained
- **WHEN** `acme/greeter` was forgotten and its clone `packages/acme/greeter` is retained, and the user installs it again
- **THEN** the command fails with `already_installed` ("acme/greeter was forgotten and its clone is kept") and the workspace is unchanged
- **AND** the message names no command; the CLI adds a `Hint:` line naming `gitpm restore ID` and `gitpm uninstall ID` (the same holds for a forgotten package of a shared clone)

### Requirement: Install from the catalog
`install-catalog ID [--version V]` (`Manager::install_from_catalog`) SHALL install the exact catalog release (latest if omitted): clone its source, check out the recorded commit, and verify that the manifest and the package dist file hashes equal the catalog's before any workspace change. A mismatch SHALL fail with `catalog`. Verified by `catalog_manifest_is_verified_before_install` and `workspace::install_from_catalog_verifies_and_check_compares`.

#### Scenario: Tampered dist
- **WHEN** the catalog's file hashes do not match the release commit's `dist/`
- **THEN** install fails with `catalog` ("catalog file hashes do not match git dist") and nothing is installed
- **AND** installing from a second catalog is refused as specified in catalog ("One catalog per workspace")

### Requirement: Refused input creates nothing
Input SHALL be validated before anything is created: the CLI checks argv syntax and parses IDs and versions with the library's parsers before opening, and `open_with` validates Options first (workspace-layout). A refused install SHALL leave no clone, no `dist/<id>`, no state entry and no `.pm-txn` (errors). Verified by `usage_errors_name_the_problem_and_create_nothing` and `workspace::install_refusals_leave_the_workspace_unchanged`.

#### Scenario: Invalid ID on a fresh root
- **WHEN** the user runs `gitpm --root new-ws update ../evil`
- **THEN** the command fails with `invalid_id` and `new-ws` is not created

### Requirement: Three removal levels
gitpm SHALL offer three removal levels: `disable` (keep everything, set `enabled: false`, remove `dist/<id>`), `forget` (drop the package from `packages` in state, keep the clone and a `forgotten` record, remove `dist/<id>`) and `uninstall` (remove the state entry, the clone and `dist/<id>`). `restore ID` SHALL record a forgotten package in state again. All three removals are guarded as in dependencies ("Dependents guard removal"). Verified by `lifecycle_and_three_removal_levels`.

#### Scenario: Uninstall one package of a shared clone
- **WHEN** another package still uses the shared clone of the package being uninstalled
- **THEN** the clone stays; it is removed only with its last package (multi-package-repos, "One shared clone per source")

#### Scenario: Forget then restore
- **WHEN** the user runs `forget acme/greeter` then `restore acme/greeter`
- **THEN** the first prints `acme/greeter forgotten (clone kept; restore re-registers it)`, the second `acme/greeter restored 0.2.0`, and `dist/acme/greeter/` is back

#### Scenario: Restore without a clone
- **WHEN** the user restores an ID that has no `forgotten` record
- **THEN** the command fails with `repository_not_found`, as does a forgotten package whose clone (own or shared) is gone (multi-package-repos, "Forgotten packages of shared clones")

### Requirement: Restore preconditions
`restore ID` SHALL record the package again from its `forgotten` record (own or shared clone) as it was; branch mode takes HEAD when it moved on along the origin's branch (dependencies checked). It SHALL refuse with `local_changes` on lost work and with `already_installed` when the ID is recorded, and print `ID restored V`. Verified by `workspace::restore_keeps_the_forgotten_record`, `restore_in_a_shared_clone_ignores_another_branch_s_revision` and `branch_mode_restore_checks_dependencies`.

#### Scenario: Restored record
- **WHEN** a clean retained own clone of `acme/greeter`, forgotten while disabled and bound to a catalog, is restored
- **THEN** state records the forgotten revisions, `catalog`, `enabled: false` and mode unchanged, and nothing is deployed because it is disabled

#### Scenario: Dirty retained clone
- **WHEN** `acme/greeter` was forgotten, a file in `packages/acme/greeter` was edited and the user runs `restore acme/greeter`
- **THEN** the command fails with `local_changes` and the package stays out of state

#### Scenario: Unpublished commit in a retained clone
- **WHEN** a commit was made in the retained clone that no remote branch contains, and the user restores it
- **THEN** restore fails with `local_changes` listing `UnpublishedCommit`, also in branch mode

#### Scenario: Branch mode restore
- **WHEN** a package in branch mode `main` was forgotten and its clone's HEAD is a published commit newer than the recorded one
- **THEN** `restore` records that HEAD as `current` and keeps branch mode `main`

### Requirement: Uninstall is planned and confirmed
`uninstall_plan(id)` SHALL be read-only and list `deletes` (paths), `registered`, `dependents` and `lost_work`. Single source for the lost-work items: `Modified`, `Untracked`, `Ignored`, `HeadMoved`, `UnpublishedCommit`, `UnpublishedBranch`, `UnpublishedTag`, `Stash`. `uninstall_with(id, confirm, force)` (FFI `uninstall(confirm, force)`) SHALL refuse with `local_changes` (listing the items) when work would be lost unless `confirm`. A clean clone SHALL be removed without confirmation.

#### Scenario: Dependents without force
- **WHEN** the package has dependents and `force` is false
- **THEN** `uninstall_with` fails with `has_dependents` (dependencies); `uninstall(id, confirm)` is `uninstall_with(id, confirm, false)`

#### Scenario: Stash alone blocks
- **WHEN** the clone is clean except for a `git stash` entry and uninstall is not confirmed
- **THEN** it fails with `local_changes` listing the stash and nothing is deleted
- **AND** this is covered by `uninstall::plan_lists_each_lost_work_type_including_stash_without_changes`

#### Scenario: Confirmed uninstall
- **WHEN** the plan has lost work and the caller confirms
- **THEN** the clone, `dist/<id>` and the state entry are deleted

### Requirement: CLI confirmation flow for uninstall
Without `--force`, the CLI SHALL list what `uninstall_plan` reports and ask `Delete anyway? [y/N]` on stderr on a TTY; anything but `y`/`yes` (EOF included) SHALL change nothing and fail with `uninstall cancelled; nothing was changed`. Without a TTY it calls the library unconfirmed, which refuses. `--force` deletes without asking and ignores dependents. Verified by `uninstall_on_tty_asks_before_losing_work` and `uninstall_without_tty_refuses_and_lists_lost_work_including_stash`.

#### Scenario: No TTY
- **WHEN** a clone has modified files and `gitpm uninstall ID` runs with stdin not a terminal
- **THEN** it prints `Error: …`, one `  - …` line per lost item and `Error kind: local_changes`, and the exit status is 1
- **AND** a declined prompt is a CLI error without `Error kind` (exit status 1), and `--force` sets both `confirm` and `force`

### Requirement: Only manager-owned files are deleted
Uninstall SHALL delete only what the workspace owns (the clone inside the workspace, `dist/<id>`, the state entry); a fork repository or author repository outside the workspace MUST NOT be touched.

#### Scenario: Uninstalling a fork
- **WHEN** `me/hello` was installed from `./hello-fork` and is uninstalled
- **THEN** `./hello-fork` still exists unchanged
