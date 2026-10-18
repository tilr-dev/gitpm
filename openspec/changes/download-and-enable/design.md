# Design

## Context

`deps.rs` builds the fixed set for install/update from state without looking at `enabled`, `upgrade.rs` skips disabled providers, and `list`/`check` report them unsatisfied. Decision D7 makes "active" the only thing that satisfies a dependency and adds `download`.

## Decisions

### Enable of an app whose library is disabled: refuse (owner decision, 2026-10-18)
`enable app` with `lib` disabled fails with `dependency_conflict` ("lib is disabled; enable it"); the user enables `lib` first, or runs `enable app --force`, which enables `app` anyway and leaves it reported broken. Rationale: enabling is the user's choice per package (a host may keep a provider disabled on purpose, for example one of two themes); silently enabling `lib` would change what the host loads beyond the request.
Alternative: `enable` enables the disabled dependencies it needs in the same transaction (like `install` does for what it downloaded). Rejected by the owner (2026-10-18, on TILR-36).

### Download resolves like install, activates nothing
`download` runs the same solve as `install` (installed packages fixed at their versions, missing ones from the catalog) but records every new package with `enabled: false`. Disabled installed packages may fix versions during a download, because nothing becomes active; the check that matters happens at `enable`.

### install = download + enable in one transaction
`install` stages the download and the enable of the package and the dependencies it downloaded together, so a failure leaves nothing (errors, "Failing operations leave the workspace unchanged"). An installed but disabled dependency is not enabled by `install`: the install fails with `dependency_conflict` ("lib is disabled; enable it").

### API
`Manager::download(source, package, target) -> InstallResult` and `download_from_catalog(id, version) -> InstallResult` (results carry `enabled: false` records and the dist report, which deploys nothing); `Manager::enable_with(id, force) -> DistSync` (the dist report, like `disable`); `install_package_with(source, package, target, force)` and `install_from_catalog_with(id, version, force)` -> `InstallResult` (`enabled`, `dependencies`). FFI `download`, `downloadFromCatalog`, `enable(id, force)`, `install(…, force)`, `installFromCatalog(…, force)`.

### One active-dependency check
Every operation that makes or keeps a package active (install, install over a download, `update ID`, `enable`, `rollback`, `restore`) runs one check in `deps.rs` (`refuse_inactive_dependencies`): each direct dependency must be satisfied by an active provider (the `list` status). Dependencies of providers are theirs to report (a broken provider is reported, not enforced); versions and missing packages are already the solver's job before the check. Dependencies an inactive package pulls (download, `update` of a disabled package) are recorded disabled.

## Transactions and crash recovery

`download` uses the install transaction without the deploy step: clones and state are staged in `.pm-txn` and committed by the journal; a crash before commit leaves nothing, after commit the journal rolls forward. `enable` stays a state + dist transaction; the dependency check runs under the lock before staging.
