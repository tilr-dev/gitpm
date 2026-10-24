## MODIFIED Requirements

### Requirement: Dependencies between forks are rewritten
Every dependency between packages of the source repository SHALL be rewritten to its fork; the requirement SHALL be kept when the fork's version satisfies it, else become `^V`, and each rewrite SHALL be printed. A manifest depending on both an original and its fork SHALL be refused with `invalid_manifest`.

#### Scenario: Requirement replaced
- **WHEN** the fork's version does not satisfy the original requirement
- **THEN** the output has `Dependency: P now depends on F ^V (was O REQ, which the fork's version does not satisfy)`

#### Scenario: Rewrite printed
- **WHEN** `tilr/panel` depends on `tilr/theme ^1.0` and both are forked into `me`
- **THEN** the output has `Dependency: me/panel now depends on me/theme ^1.0 (was tilr/theme)`
- **AND** this is covered by `fork::a_dependency_on_an_original_and_its_fork_is_refused_in_any_key_order`

#### Scenario: Dependency cycle between forks
- **WHEN** packages forked together depend on each other in a cycle
- **THEN** fork rewrites the dependencies to their fork IDs without trying to order a release or installation
- **AND** the cycle alone does not trigger an implicit release, installation or replacement
