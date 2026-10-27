## ADDED Requirements

### Requirement: update all reports every package
`UpdateAllPlan` (of `update_all_plan` and `update_all`) SHALL list every installed package in `updates`, `kept_back` or `unchanged` (packages that stay at their installed revision with no newer release, with their version); a package updated to a release older than its latest is in both `updates` and `kept_back`, and `installs` lists only new dependencies. The output is built from the plan alone. Verified by `update_all_plan_lists_unchanged_packages`.

#### Scenario: One update, one current
- **WHEN** `a` has a newer release and `b` is current, and the user runs `update all`
- **THEN** the plan has `a` in `updates` and `b` with `2.0.0` in `unchanged`, and the CLI prints `b up to date (2.0.0)` from it
