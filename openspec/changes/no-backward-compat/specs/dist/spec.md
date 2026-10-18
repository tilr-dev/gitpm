## MODIFIED Requirements

### Requirement: dist holds exactly the active packages
The deployed folder `dist/<namespace>/<name>/` SHALL exist exactly when the package is active: installed, enabled and not replaced by another package. A host SHALL be able to load every package folder under `dist/` without consulting gitpm state while no transaction is committing (see the scenario). Verified by `dist_active.rs` (CLI) tests.

#### Scenario: Host reads during a commit
- **WHEN** a host reads `dist/` while `.pm-txn/` exists in the workspace root
- **THEN** it may miss a package, because a commit moves deployed folders one by one; hosts read `dist/` after the gitpm call returned

#### Scenario: Installed and enabled
- **WHEN** `acme/greeter` is installed, enabled and not replaced
- **THEN** `dist/acme/greeter/` holds the files of its release's package dist

#### Scenario: Enabled but replaced
- **WHEN** `acme/greeter` is replaced by `me/greeter`
- **THEN** `dist/acme/greeter/` does not exist, `dist/me/greeter/` does, and `acme/greeter` stays `enabled: true` with `active: false` in `list`
