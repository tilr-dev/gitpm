## MODIFIED Requirements

### Requirement: Install the latest release of a Git source
`install SOURCE` (`Manager::install`) SHALL clone the Git source (path or URL), select the newest valid release tag that is not a pre-release (package-ids), check it out detached, verify the manifest and deploy its package dist; missing dependencies are installed as in dependencies. Untagged commits SHALL be ignored; a source without release tags, or with pre-release tags only, SHALL fail with `no_releases`. Verified by `stable_install_and_update_ignore_untagged_head_and_sort_versions`.

#### Scenario: Newest numeric tag wins
- **WHEN** a repository has tags `v0.9.0` and `v0.10.0` and a newer untagged commit
- **THEN** install checks out `0.10.0` and prints `installed d135a7cf-1c63-549a-93cb-8f89656b2ced@0.10.0 from SOURCE`

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
- **WHEN** a repository has `v1.2.0` and the newer `v1.3.0-beta.1`
- **THEN** `install` installs 1.2.0; `install SOURCE --ref v1.3.0-beta.1` installs the beta (pinned, as every `--ref` tag)

### Requirement: Install is idempotent for the same source
Installing an installed and enabled package again from the same source SHALL change nothing and succeed, reporting `ID already installed (VERSION)` (with a mode: `(VERSION, pinned at REF)`), with `; run update` when the source resolves to another revision or mode (`; requested --ref REF — run update ID --ref REF`). `InstallResult` SHALL carry `already_installed`, `available`, the recorded `mode`, `requested_mode` and `enabled`.

#### Scenario: Repeat install
- **WHEN** `d135a7cf-1c63-549a-93cb-8f89656b2ced` 0.2.0 is installed from `./greeter` and the user runs `gitpm install ./greeter`
- **THEN** it prints `d135a7cf-1c63-549a-93cb-8f89656b2ced already installed (0.2.0)`, exits 0 and the workspace is byte-identical
- **AND** this is covered by `install_again_reports_already_installed_and_changes_nothing`

#### Scenario: Newer release available
- **WHEN** the source has a newer release than the installed one
- **THEN** it prints `d135a7cf-1c63-549a-93cb-8f89656b2ced already installed (0.2.0); run update` and changes nothing

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
- **WHEN** `d135a7cf-1c63-549a-93cb-8f89656b2ced` is installed from `./greeter` and the user installs `file:///…/greeter`
- **THEN** the command fails with `already_installed`

#### Scenario: Forgotten but retained
- **WHEN** `d135a7cf-1c63-549a-93cb-8f89656b2ced` was forgotten and its clone `repos/<key>` is retained, and the user installs it again
- **THEN** the command fails with `already_installed` ("d135a7cf-1c63-549a-93cb-8f89656b2ced was forgotten and its clone is kept") and the workspace is unchanged
- **AND** the message names no command; the CLI adds a `Hint:` line naming `gitpm restore ID` and `gitpm uninstall ID` (the same holds for a forgotten package of a shared clone)

### Requirement: Install from the catalog
`install-catalog ID [--version V]` (`Manager::install_from_catalog`) SHALL install the exact release (latest if omitted), verifying the manifest and file hashes before any change. Git installs SHALL check out the recorded source commit; builds SHALL verify archive SHA-256 and deploy without cloning. Mismatches SHALL fail with `catalog`, unavailable artifacts with `http`. Only verified catalogs protect. Verified by `catalog_manifest_is_verified_before_install`.

#### Scenario: Tampered dist
- **WHEN** the catalog's file hashes do not match the release commit's `dist/`
- **THEN** install fails with `catalog` ("catalog file hashes do not match git dist") and nothing is installed
- **AND** installing from a second catalog is refused as specified in catalog ("One catalog per workspace")

#### Scenario: Catalog install over a direct install
- **WHEN** `d135a7cf-1c63-549a-93cb-8f89656b2ced` 0.1.0 was installed or downloaded with `install`/`download ./greeter` and the user runs `install-catalog d135a7cf-1c63-549a-93cb-8f89656b2ced` from a verified catalog
- **THEN** the installed revision must be the catalog's record of 0.1.0 (same source and commit, package folder, manifest and file hashes): the record gets `catalog` (a download is also enabled) and the result warns `d135a7cf-1c63-549a-93cb-8f89656b2ced is now protected by catalog LOCATION`
- **AND** another commit or other files fail with `catalog` ("… it cannot be protected by that catalog") and nothing changes; this is covered by `workspace::install_from_a_verified_catalog_protects_a_direct_install_of_its_release`

#### Scenario: Install from an unverified catalog
- **WHEN** the catalog has no `"verified": true` and the user runs `install-catalog d135a7cf-1c63-549a-93cb-8f89656b2ced`
- **THEN** the release is verified as above, the record has no `catalog`, and a Git installation later updates from the origin; a build installation follows its separately recorded catalog location

#### Scenario: Install a build with unavailable source
- **WHEN** a catalog release has a valid build and its source repository is unavailable
- **THEN** install-catalog deploys the verified archive output without a clone

#### Scenario: Catalog verification before mutation
- **WHEN** `Manager::install_from_catalog` selects a Git release
- **THEN** the committed package folder, whole manifest and output file hashes are verified against the catalog before any workspace change
- **AND** verification is covered by `workspace::install_from_catalog_verifies_and_check_compares`

### Requirement: Three removal levels
gitpm SHALL offer three removal levels: `disable` (set `enabled: false`, remove `dist/<id>`), `forget` (drop the package from `packages` in state, keep the clone and a `forgotten` record, remove `dist/<id>`) and `uninstall` (remove the state entry, the clone and `dist/<id>`). `restore ID` SHALL record a forgotten package again; all three are guarded as in dependencies ("Dependents guard removal"). Verified by `lifecycle_and_three_removal_levels` and `forget_says_whether_the_clone_was_kept`.

#### Scenario: Uninstall one package of a shared clone
- **WHEN** another package still uses the shared clone of the package being uninstalled
- **THEN** the clone stays; it is removed only with its last package (multi-package-repos, "One shared clone per source")

#### Scenario: Forget then restore
- **WHEN** the user runs `forget d135a7cf-1c63-549a-93cb-8f89656b2ced` then `restore d135a7cf-1c63-549a-93cb-8f89656b2ced`
- **THEN** the first prints `d135a7cf-1c63-549a-93cb-8f89656b2ced forgotten (clone kept; restore re-registers it)`, the second `d135a7cf-1c63-549a-93cb-8f89656b2ced restored 0.2.0`, and `dist/d135a7cf-1c63-549a-93cb-8f89656b2ced/` is back

#### Scenario: Restore without a clone
- **WHEN** the user restores an ID that has no `forgotten` record
- **THEN** the command fails with `repository_not_found`, as does a forgotten package whose clone (own or shared) is gone (multi-package-repos, "Forgotten packages of shared clones")

#### Scenario: Forget result
- **WHEN** `forget` succeeds
- **THEN** it returns `ForgetResult { clone_kept, dist }`; `clone_kept` is false when the clone was gone and no `forgotten` record was kept ("Removing a package whose clone is gone")

### Requirement: Restore preconditions
`restore ID` SHALL record the package again from its `forgotten` record (Git or build) as it was; branch mode takes HEAD when it moved on along the origin's branch (dependencies checked). It SHALL refuse with `local_changes` on lost work and with `already_installed` when the ID is recorded, and print `ID restored V`. Verified by `workspace::restore_keeps_the_forgotten_record`, `restore_in_a_shared_clone_ignores_another_branch_s_revision` and `branch_mode_restore_checks_dependencies`.

#### Scenario: Restored record
- **WHEN** a clean retained clone of `d135a7cf-1c63-549a-93cb-8f89656b2ced`, forgotten while disabled and bound to a catalog, is restored
- **THEN** state records the forgotten revisions, `catalog`, `enabled: false` and mode unchanged, and nothing is deployed because it is disabled

#### Scenario: Dirty retained clone
- **WHEN** `d135a7cf-1c63-549a-93cb-8f89656b2ced` was forgotten, a file in `repos/<key>` was edited and the user runs `restore d135a7cf-1c63-549a-93cb-8f89656b2ced`
- **THEN** the command fails with `local_changes` and the package stays out of state

#### Scenario: Unpublished commit in a retained clone
- **WHEN** a commit was made in the retained clone that no remote branch contains, and the user restores it
- **THEN** restore fails with `local_changes` listing `UnpublishedCommit`, also in branch mode

#### Scenario: Branch mode restore
- **WHEN** a package in branch mode `main` was forgotten and its clone's HEAD is a published commit newer than the recorded one
- **THEN** `restore` records that HEAD as `current` and keeps branch mode `main`

### Requirement: Only manager-owned files are deleted
Uninstall SHALL delete only what the workspace owns (the clone inside the workspace, `dist/<id>`, the state entry); a fork repository or author repository outside the workspace MUST NOT be touched.

#### Scenario: Uninstalling a fork
- **WHEN** `ce6c8a3b-b2f8-5391-8ef8-4cbb7873f5d4` was installed from `./hello-fork` and is uninstalled
- **THEN** `./hello-fork` still exists unchanged

### Requirement: Removing a package whose clone is gone
`uninstall ID` and `forget ID` of a recorded Git package whose clone is gone SHALL succeed without confirmation, drop the record and deployed folder, keep no forgotten record, and retain dependent guards (dependencies, "Dependents guard removal"). Builds intentionally have no clone and SHALL follow the cache lifecycle below. Verified by `workspace::uninstall_and_forget_of_a_package_without_clone_drop_the_record`.

#### Scenario: Clone deleted by hand
- **WHEN** `repos/<key>` was deleted by hand while `d135a7cf-1c63-549a-93cb-8f89656b2ced` is recorded and active, and the user runs `uninstall d135a7cf-1c63-549a-93cb-8f89656b2ced`
- **THEN** it prints `d135a7cf-1c63-549a-93cb-8f89656b2ced uninstalled`, the record and `dist/d135a7cf-1c63-549a-93cb-8f89656b2ced/` are gone, and later commands warn no more

#### Scenario: Forget without a clone
- **WHEN** the same package is forgotten instead
- **THEN** it prints `d135a7cf-1c63-549a-93cb-8f89656b2ced forgotten (no clone; record and dist removed)` (`clone_kept` false), the record is dropped and no `forgotten` record is kept, because there is nothing to restore; a later `restore d135a7cf-1c63-549a-93cb-8f89656b2ced` fails with `repository_not_found`

#### Scenario: Forget and restore a build
- **WHEN** a build package is forgotten and restored
- **THEN** its cached output and record survive forget and restore redeploys them without Git

#### Scenario: Managed build output is not lost Git work
- **WHEN** a build is forgotten or uninstalled
- **THEN** managed cached output needs no lost-work confirmation; forget retains its record and cached output for restore, while uninstall removes the cache, subject to dependent guards

### Requirement: Explicit local linking is distinct from branch following
gitpm SHALL provide a library link operation and equivalent FFI and CLI entry points for a local author package. It SHALL validate identity and resolve the installation folder before publishing a symlink or changing state, using the same `plugin.json.dist` selection as managed installation (`"dist"` when omitted, `"."` for the package root). It MUST NOT redefine `--dev`, which continues to follow the origin default branch.

#### Scenario: Link a package folder
- **WHEN** a valid local package folder is linked
- **THEN** gitpm records the package in link mode and publishes its installation folder through a symlink, without creating a managed clone or executing package code

#### Scenario: Multi-package repository
- **WHEN** the selected local package belongs to a multi-package repository
- **THEN** the link identifies that package and resolves its installation folder relative to its package folder

#### Scenario: Explicitly link a prepared fork
- **WHEN** the author prepares a repository with `gitpm fork 7e6e8392-7be8-55ae-bd03-2b430b1c5d59 --into hello-fork` and then explicitly links its `ce6c8a3b-b2f8-5391-8ef8-4cbb7873f5d4` package
- **THEN** only that selected package is installed in link mode, without requiring a new release or changing the original's installation or replacement bindings
- **AND** fork preparation alone neither installs the fork nor publishes a linked deployment; sibling packages remain uninstalled

#### Scenario: Explicitly replace with a linked fork
- **WHEN** an installed, enabled linked fork declares a validated `replaces.id` matching the original and the user runs `replace ORIGINAL --with FORK`
- **THEN** the existing explicit replacement operation reads the link record's top-level `replaces` offer and applies the existing replacement and dependency rules
- **AND** consumers of the original use the linked fork's live deployment; linking alone never creates that binding
