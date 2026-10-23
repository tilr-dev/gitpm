## MODIFIED Requirements

### Requirement: Updates need no catalog
Unprotected Git installs SHALL update from their clone origin without a catalog. Build installs SHALL update from their recorded catalog, including unverified catalogs and without a catalog option. Targets with build SHALL install without Git; targets without build SHALL use Git. Explicit ref, branch, dev or detach SHALL transition a build to Git. New dependencies follow the dependency-resolution rules.

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


### Requirement: Protected packages update through their catalog
Protected packages SHALL update from their recorded verified catalog, regardless of the configured catalog. Git targets SHALL match source commit, path, manifest and hashes; build targets SHALL verify archive and files without Git. Unverified build provenance SHALL NOT imply protection. Detach SHALL transition to unprotected Git atomically; ref/branch/dev of a build SHALL also transition to Git. Git protection otherwise requires detach for refs.

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


### Requirement: Rollback swaps current and previous
Rollback SHALL swap current and previous revisions and modes offline and redeploy, refusing broken dependents. Build revisions SHALL restore locally retained output, metadata and provenance; Git revisions SHALL use their retained clone. Missing previous revisions, Git commits or cached builds SHALL fail with no_previous_version. Rollback SHALL print ID rolled back A → B.

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

