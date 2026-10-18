## MODIFIED Requirements

### Requirement: Records mirror library results
FFI records SHALL convert library results field by field and SHALL drop no field, so a host sees everything the CLI prints (scenario "Record fields" lists the records). Verified by `records_convert_field_by_field`.

#### Scenario: Partial fork failure
- **WHEN** `forkAndReplace` fails at the install step
- **THEN** the result has `failedStep` `install` with the error kind and message, not a thrown error
- **AND** this is covered by `records_convert_field_by_field` and `fork_replace_result_reports_the_failed_step_with_kind_and_message`

#### Scenario: Warnings reach the host
- **WHEN** an install skips a broken historical tag and ignores a wrong-typed metadata key
- **THEN** the FFI `InstallResult` carries both in `skipped` and `warnings`, as the CLI prints them

#### Scenario: Record fields
- **WHEN** Swift bindings are generated
- **THEN** they include `InstalledPlugin` (`active`, `clone`, `path`, `mode`, metadata), `InstallResult` (`alreadyInstalled`, `available`, `skipped`, `warnings`), `RevisionChange` (`warnings`, `unchangedReason`, `major`), the dist report of every mutating result, `CatalogRelease`, `PluginSummary`, `DistSync`, `UpdateAllPlan` (`unchanged` with reason, `keptBack` with `reason`), `ReplacementPlan` (`installedVersion`, `forkEnabled`), `CheckReport` (`catalogVerified`, rows with `source`, `major`, `prerelease`, upstream, replacements, unsatisfied dependencies) and fork results (`ForkStep`, `errorKind`, `errorMessage`, pending plan)

#### Scenario: check without a catalog from Swift
- **WHEN** a host calls `check(root, nil)` on a workspace with packages installed from Git
- **THEN** it receives a `CheckReport` whose rows have `source` `origin`, plus the replacement bindings and unsatisfied dependencies the CLI prints

