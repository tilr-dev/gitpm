## MODIFIED Requirements

### Requirement: Read-only commands never change dist
`list` and `check` SHALL NOT reconcile or deploy anything in `dist/`, and neither opens the workspace (no lock, no crash recovery; transactions-locking, "Workspace lock"), so an interrupted commit is completed only by the next command that opens it.

#### Scenario: list on a 0.4 workspace
- **WHEN** a workspace written by gitpm 0.4 or 0.5 (state format 2) is listed
- **THEN** `list` fails with `unsupported` (workspace-layout, "State format versions") and `dist/` is unchanged

#### Scenario: list after an interrupted commit
- **WHEN** `.pm-txn/` holds a committed journal and the user runs `gitpm list`
- **THEN** `list` reports `state.json` as it is and leaves `.pm-txn/` and `dist/` alone

