## MODIFIED Requirements

### Requirement: Conflicts are explained
Unsatisfiable dependencies SHALL fail with `dependency_conflict` carrying the solver's explanation that names the packages involved. Cycles between installed packages SHALL resolve normally. Verified by `deps::solver_explains_conflicts_naming_both_consumers` and `deps::solver_accepts_cycles_and_reports_missing_packages`.

#### Scenario: Two consumers disagree
- **WHEN** installed `a` requires `lib ^1` and the new package `b` requires `lib ^2`
- **THEN** install fails with `dependency_conflict` and the message names `a`, `b` and `lib`
