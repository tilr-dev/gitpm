## 1. Specification
- [x] Rewrite branch flow, CI checks, versioning and release requirements for `main` and `release/*`.
- [x] Rename spec-change authorization to `main` and update the CLI dev build example.
- [x] Replace the merge diagnostics requirement with "Specification merge readiness diagnostics on main" (scenario "Pull request conflicts with main"): a modified requirement cannot rename a scenario.

## 2. Implementation
- [x] Run release-please only on `release/**` with the pushed branch as `target-branch`.
- [x] Run CI on pull requests into and pushes to `release/**`.
- [x] Drop `target-branch: dev` from Dependabot.
- [x] Tell a conflicting pull request to merge `main`; update fixtures, notes and the digest pin.
- [x] Remove remaining references to the `dev` branch from docs and comments.

## 3. Validation
- [x] Shared helper tests pass.
- [x] `openspec validate release-branch-flow --strict` passes.
- [x] `git grep -w dev` leaves only `/dev/null`, the `-dev+HASH` version suffix, the `--dev` option, history, the spec text this change replaces on archive and one comment in `scripts/spec_change.py` (tilrd pins that file's digest from gitpm main, so it stays byte-identical).
