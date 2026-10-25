## MODIFIED Requirements

### Requirement: Records carry the recorded mode
FFI records SHALL carry every field the recorded modes add to library results, as the `Mode` and `UnchangedReason` enums. This adds to "Records mirror library results". Verified by `records_convert_field_by_field`.

#### Scenario: Mode fields
- **WHEN** Swift bindings are generated
- **THEN** `InstalledPackage`, `PackageRecord`, `CheckRow`, `RevisionChange` and `InstallResult` have `mode` (`release`, `pinned(reference)`, `branch(name)`), `Revision.mode` is the mode of a `previous` revision, `InstallResult` has `requestedMode`, and `RevisionChange.unchangedReason` and `Unchanged.reason` are `pinned(reference)`, `diverged(branch)`, `branchGone(branch)` or (`updateAll` only) `brokenDependency(dependency, problem)`

#### Scenario: Pinned package from Swift
- **WHEN** a host lists a workspace where `acme/greeter` was installed with `--ref v0.1.0` and then calls `update(id, .recorded)`
- **THEN** the listed `InstalledPackage.mode` is pinned to `v0.1.0`, and the returned `RevisionChange` is unchanged with `unchangedReason` `pinned`
