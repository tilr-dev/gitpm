## MODIFIED Requirements

### Requirement: Filter by provided service
`list --provides SERVICE` (library `list_with`, FFI `list`) SHALL keep only the packages whose `provides` contains `SERVICE` in `packages` and `resolved`; `replacements` and `forgotten` SHALL be printed unfiltered, and inactive (replaced, disabled) providers SHALL be kept because `list` is a status view. The filter SHALL be applied by the library.

#### Scenario: Replaced original still listed
- **WHEN** `me/theme` replaces `tilr/theme`, both providing `theme`
- **THEN** `list --provides theme` lists both, with `active` false for `tilr/theme`
