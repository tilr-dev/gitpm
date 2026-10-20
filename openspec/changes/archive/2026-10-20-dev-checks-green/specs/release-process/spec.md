## ADDED Requirements

### Requirement: Scoped security-check exceptions
Security-check exceptions SHALL identify a specific confirmed finding and document
its reason. A synthetic credential used only by a local test server MAY be ignored
by its gitleaks fingerprint. An unmaintained transitive dependency MAY have an
advisory-specific exception when no released upstream update removes it; the
exception SHALL document a dated verification and a removal condition. Other
secrets and advisories SHALL remain subject to the existing checks.

#### Scenario: Loopback Git fixture in history
- **WHEN** gitleaks scans the test-only credential introduced in commit bbe8cf6511b9cb96d858f7c65f046e0c06dc9295 at itests/scenarios/uc_049.py:25
- **THEN** only that finding's fingerprint is ignored
- **AND** unrelated credentials are still reported

#### Scenario: Unmaintained transitive parser without an upstream replacement
- **WHEN** the released UniFFI and Askama dependency tree still requires basic-toml
- **THEN** cargo-deny permits only advisory RUSTSEC-2026-0374 with a documented reason
- **AND** all other advisory checks stay enabled
- **AND** the exception is removed once an upstream update removes basic-toml
