## ADDED Requirements

### Requirement: Linked deployments expose the author installation folder
For link records, gitpm SHALL deploy `dist/<id>` as a symlink to the author-owned installation folder. It SHALL use the same `plugin.json.dist` folder selection as managed install: `"dist"` by default, or the configured package-relative folder (`"."` selects the package root). gitpm MUST NOT build, watch, copy or change its contents.

#### Scenario: Uncommitted author edits
- **WHEN** an active linked package's author edits a file in its installation folder
- **THEN** reading that file through `dist/<id>` returns the edited bytes without a commit or another gitpm operation

#### Scenario: Author-maintained build output
- **WHEN** the package declares an installation folder and the author writes new output there
- **THEN** `dist/<id>` resolves to that folder and exposes the new output without gitpm executing a build or watcher

### Requirement: Linked deployments follow activity without owning their target
Enable, disable, replacement, forget, restore, uninstall and reconciliation SHALL create or remove the linked package's deployed entry according to the existing active-package rules. They MUST NOT require a managed clone for a link record or delete or modify its external target. Missing linked folders SHALL be reported structurally as not deployed, with the link mode retained, and unrelated packages SHALL remain operable under the existing reconciliation rules.

#### Scenario: Disable and enable a link
- **WHEN** an enabled linked package is disabled and then enabled
- **THEN** its symlink is removed and recreated, and its external folder keeps its contents

#### Scenario: Author moves the target away
- **WHEN** a recorded linked folder no longer exists and reconciliation runs for another package
- **THEN** the report identifies the unavailable linked package and its link mode, and no clone or build output is created for it

