# install-uninstall Specification

## Purpose

Defines how packages enter and leave a workspace: installing from a Git source or the catalog, idempotent re-install, the three removal levels (disable, forget, uninstall), restore, and the plan-and-confirm protocol that protects local work.

## Requirements

### Requirement: Install the latest release of a Git source
`install SOURCE` (`Manager::install`) SHALL clone the Git source (path or URL), select the newest valid release tag that is not a pre-release (package-ids), check it out detached, verify the manifest and deploy its package dist; missing dependencies are installed as in dependencies. Untagged commits SHALL be ignored; a source without release tags, or with pre-release tags only, SHALL fail with `no_releases`. Verified by `stable_install_and_update_ignore_untagged_head_and_sort_versions`.

#### Scenario: Newest numeric tag wins
- **WHEN** a repository has tags `acme/greeter@0.9.0` and `acme/greeter@0.10.0` and a newer untagged commit
- **THEN** install checks out `0.10.0` and prints `installed acme/greeter@0.10.0 from SOURCE`

#### Scenario: Repository without releases
- **WHEN** the source has no release tags
- **THEN** install fails with `no_releases` and the workspace is unchanged

#### Scenario: Install enables pulled dependencies
- **WHEN** `install-catalog app` pulls the missing `lib` from the catalog
- **THEN** both are recorded `enabled: true` and deployed in the same transaction: an install is a download plus enabling the package and the dependencies it downloaded

#### Scenario: Forced install next to a disabled dependency
- **WHEN** `lib` is installed but disabled and the user runs `install ./app --force` (`Manager::install_package_with` with `force`; `install-catalog app --force`, `install_from_catalog_with`), where `app` requires `lib ^1`
- **THEN** `app` is recorded enabled and deployed although its dependency is not active (dependencies, "enable needs active dependencies"), `lib` stays disabled, missing dependencies are installed as usual, and `list` reports `app`'s dependency on `lib` unsatisfied (`forced_install_and_install_over_a_download`)

#### Scenario: Pre-release tag is not picked
- **WHEN** a repository has `acme/greeter@1.2.0` and the newer `acme/greeter@1.3.0-beta.1`
- **THEN** `install` installs 1.2.0; `install SOURCE --ref acme/greeter@1.3.0-beta.1` installs the beta (pinned, as every `--ref` tag)

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
Installing an installed and enabled package again from the same source SHALL change nothing and succeed, reporting `ID already installed (VERSION)` (with a mode: `(VERSION, pinned at REF)`), with `; run update` when the source resolves to another revision or mode (`; requested --ref REF — run update ID --ref REF`). `InstallResult` SHALL carry `already_installed`, `available`, the recorded `mode`, `requested_mode` and `enabled`.

#### Scenario: Repeat install
- **WHEN** `acme/greeter` 0.2.0 is installed from `./greeter` and the user runs `gitpm install ./greeter`
- **THEN** it prints `acme/greeter already installed (0.2.0)`, exits 0 and the workspace is byte-identical
- **AND** this is covered by `install_again_reports_already_installed_and_changes_nothing`

#### Scenario: Newer release available
- **WHEN** the source has a newer release than the installed one
- **THEN** it prints `acme/greeter already installed (0.2.0); run update` and changes nothing

#### Scenario: Same source
- **WHEN** the recorded source is `/…/greeter` and the user installs `./greeter`
- **THEN** it is the same source: "same source" means the same string as recorded after canonicalizing a local path

#### Scenario: Install over a download
- **WHEN** `app` was downloaded from `./app` and the user runs `install ./app`
- **THEN** it fails with `dependency_conflict` while a dependency is not active (dependencies, "enable needs active dependencies"); with every dependency active, or with `--force`, `app` is enabled and deployed at its recorded revision and mode, and the line is `installed app@V from SOURCE`; verified by `forced_install_and_install_over_a_download`
- **AND** the result has `enabled` true and `already_installed` false, `available` and `requested_mode` as for an installed package, and a warning when the requested mode differs from the recorded one (`install_over_a_download_reports_the_requested_mode`)

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
`install-catalog ID [--version V]` (`Manager::install_from_catalog`) SHALL install the exact release (latest if omitted), verifying the manifest and file hashes before any change. Git installs SHALL check out the recorded source commit; builds SHALL verify archive SHA-256 and deploy without cloning. Mismatches SHALL fail with `catalog`, unavailable artifacts with `http`. Only verified catalogs protect. Verified by `catalog_manifest_is_verified_before_install`.

#### Scenario: Tampered dist
- **WHEN** the catalog's file hashes do not match the release commit's `dist/`
- **THEN** install fails with `catalog` ("catalog file hashes do not match git dist") and nothing is installed
- **AND** installing from a second catalog is refused as specified in catalog ("One catalog per workspace")

#### Scenario: Catalog install over a direct install
- **WHEN** `acme/greeter` 0.1.0 was installed or downloaded with `install`/`download ./greeter` and the user runs `install-catalog acme/greeter` from a verified catalog
- **THEN** the installed revision must be the catalog's record of 0.1.0 (same source and commit, package folder, manifest and file hashes): the record gets `catalog` (a download is also enabled) and the result warns `acme/greeter is now protected by catalog LOCATION`
- **AND** another commit or other files fail with `catalog` ("… it cannot be protected by that catalog") and nothing changes; this is covered by `workspace::install_from_a_verified_catalog_protects_a_direct_install_of_its_release`

#### Scenario: Install from an unverified catalog
- **WHEN** the catalog has no `"verified": true` and the user runs `install-catalog acme/greeter`
- **THEN** the release is verified as above, the record has no `catalog`, and a Git installation later updates from the origin; a build installation follows its separately recorded catalog location

#### Scenario: Install a build with unavailable source
- **WHEN** a catalog release has a valid build and its source repository is unavailable
- **THEN** install-catalog deploys the verified archive output without a clone

#### Scenario: Catalog verification before mutation
- **WHEN** `Manager::install_from_catalog` selects a Git release
- **THEN** the committed package folder, whole manifest and output file hashes are verified against the catalog before any workspace change
- **AND** verification is covered by `workspace::install_from_catalog_verifies_and_check_compares`

### Requirement: Refused input creates nothing
Input SHALL be validated before anything is created: the CLI checks argv syntax and parses IDs and versions with the library's parsers before opening, and `open_with` validates Options first (workspace-layout). A refused install SHALL leave no clone, no `dist/<id>`, no state entry and no `.pm-txn` (errors). Verified by `usage_errors_name_the_problem_and_create_nothing` and `workspace::install_refusals_leave_the_workspace_unchanged`.

#### Scenario: Invalid ID on a fresh root
- **WHEN** the user runs `gitpm --root new-ws update ../evil`
- **THEN** the command fails with `invalid_id` and `new-ws` is not created

### Requirement: Three removal levels
gitpm SHALL offer three removal levels: `disable` (set `enabled: false`, remove `dist/<id>`), `forget` (drop the package from `packages` in state, keep the clone and a `forgotten` record, remove `dist/<id>`) and `uninstall` (remove the state entry, the clone and `dist/<id>`). `restore ID` SHALL record a forgotten package again; all three are guarded as in dependencies ("Dependents guard removal"). Verified by `lifecycle_and_three_removal_levels` and `forget_says_whether_the_clone_was_kept`.

#### Scenario: Uninstall one package of a shared clone
- **WHEN** another package still uses the shared clone of the package being uninstalled
- **THEN** the clone stays; it is removed only with its last package (multi-package-repos, "One shared clone per source")

#### Scenario: Forget then restore
- **WHEN** the user runs `forget acme/greeter` then `restore acme/greeter`
- **THEN** the first prints `acme/greeter forgotten (clone kept; restore re-registers it)`, the second `acme/greeter restored 0.2.0`, and `dist/acme/greeter/` is back

#### Scenario: Restore without a clone
- **WHEN** the user restores an ID that has no `forgotten` record
- **THEN** the command fails with `repository_not_found`, as does a forgotten package whose clone (own or shared) is gone (multi-package-repos, "Forgotten packages of shared clones")

#### Scenario: Forget result
- **WHEN** `forget` succeeds
- **THEN** it returns `ForgetResult { clone_kept, dist }`; `clone_kept` is false when the clone was gone and no `forgotten` record was kept ("Removing a package whose clone is gone")

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
`uninstall_plan(id)` SHALL be read-only and list `deletes`, `registered`, `dependents`, `dependency_conflict` (a reactivated original's, dependencies) and `lost_work`; `UninstallPlan::refusal` SHALL return the error `uninstall` raises without force (`has_dependents` with `lost_work`, else `dependency_conflict`). Lost-work items: `Modified`, `Untracked`, `Ignored`, `HeadMoved`, `UnpublishedCommit`, `UnpublishedBranch`, `UnpublishedTag`, `Stash`. A clean clone needs no confirmation.

#### Scenario: Dependents without force
- **WHEN** the package has dependents and `force` is false
- **THEN** `uninstall` fails with `has_dependents` (dependencies), and the error carries both `dependents` and the plan's `lost_work`, so one refusal tells everything

#### Scenario: Stash alone blocks
- **WHEN** the clone is clean except for a `git stash` entry and uninstall is not confirmed
- **THEN** it fails with `local_changes` listing the stash and nothing is deleted
- **AND** this is covered by `uninstall::plan_lists_each_lost_work_type_including_stash_without_changes`

#### Scenario: Confirmed uninstall
- **WHEN** the plan has lost work and the caller passes exactly those items as `confirmed`
- **THEN** the clone, `dist/<id>` and the state entry are deleted

#### Scenario: Lost work grew after the plan
- **WHEN** a file is added to the clone after the plan was shown and the caller confirms only the plan's items
- **THEN** `uninstall` fails with `local_changes` listing the new item and nothing is deleted
- **AND** this is covered by `uninstall::confirmation_is_bound_to_the_planned_lost_work`

#### Scenario: Confirmed items and force
- **WHEN** a caller runs `uninstall(id, confirmed, force)` (FFI `uninstall(confirmed, force)`)
- **THEN** lost work not in `confirmed` refuses with `local_changes` listing it, and the plan's refusal applies unless `force`

### Requirement: CLI confirmation flow for uninstall
Without `--force`, the CLI SHALL report the plan's refusal when it has one, before asking about lost work, and otherwise list what `uninstall_plan` reports and ask `Delete anyway? [y/N]` on stderr on a TTY; anything but `y`/`yes` SHALL change nothing and fail with `uninstall cancelled; nothing was changed`. Verified by `uninstall_on_tty_asks_before_losing_work`, `uninstall_without_tty_refuses_and_lists_lost_work_including_stash` and `uninstall_refusal_of_a_reactivation_comes_before_the_prompt`.

#### Scenario: No TTY
- **WHEN** a clone has modified files and `gitpm uninstall ID` runs with stdin not a terminal
- **THEN** it prints `Error: …`, one `  - …` line per lost item and `Error kind: local_changes`, and the exit status is 1
- **AND** a declined prompt is a CLI error without `Error kind` (exit status 1), and `--force` passes the plan's items as `confirmed` and sets `force`

#### Scenario: Confirmation and force
- **WHEN** the user answers `y`, runs without a TTY, or passes `--force`
- **THEN** `y` passes the plan's items as `confirmed`, without a TTY nothing is confirmed (the library refuses), and `--force` confirms the plan's items without asking and ignores dependents

### Requirement: Only manager-owned files are deleted
Uninstall SHALL delete only what the workspace owns (the clone inside the workspace, `dist/<id>`, the state entry); a fork repository or author repository outside the workspace MUST NOT be touched.

#### Scenario: Uninstalling a fork
- **WHEN** `me/hello` was installed from `./hello-fork` and is uninstalled
- **THEN** `./hello-fork` still exists unchanged

### Requirement: Download without enabling
`download SOURCE [ID] [--package ID] [--ref REF | --dev]` (`Manager::download`) and `download-catalog ID [--version V]` (`Manager::download_from_catalog`) SHALL clone, verify and record a package exactly as `install`/`install-catalog` do, but with `enabled: false` and nothing deployed; missing dependencies SHALL be downloaded the same way, disabled. The CLI prints `downloaded ID@V from SOURCE (disabled)` per package. Verified by `workspace::download_records_disabled_packages_and_dependencies`.

#### Scenario: Download with a missing dependency
- **WHEN** `app` requires `lib ^1`, only the catalog has `lib`, and the user runs `gitpm download ./app`
- **THEN** both are recorded with `enabled: false`, neither `dist/app/` nor `dist/lib/` exists, and the output has one `downloaded …` line per package

#### Scenario: Enable after download
- **WHEN** the user then runs `enable app`
- **THEN** it fails with `dependency_conflict` naming `lib` as disabled (dependencies, "enable needs active dependencies"), while `enable lib` followed by `enable app` succeeds

### Requirement: Removing a package whose clone is gone
`uninstall ID` and `forget ID` of a recorded Git package whose own or shared clone is gone SHALL succeed without confirmation, drop the record and deployed folder, keep no forgotten record, and retain dependent guards (dependencies, "Dependents guard removal"). Builds intentionally have no clone and SHALL follow the cache lifecycle below. Verified by `workspace::uninstall_and_forget_of_a_package_without_clone_drop_the_record`.

#### Scenario: Clone deleted by hand
- **WHEN** `packages/acme/greeter` was deleted by hand while `acme/greeter` is recorded and active, and the user runs `uninstall acme/greeter`
- **THEN** it prints `acme/greeter uninstalled`, the record and `dist/acme/greeter/` are gone, and later commands warn no more

#### Scenario: Forget without a clone
- **WHEN** the same package is forgotten instead
- **THEN** it prints `acme/greeter forgotten (no clone; record and dist removed)` (`clone_kept` false), the record is dropped and no `forgotten` record is kept, because there is nothing to restore; a later `restore acme/greeter` fails with `repository_not_found`

#### Scenario: Forget and restore a build
- **WHEN** a build package is forgotten and restored
- **THEN** its cached output and record survive forget and restore redeploys them without Git

#### Scenario: Managed build output is not lost Git work
- **WHEN** a build is forgotten or uninstalled
- **THEN** managed cached output needs no lost-work confirmation; forget retains its record and cached output for restore, while uninstall removes the cache, subject to dependent guards
