## ADDED Requirements

### Requirement: Kept-back reasons in records
Each `keptBack` entry of the FFI `UpdateAllPlan` SHALL carry the library's `HeldBack.reason` as the `HeldBackReason` enum (`constraints`, `missingDependency(dependency, requirement)`, `noLongerReplaces(original)`) plus its `message`, and each `unchanged` entry the `message` of its `reason`, so a host explains a kept-back or skipped package as `gitpm update all` does. This adds to "Records mirror library results". Verified by `records_convert_field_by_field`.

#### Scenario: Broken consumer from Swift
- **WHEN** `lib` was force-uninstalled while `app` depends on it and a host calls `updateAllPlan`
- **THEN** the plan's `unchanged` has `app` with `reason` `brokenDependency(dependency: "acme/lib", problem: "not installed")` and `message` `dependency acme/lib is not installed`

#### Scenario: Fork kept back from Swift
- **WHEN** the newest release of a replacing fork no longer declares `replaces` and a host calls `updateAllPlan`
- **THEN** the fork's `keptBack` entry has `reason` `noLongerReplaces(original)` and `message` `no longer replaces ORIGINAL`

## MODIFIED Requirements

### Requirement: Records carry the recorded mode
FFI records SHALL carry every field the recorded modes add to library results, as the `Mode` and `UnchangedReason` enums. This adds to "Records mirror library results". Verified by `records_convert_field_by_field`.

#### Scenario: Mode fields
- **WHEN** Swift bindings are generated
- **THEN** `InstalledPackage`, `PackageRecord`, `CheckRow`, `RevisionChange` and `InstallResult` have `mode` (`release`, `pinned(reference)`, `branch(name)`), `Revision.mode` is the mode of a `previous` revision, `InstallResult` has `requestedMode`, and `RevisionChange.unchangedReason` and `Unchanged.reason` are `pinned(reference)`, `diverged(branch)`, `branchGone(branch)` or (`updateAll` only) `brokenDependency(dependency, problem)`

#### Scenario: Pinned package from Swift
- **WHEN** a host lists a workspace where `acme/greeter` was installed with `--ref acme/greeter@0.1.0` and then calls `update(id, .recorded)`
- **THEN** the listed `InstalledPackage.mode` is pinned to `acme/greeter@0.1.0`, and the returned `RevisionChange` is unchanged with `unchangedReason` `pinned`
