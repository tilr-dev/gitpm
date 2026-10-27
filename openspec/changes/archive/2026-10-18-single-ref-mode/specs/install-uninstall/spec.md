## RENAMED Requirements

- FROM: `### Requirement: Install an explicit ref or the dev channel`
- TO: `### Requirement: Install an explicit ref or a branch`

## MODIFIED Requirements

### Requirement: Install an explicit ref or a branch
`install SOURCE --ref REF` SHALL resolve REF against the origin (tag, origin branch, commit) and record the mode (workspace-layout): a tag or commit pins the package, a branch makes it follow that branch; `--dev` SHALL follow the origin's default branch. Without either, the newest release is installed in release mode. A ref starting with `-` is refused as in git-safety. Verified by `workspace::install_records_the_ref_mode` and `model::resolve_ref_refuses_option_like_refs_and_resolves_commits`.

#### Scenario: Ref and dev channel in the library
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
Installing an installed package again from the same source SHALL change nothing and succeed, reporting `ID already installed (VERSION)` (with the recorded mode: `(VERSION, pinned at REF)`, `(VERSION, following branch B)`), with `; run update` when the source resolves to another revision or `--ref`/`--dev` asks for another mode (then `; requested --ref REF — run update ID --ref REF`). "Same source" SHALL mean the same string as recorded after canonicalizing a local path. `InstallResult` SHALL carry `already_installed`, `available`, the recorded `mode` and `requested_mode`.

#### Scenario: Repeat install
- **WHEN** `acme/greeter` 0.2.0 is installed from `./greeter` and the user runs `gitpm install ./greeter`
- **THEN** it prints `acme/greeter already installed (0.2.0)`, exits 0 and the workspace is byte-identical
- **AND** this is covered by `install_again_reports_already_installed_and_changes_nothing`

#### Scenario: Newer release available
- **WHEN** the source has a newer release than the installed one
- **THEN** it prints `acme/greeter already installed (0.2.0); run update` and changes nothing

### Requirement: Restore preconditions
`restore ID` SHALL record the package again from its `forgotten` record (own or shared clone), keeping revisions, `catalog`, `enabled` and mode; release and pinned mode check out the recorded commit, branch mode takes HEAD as found when it descends from the recorded commit and the origin's branch contains it (then the restored revision passes the dependency check, `dependency_conflict` otherwise; a shared clone at another package's revision is checked out at the recorded commit). It SHALL refuse with `local_changes` on lost work (unpublished commits included) and with `already_installed` when the ID is recorded. It SHALL print `ID restored V`. Verified by `workspace::restore_keeps_the_forgotten_record`, `restore_in_a_shared_clone_ignores_another_branch_s_revision` and `branch_mode_restore_checks_dependencies`.

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
