## ADDED Requirements

### Requirement: update all reports every package
`UpdateAllPlan` (of `update_all_plan` and `update_all`) SHALL list every installed package in exactly one of `updates`, `installs`, `kept_back` and `unchanged` (packages that stay at their installed revision, with their version), so the output is built from the plan alone. Verified by `update_all_plan_lists_unchanged_packages`.

#### Scenario: One update, one current
- **WHEN** `a` has a newer release and `b` is current, and the user runs `update all`
- **THEN** the plan has `a` in `updates` and `b` with `2.0.0` in `unchanged`, and the CLI prints `b up to date (2.0.0)` from it
