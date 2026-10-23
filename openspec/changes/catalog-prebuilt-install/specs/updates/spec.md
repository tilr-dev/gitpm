## MODIFIED Requirements

### Requirement: Updates need no catalog
`update ID` (any mode) and `update all` SHALL read releases and branches from the clone origin (set from `packages.<id>.source`) without catalog reads for unprotected Git packages. `check` SHALL use that origin when the catalog does not list them. Builds SHALL instead follow their recorded catalog, including unverified catalogs, without a catalog option. Verified by `workspace::update_and_rollback_swap_revisions_and_redeploy` (no catalog configured).

#### Scenario: New dependency without a catalog
- **WHEN** no catalog is configured and the newest release of a package adds a dependency that is not installed
- **THEN** `update ID` fails as specified in dependencies ("Install resolves dependencies"), while `update all` keeps that package back ("update all never removes or downgrades")

#### Scenario: Catalog-less update
- **WHEN** `acme/greeter` 0.1.0 was installed with `gitpm install ./greeter`, no `--catalog` or `GITPM_CATALOG` is set, and 0.2.0 is tagged in `./greeter`
- **THEN** `gitpm update acme/greeter` prints `acme/greeter updated 0.1.0 → 0.2.0 (major)`

#### Scenario: Catalog-less update all preview
- **WHEN** the same workspace has no catalog and the user runs `gitpm update all --dry-run`
- **THEN** it prints `acme/greeter would be updated 0.1.0 → 0.2.0 (major)` and the totals line ends with `Nothing was changed.`

#### Scenario: Catalog-less check
- **WHEN** the same workspace has no catalog and the user runs `gitpm check`
- **THEN** the row of `acme/greeter` says `update available` with `0.2.0 (major)` from the origin, and nothing in the workspace changes

#### Scenario: Unverified build follows its recorded catalog
- **WHEN** a build from an unverified catalog is updated without a catalog option
- **THEN** it reads its recorded catalog and installs the newer archive without cloning

#### Scenario: Select the target source kind
- **WHEN** update selects a release with a build, or without one
- **THEN** it installs the former without Git and the latter through Git; explicit ref, branch, dev or detach transitions a build to Git, and new dependencies follow the dependency-resolution rules

### Requirement: Protected packages update through their catalog
Protected packages SHALL update only to versions in their recorded verified catalog, whatever `Options.catalog` is. Git targets SHALL match recorded source, commit, path, manifest and hashes before any change (`catalog` otherwise); builds SHALL verify archive and files without Git. `update ID --detach` (`Manager::update_with`) SHALL drop protection and update from origin in one transaction. Verified by `workspace::protected_packages_update_only_to_catalog_versions`.

#### Scenario: Newer tag not in the catalog
- **WHEN** protected `acme/greeter` 0.2.0 has the tag `acme/greeter@0.3.0` on its origin but the verified catalog lists only 0.1.0 and 0.2.0
- **THEN** `update acme/greeter` prints `acme/greeter kept at 0.2.0 (upstream 0.3.0 not in catalog)` and `update all` lists it in `unchanged` with that reason
- **AND** a ref or branch target needs a detach: `update acme/greeter --ref acme/greeter@0.3.0` fails with `invalid_argument` ("… detach it to follow a ref or branch of its origin")

#### Scenario: Hash mismatch on update
- **WHEN** the verified catalog lists 0.3.0 but the files at the recorded commit do not match its hashes
- **THEN** `update acme/greeter` and `update all` fail with `catalog` ("catalog file hashes do not match git dist of acme/greeter@0.3.0: the release differs from the one added to the catalog") and the workspace is unchanged; a release commit the origin does not have fails with `catalog` ("… is not on the origin")

#### Scenario: Catalog no longer verified or unreachable
- **WHEN** the owner removed `"verified": true` from the catalog that protects `acme/greeter`, or the catalog cannot be read
- **THEN** `update acme/greeter` fails with `catalog` ("… is no longer verified; detach …") or `http` and nothing changes
- **AND** `update all` does not fail: it keeps `acme/greeter` in `unchanged` with the reason `catalog_unavailable` (`acme/greeter kept at 0.2.0 (catalog unavailable: REASON)`) and updates the other packages
- **AND** this is covered by `workspace::a_catalog_that_is_no_longer_verified_refuses_protected_updates`

#### Scenario: New dependency of a protected package
- **WHEN** protected `acme/app` 1.1.0 in its verified catalog adds a dependency `acme/lib` that is not installed, and no catalog or an unverified one is configured
- **THEN** `update acme/app` and `update all` install `acme/lib` from the recorded verified catalog and record it protected by that catalog
- **AND** this is covered by `workspace::new_dependencies_of_a_protected_package_come_from_its_catalog`

#### Scenario: Detach
- **WHEN** the user runs `update acme/greeter --detach`
- **THEN** it prints `acme/greeter detached from catalog LOCATION; it now follows its origin` and `acme/greeter updated 0.2.0 → 0.3.0 (major)`, the record loses `catalog` and later updates follow the origin
- **AND** a detach with no newer release is still a change (`workspace::detach_without_a_new_release_is_a_change`); detaching an unprotected package fails with `invalid_argument`; `update all --detach` is a usage error (exit status 1)

#### Scenario: Detach a protected build
- **WHEN** a protected build is updated with detach or an explicit Git ref or dev target
- **THEN** the transition stages a Git clone, drops protection and deploys Git output atomically

#### Scenario: Build provenance and protection
- **WHEN** an unverified catalog supplies a build
- **THEN** recorded build provenance does not imply protection; an explicit ref, branch or dev target transitions a build to Git atomically, while protected Git packages require detach for refs

### Requirement: Rollback swaps current and previous
`rollback ID` SHALL swap `current` and `previous` revisions and modes offline (no fetch) and redeploy retained Git or build output. Missing previous revisions, Git commits or cached builds SHALL fail with `no_previous_version`. It SHALL print `ID rolled back A → B` and refuse a target that breaks an installed dependent with `dependency_conflict` (dependencies, "Updates keep dependents working"). Verified by `rollback_to_a_pruned_commit_names_it`.

#### Scenario: Previous revision needs a missing dependency
- **WHEN** the previous revision depends on a package that is not installed
- **THEN** rollback fails with `dependency_conflict`, because it never installs dependencies

#### Scenario: Rollback after update
- **WHEN** a package was updated 0.1.0 → 0.2.0 and the user runs `rollback`
- **THEN** it prints `acme/greeter rolled back 0.2.0 → 0.1.0` and `previous` becomes 0.2.0

#### Scenario: Previous commit gone
- **WHEN** the previous revision's commit is no longer in the clone (pruned after the origin deleted its tag)
- **THEN** rollback fails with `no_previous_version` ("previous revision COMMIT of ID is no longer available") and the workspace is unchanged

#### Scenario: Offline build rollback
- **WHEN** a build was updated and its catalog and artifact server are unavailable
- **THEN** rollback restores the retained previous output and metadata and swaps the two revisions

#### Scenario: Restore retained build state
- **WHEN** rollback selects a retained build revision
- **THEN** its output, release metadata and catalog provenance are restored locally, and current and previous modes and source kinds are swapped
