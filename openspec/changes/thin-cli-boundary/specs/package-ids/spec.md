## MODIFIED Requirements

### Requirement: No default namespace
gitpm SHALL NOT derive or default a namespace: every operation that creates a package (`init`) MUST receive a full `NAMESPACE/NAME`, and `fork` MUST receive the target namespace explicitly. The library SHALL refuse a name without a namespace with `invalid_id`. There SHALL be no namespace configuration file or command.

#### Scenario: init without a namespace
- **WHEN** the user runs `gitpm init hello --into hello`
- **THEN** the library fails with `invalid_id` ("package ID needs a namespace: use NAMESPACE/NAME, for example acme/hello"), the CLI prints `Error kind: invalid_id`, the exit status is 1 and nothing is created

### Requirement: Broken historical tags are skipped
Single source for broken release tags: a broken historical release tag (an invalid manifest at its commit, a tag that disagrees with the manifest, a tag whose commit lacks its package) SHALL be skipped and reported, not fail discovery; install, `update ID` and `update all` SHALL return the skipped tags in their results (`InstallResult.skipped`, `RevisionChange.skipped`, and for `update all` the plan's `UpdateAllPlan.skipped`, covering every installed package). Selecting the latest release (install, update, `catalog-update`) SHALL still fail with `version_conflict` ("latest release tag is broken: …") when a skipped tag is newer than every valid release. Verified by `releases::bad_historical_tag_does_not_hide_valid_releases`.

#### Scenario: Bad newest tag
- **WHEN** the newest tag is broken and an older one is valid
- **THEN** install, update and `catalog-update` fail with `version_conflict` ("latest release tag is broken: …")

#### Scenario: One bad old tag
- **WHEN** tag 0.1.0's manifest is invalid but 0.2.0 is valid
- **THEN** `catalog-update` publishes 0.2.0 and warns about 0.1.0
- **AND** `catalog-update` warns `Warning: SOURCE: reason`; this is covered by `catalog::update_sources_skips_and_reports_a_broken_historical_tag`

#### Scenario: Update past a broken old tag
- **WHEN** `acme/greeter` 0.1.0 is installed, the origin adds a tag `acme/greeter@0.1.5` whose manifest is invalid and a valid `acme/greeter@0.2.0`, and the user runs `update acme/greeter` or `update all`
- **THEN** the package moves to 0.2.0, the result's `skipped` names `acme/greeter@0.1.5` with its reason, and the CLI prints `Warning: skipped release tag acme/greeter@0.1.5: reason`, as install does
