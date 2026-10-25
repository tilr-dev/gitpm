## MODIFIED Requirements

### Requirement: Install the latest release of a Git source
`install SOURCE` (`Manager::install`) SHALL bare-clone the source, select its newest valid stable release tag (package-ids), create a detached package worktree at that commit, verify the manifest and deploy its output, installing missing dependencies (dependencies). Untagged commits are ignored; no stable releases SHALL fail with `no_releases`. Verified by `stable_install_and_update_ignore_untagged_head_and_sort_versions`.

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

#### Scenario: Bare source storage
- **WHEN** a root or nested Git package is installed
- **THEN** `repos/<key>` is a bare repository with no working tree, and `worktrees/<id>` is its detached package worktree


### Requirement: Three removal levels
gitpm SHALL offer `disable` (disable and undeploy), `forget` (undeploy, move the full record to `forgotten`, retain Git storage or cached build), and `uninstall` (remove record, worktree, output and unused bare storage). `restore` SHALL re-register forgotten packages. All levels use dependent guards (dependencies). Verified by `lifecycle_and_three_removal_levels` and `forget_says_whether_the_clone_was_kept`.

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
`restore ID` SHALL record the package again from its forgotten Git or build record as it was, checking dependencies. A Git worktree SHALL be clean and at its recorded commit, also in branch mode; restore SHALL NOT adopt a different HEAD. Restore SHALL refuse with `local_changes` on lost work, `repository_not_found` when required Git storage is missing, and `already_installed` when the ID is recorded, and print `ID restored V`.

#### Scenario: Restored record
- **WHEN** a clean package worktree at its recorded commit is restored
- **THEN** the forgotten revisions, catalog binding, enabled flag and mode are preserved, and deployment follows the enabled flag

#### Scenario: Dirty retained clone
- **WHEN** a forgotten package worktree was edited, including outside its nested package folder
- **THEN** restore fails with `local_changes` and keeps the forgotten record

#### Scenario: Unpublished commit in a retained clone
- **WHEN** a forgotten package worktree HEAD is an unpublished commit different from its recorded current commit
- **THEN** restore fails with `local_changes` listing `HeadMoved`, also in branch mode, and preserves the forgotten record

#### Scenario: Branch mode restore
- **WHEN** a forgotten branch-mode package worktree HEAD differs from its recorded commit
- **THEN** restore fails with `local_changes` listing `HeadMoved`, including when the new commit is published


### Requirement: Removing a package whose clone is gone
`uninstall ID` and `forget ID` of a recorded Git package whose bare repository or package worktree is gone SHALL succeed without confirmation, drop the record and deployed folder, keep no forgotten record, and retain dependent guards (dependencies, "Dependents guard removal"). Builds intentionally have no clone and SHALL follow the cache lifecycle below. Verified by `workspace::uninstall_and_forget_of_a_package_without_clone_drop_the_record`.

#### Scenario: Clone deleted by hand
- **WHEN** `repos/<key>` or its package worktree was deleted by hand while `acme/greeter` is recorded and active, and the user runs `uninstall acme/greeter`
- **THEN** it prints `acme/greeter uninstalled`, the record and `dist/acme/greeter/` are gone, and later commands warn no more

#### Scenario: Orphan worktree survives missing source storage
- **WHEN** a package bare repository was deleted by hand while its worktree still contains local files, and the package is uninstalled or forgotten
- **THEN** the record and deployed folder are removed while the orphan worktree and its files remain untouched

#### Scenario: Forget without a clone
- **WHEN** the same package is forgotten instead
- **THEN** it prints `acme/greeter forgotten (no clone; record and dist removed)` (`clone_kept` false), the record is dropped and no `forgotten` record is kept, because there is nothing to restore; a later `restore acme/greeter` fails with `repository_not_found`

#### Scenario: Forget and restore a build
- **WHEN** a build package is forgotten and restored
- **THEN** its cached output and record survive forget and restore redeploys them without Git

#### Scenario: Managed build output is not lost Git work
- **WHEN** a build is forgotten or uninstalled
- **THEN** managed cached output needs no lost-work confirmation; forget retains its record and cached output for restore, while uninstall removes the cache, subject to dependent guards
