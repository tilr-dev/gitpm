## ADDED Requirements

### Requirement: Mutating results carry the dist report
Every mutating library operation SHALL return, inside its result, the reconciliation report of its transaction: `deployed`, `removed`, `unmanaged` (paths under `dist/` left alone) and `skipped` (active packages not deployed, each with its reason). The library SHALL NOT print it; the CLI turns `unmanaged` and `skipped` into warnings (cli, "Warnings go to stderr") and an FFI host reads the same fields. Verified by `workspace::mutating_results_carry_the_dist_report`.

#### Scenario: Unknown folder in an install result
- **WHEN** `dist/custom/` belongs to no package and the user installs `acme/greeter`
- **THEN** the `InstallResult` lists `dist/custom` in its dist report's `unmanaged`, and the CLI prints `Warning: dist/custom belongs to no known package or is not a folder; left as it is`

#### Scenario: Skipped package in an enable result
- **WHEN** the clone of an active package is missing and the user enables another package
- **THEN** the enable result's dist report lists the package in `skipped` with the reason `clone missing`

## MODIFIED Requirements

### Requirement: sync-dist repairs dist explicitly
`gitpm sync-dist [--dry-run]` (`Manager::sync_dist`, `sync_dist_plan`, FFI `syncDist`/`syncDistPlan`) SHALL perform only the reconciliation, report each package deployed (`ID deployed to dist/ID`) or removed (`ID removed from dist/ID`; `would be …` with `--dry-run`) and sync every package it can. When an active package stays undeployed, `sync_dist` SHALL then fail with `not_deployed`, carrying the report; what it synced stays. Verified by `sync_dist_repairs_a_tree_written_by_an_older_gitpm`.

#### Scenario: Package that cannot be deployed
- **WHEN** the clone of an active package is missing and the user runs `sync-dist`
- **THEN** it syncs the others and fails with `not_deployed` ("N package(s) not deployed: IDS"); the CLI prints `Error kind: not_deployed` and exits with status 1

#### Scenario: Nothing to do
- **WHEN** `dist/` already matches the active packages (unmanaged paths may exist; they are only warned about)
- **THEN** `sync-dist` prints `dist/ already holds exactly the active packages.` and exits 0

#### Scenario: Dry run
- **WHEN** the user runs `sync-dist --dry-run` on a drifted workspace
- **THEN** it prints the `would be deployed to` / `would be removed from` lines and `Nothing was changed (--dry-run).` and changes nothing; `sync_dist_plan` lists undeployable packages in `skipped` and does not fail

### Requirement: Read-only commands never change dist
`list` and `check` SHALL NOT reconcile or deploy anything in `dist/`, and neither opens the workspace (no lock, no crash recovery; transactions-locking, "Workspace lock"), so an interrupted commit is completed only by the next command that opens it.

#### Scenario: list on a 0.4 workspace
- **WHEN** a workspace written by gitpm 0.4 is listed by a later gitpm
- **THEN** `dist/` is unchanged

#### Scenario: list after an interrupted commit
- **WHEN** `.pm-txn/` holds a committed journal and the user runs `gitpm list`
- **THEN** `list` reports `state.json` as it is and leaves `.pm-txn/` and `dist/` alone
