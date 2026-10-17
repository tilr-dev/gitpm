# install-uninstall Specification

## Purpose

Defines how packages enter and leave a workspace: installing from a Git source or the catalog, idempotent re-install, the three removal levels (disable, forget, uninstall), restore, and the plan-and-confirm protocol that protects local work.

## Requirements

### Requirement: Install the latest release of a Git source
`install SOURCE` (`Manager::install`) SHALL clone the Git source (path or URL), select the numerically newest valid release tag (see package-ids), check it out detached, verify the manifest and deploy its `dist/`; missing dependencies are installed as in dependencies. Untagged commits SHALL be ignored; a source without release tags SHALL fail with `no_releases`. Verified by `stable_install_and_update_ignore_untagged_head_and_sort_versions`.

#### Scenario: Newest numeric tag wins
- **WHEN** a repository has tags `acme/greeter@0.9.0` and `acme/greeter@0.10.0` and a newer untagged commit
- **THEN** install checks out `0.10.0` and prints `installed acme/greeter@0.10.0 from SOURCE`

#### Scenario: Repository without releases
- **WHEN** the source has no release tags
- **THEN** install fails with `no_releases` and the workspace is unchanged

### Requirement: Install an explicit ref or the dev channel
`install SOURCE --ref REF` SHALL install the given tag, branch or commit; `--channel dev` SHALL follow the default branch HEAD and `--channel stable` SHALL mean the default. `--ref` and `--channel` MUST NOT be combined (`invalid_argument`; the CLI refuses earlier). A ref starting with `-` SHALL be refused with `invalid_argument` ("invalid git ref"). Verified by `workspace::install_channels_and_explicit_refs` and `model::resolve_ref_refuses_option_like_refs_and_resolves_commits`.

#### Scenario: Ref and channel in the CLI
- **WHEN** the user runs `gitpm install SOURCE --ref x --channel dev`
- **THEN** it fails with `--channel conflicts with --ref` (no `Error kind`) before the workspace is opened

#### Scenario: Install by exact commit
- **WHEN** the user runs `install SOURCE --ref <40-hex commit>`
- **THEN** that commit is installed and recorded as `current.commit`

#### Scenario: Option-like ref
- **WHEN** the ref is `--upload-pack=evil`
- **THEN** the command fails with `invalid_argument`, no git process receives the ref and the workspace is unchanged

### Requirement: Install is idempotent for the same source
Installing an installed package again from the same source SHALL change nothing and succeed, reporting `ID already installed (VERSION)`, with `; run update` when the source resolves to another revision (with `--ref`/`--channel dev`: `; requested --ref REF — run update ID --ref REF`). "Same source" SHALL mean the same string as recorded after canonicalizing a local path. `InstallResult` SHALL carry `already_installed` and `available`.

#### Scenario: Repeat install
- **WHEN** `acme/greeter` 0.2.0 is installed from `./greeter` and the user runs `gitpm install ./greeter`
- **THEN** it prints `acme/greeter already installed (0.2.0)`, exits 0 and the workspace is byte-identical
- **AND** this is covered by `install_again_reports_already_installed_and_changes_nothing`

#### Scenario: Newer release available
- **WHEN** the source has a newer release than the installed one
- **THEN** it prints `acme/greeter already installed (0.2.0); run update` and changes nothing

### Requirement: Same ID from another source is refused
Installing an ID that is installed from a different source (including a `file://` URL against a plain path, or a URL with and without `.git`) SHALL fail with `already_installed`. Installing an ID that is forgotten but retained SHALL be refused until it is restored or uninstalled. Verified by `forgotten_repository_blocks_reinstall_until_restored`.

#### Scenario: Other source
- **WHEN** `acme/greeter` is installed from `./greeter` and the user installs `file:///…/greeter`
- **THEN** the command fails with `already_installed`

### Requirement: Install from the catalog
`install-catalog ID [--version V]` (`Manager::install_from_catalog`) SHALL install the exact catalog release (latest if omitted): clone its source, check out the recorded commit, and verify that the manifest equals the catalog's manifest and the `dist/` file hashes equal the catalog's before any workspace change. A mismatch SHALL fail with `catalog`. Verified by `catalog_manifest_is_verified_before_install` and `workspace::install_from_catalog_verifies_and_check_compares`.

#### Scenario: Second catalog
- **WHEN** plugins from catalog A are installed and the user runs `install-catalog` with catalog B
- **THEN** it is refused as specified in catalog ("One catalog per workspace")

#### Scenario: Tampered dist
- **WHEN** the catalog's file hashes do not match the release commit's `dist/`
- **THEN** install fails with `catalog` ("catalog file hashes do not match git dist") and nothing is installed

### Requirement: Refused input creates nothing
CLI arguments (IDs, versions, option combinations, unknown options, missing values) SHALL be validated before the workspace is opened or created, and a refused install SHALL leave no clone, no `dist/<id>`, no state entry and no `.pm-txn` (general rule in errors, "Failing operations leave the workspace unchanged"). Verified by `usage_errors_name_the_problem_and_create_nothing` and `workspace::install_refusals_leave_the_workspace_unchanged`.

#### Scenario: Invalid ID on a fresh root
- **WHEN** the user runs `gitpm --root new-ws update ../evil`
- **THEN** the command fails with `invalid_id` and `new-ws` is not created

### Requirement: Three removal levels
gitpm SHALL offer three removal levels: `disable` (keep everything, flag `enabled: false`, remove `dist/<id>`), `forget` (unregister, keep the clone, remove `dist/<id>`) and `uninstall` (remove state entry, clone and `dist/<id>`). `restore ID` SHALL re-register a retained clone at its recorded or checked-out revision. Verified by `lifecycle_and_three_removal_levels`.

#### Scenario: Package with dependents
- **WHEN** another package depends on the one being disabled, forgotten or uninstalled
- **THEN** the `has_dependents` guard of dependencies ("Dependents guard removal") applies

#### Scenario: Forget then restore
- **WHEN** the user runs `forget acme/greeter` then `restore acme/greeter`
- **THEN** the first prints `acme/greeter forgotten (clone kept; restore re-registers it)`, the second `acme/greeter restored 0.2.0`, and `dist/acme/greeter/` is back

#### Scenario: Restore without a clone
- **WHEN** the user restores an ID whose clone does not exist
- **THEN** the command fails with `repository_not_found`

### Requirement: Restore preconditions
`restore ID` of an own clone (`plugins/<id>`) SHALL refuse with `already_installed` ("plugin ID is already registered") when the ID is still registered, with `local_changes` when the clone has modified, untracked or ignored files, and with `invalid_manifest` ("plugin id mismatch") when the manifest at HEAD has another ID. It SHALL register HEAD with no `previous` and print `ID restored V`. Shared-clone restore: see multi-package-repos.

#### Scenario: Dirty retained clone
- **WHEN** `acme/greeter` was forgotten, a file in `plugins/acme/greeter` was edited and the user runs `restore acme/greeter`
- **THEN** the command fails with `local_changes` and the package stays unregistered

### Requirement: Uninstall is planned and confirmed
`uninstall_plan(id)` SHALL be read-only and list `deletes` (paths), `registered`, `dependents` and `lost_work` (`Modified`, `Untracked`, `Ignored`, `HeadMoved`, `UnpublishedCommit`, `UnpublishedBranch`, `UnpublishedTag`, `Stash`). `uninstall(id, false)` SHALL refuse with `local_changes` (listing the items) when anything would be lost; `uninstall(id, true)` SHALL delete anyway. A clean clone SHALL be removed without confirmation.

#### Scenario: Stash alone blocks
- **WHEN** the clone is clean except for a `git stash` entry and uninstall is not confirmed
- **THEN** it fails with `local_changes` listing the stash and nothing is deleted
- **AND** this is covered by `uninstall::plan_lists_each_lost_work_type_including_stash_without_changes`

#### Scenario: Confirmed uninstall
- **WHEN** the plan has lost work and the caller confirms
- **THEN** the clone, `dist/<id>` and the state entry are deleted

### Requirement: CLI confirmation flow for uninstall
Without `--force`, the CLI SHALL ask `Delete anyway? [y/N]` on a TTY after listing the deleted paths, the state entry and the lost work; anything but `y`/`yes` (including EOF) SHALL change nothing and exit non-zero. Without a TTY it SHALL refuse and list the lost work. `--force` SHALL delete without asking and ignore dependents. Verified by `uninstall_on_tty_asks_before_losing_work` and `uninstall_without_tty_refuses_and_lists_lost_work_including_stash`.

#### Scenario: No TTY
- **WHEN** a clone has modified files and `gitpm uninstall ID` runs with stdin not a terminal
- **THEN** it prints `Error: …`, one `  - …` line per lost item and `Error kind: local_changes`, and exits 1

### Requirement: Only manager-owned files are deleted
Uninstall SHALL delete only what the workspace owns (the clone inside the workspace, `dist/<id>`, the state entry); a fork repository or author repository outside the workspace MUST NOT be touched.

#### Scenario: Uninstalling a fork
- **WHEN** `me/hello` was installed from `./hello-fork` and is uninstalled
- **THEN** `./hello-fork` still exists unchanged
