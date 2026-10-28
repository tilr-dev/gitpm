## ADDED Requirements

### Requirement: CI scans secrets in the checked commit's history only
The `gitleaks` CI check SHALL scan every commit reachable from the checked-out
HEAD, with redacted findings, and SHALL exclude commits reachable only from other
fetched branches. It MUST retain full checkout history and use
`gitleaks git --log-opts=HEAD` for push, pull request and merge queue events.

#### Scenario: Unrelated branch contains a secret
- **WHEN** a push, pull request or merge queue check has a clean HEAD history and a fetched unrelated branch contains a recognized secret
- **THEN** the `gitleaks` check passes without reporting that unrelated secret

#### Scenario: Secret removed from the checked tree
- **WHEN** a recognized secret exists in an ancestor of HEAD but has been removed from the checked tree
- **THEN** the `gitleaks` check fails with a redacted finding

#### Scenario: Checked merge introduces a secret
- **WHEN** the checked pull request or merge queue merge commit includes a parent containing a recognized secret
- **THEN** the `gitleaks` check fails with a redacted finding
