# Tasks

## 0. Decision

- [ ] 0.1 Owner confirms the need and the opt-in model; write the spec deltas for `package-ids`, `dependencies` and `updates` and drop `skip_specs`

## 1. Implementation

- [ ] 1.1 Extend version parsing/ordering to SemVer pre-releases; unit tests for ordering (`1.0.0-alpha < 1.0.0-beta < 1.0.0`)
- [ ] 1.2 Exclude pre-releases from implicit selection; tests for install/update/update all with and without opt-in
- [ ] 1.3 Decide and test how 0.5 clients behave on catalogs containing pre-releases; document in README
