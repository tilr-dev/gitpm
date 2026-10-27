## MODIFIED Requirements

### Requirement: Active fork rules
Single source for what happens to a replacement binding when a package leaves or changes. Disabling, forgetting or uninstalling an active fork is guarded as in dependencies ("Dependents guard removal"). A fork update SHALL keep declaring `replaces`. Uninstalling or forgetting the fork SHALL drop its binding and deploy ORIGINAL's `dist/` again in the same transaction when ORIGINAL is active then; uninstalling or forgetting the original SHALL keep the binding.

#### Scenario: Fork release without replaces
- **WHEN** the fork's newer release no longer declares `replaces` for ORIGINAL
- **THEN** `update FORK` (and `rollback` to such a revision) fails with `version_conflict` ("… no longer declares that it replaces ORIGINAL …"), and `update all` keeps the fork back with the reason `no longer replaces ORIGINAL` (`HeldBack.reason` `no_longer_replaces`; `update_all_resolves_replacement_versions_together_with_consumers`)

#### Scenario: Uninstall the fork
- **WHEN** `me/greeter` replaces `acme/greeter` and the user uninstalls `me/greeter`
- **THEN** the binding is dropped and `dist/acme/greeter/` is deployed again
- **AND** this is covered by `uninstalling_or_forgetting_the_replacement_deploys_the_original_again`

#### Scenario: Reinstalled original stays replaced
- **WHEN** the original `vendor/base` is uninstalled while replaced and later installed again
- **THEN** it is still replaced by the fork until `replace vendor/base --back`
