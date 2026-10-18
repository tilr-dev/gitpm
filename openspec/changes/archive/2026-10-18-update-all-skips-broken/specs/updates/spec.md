## MODIFIED Requirements

### Requirement: update all never removes or downgrades
`update all` SHALL NOT remove or downgrade packages, SHALL preserve replacement bindings and disabled flags (a disabled package satisfies no dependency of an active package), SHALL solve only healthy packages and SHALL report each kept-back package with its latest version and `HeldBack.reason`; a broken state SHALL NOT fail the run. A broken consumer (an installed package with a direct dependency unsatisfied now, as `list` reports it; an inactive package takes a disabled provider of a matching version as given) SHALL stay at its installed revision and be listed in `unchanged` with reason `broken_dependency`, and its unsatisfied dependencies SHALL stay out of the solve, so they are never installed for it. Verified by `update_all_does_not_downgrade_or_accept_mutated_releases`, `update_all_reports_unsatisfiable_state_and_preserves_disabled_flags`, `update_all_skips_broken_consumers_and_never_reinstalls` `update_all_keeps_back_a_release_needing_a_missing_dependency_without_a_catalog`, `update_all_pulls_disabled_dependencies_for_a_disabled_consumer` and `update_all_enables_dependencies_of_active_consumers_only`. A new dependency SHALL be installed enabled only when an active package of the solution pulls it, as `update ID` does.

#### Scenario: Kept back
- **WHEN** the newest release of a package does not fit a dependent's requirement
- **THEN** the CLI prints `ID kept back at V (latest L: dependency constraints)`, or, when it advances to an intermediate compatible release, `ID updated A → B (latest L kept back: dependency constraints)`, the reason taken from `HeldBack.reason`
- **AND** a package whose newer release needs a dependency that is not installed, with no catalog configured, is kept back with the reason `needs DEP REQ, which is not installed and no catalog is configured`

#### Scenario: Reasons
- **WHEN** a package is kept back
- **THEN** `HeldBack.reason` is `constraints` (`dependency constraints`), `missing_dependency` (`needs DEP REQ, which is not installed and no catalog is configured`) or `no_longer_replaces` (`no longer replaces ORIGINAL`, a replacing package whose newest release or branch commit no longer declares `replaces`)
- **AND** a diverged branch history stays an `unchanged` reason (`diverged`), not a kept-back one

#### Scenario: Broken state does not fail the run
- **WHEN** a consumer's dependency was removed with force and the other packages have newer releases
- **THEN** `update all` updates the others, lists the consumer in `unchanged` with reason `broken_dependency` (CLI `app skipped at V (dependency DEP is not installed)`) and exits 0, with or without a catalog; a single `update` of that consumer without a catalog still fails with `dependency_conflict` (dependencies)
- **AND** `broken_dependency` takes precedence over the mode reasons (`pinned`, `diverged`, `branch_gone`)

#### Scenario: Dependencies pulled for a disabled consumer
- **WHEN** a disabled package's newer release needs a package that is not installed, and the user runs `update all`
- **THEN** the dependency is installed from the catalog recorded disabled and not deployed, and it takes disabled providers as given; a new dependency an active package pulls is enabled and needs active providers (`update_all_pulls_disabled_dependencies_for_a_disabled_consumer`, `update_all_enables_dependencies_of_active_consumers_only`)

### Requirement: update all reports every package
`UpdateAllPlan` (of `update_all_plan` and `update_all`) SHALL list every installed package in `updates`, `kept_back` or `unchanged` (packages that stay at their installed revision with no newer release, or with a `reason` such as a broken consumer, also when it has a newer release; with their version); a package updated to a release older than its latest is in both `updates` and `kept_back`, and `installs` lists only new dependencies. The output is built from the plan alone. Verified by `update_all_plan_lists_unchanged_packages`.

#### Scenario: One update, one current
- **WHEN** `a` has a newer release and `b` is current, and the user runs `update all`
- **THEN** the plan has `a` in `updates` and `b` with `2.0.0` in `unchanged`, and the CLI prints `b up to date (2.0.0)` from it
