# Proposal: Joint compatible upgrade: `gitpm update all`

> Retro archive entry, reconstructed on 2026-10-18 from git history, PRs, Basic Memory notes and agent session logs. Spec deltas are not kept; the resulting behavior is in `openspec/specs/`.

## Why

Individual updates cannot move an app and its dependency across a major boundary together. The owner asked for an APT-like "update everything choosing compatible versions", with tests (Codex session 2026-10-16 20:52).

## What Changes

- `gitpm update all [--dry-run]`, `Manager::update_all_plan()`/`update_all()`, Swift/Kotlin bindings.
- Joint PubGrub resolution across all installed plugins (dependencies through active forks included); prefers newer compatible stable releases, keeps installed versions as fallbacks, installs missing catalog dependencies, reports kept-back packages.
- One recoverable transaction; preserves replacement bindings and disabled flags; refuses local work and moved tags; never removes or downgrades; dry-run validates without committing.
- Recovery tested by crashing at each transaction-write stage.

## Capabilities

### Modified Capabilities
- `updates`

## Out of scope

- Pre-release versions (stable only; see change `pre-release-versions`).
- Removals as in `apt full-upgrade`.
- Maximizing the number of upgraded packages.

## Impact

Implemented by: [`1680bfc`](https://github.com/tilr-dev/gitpm/commit/1680bfc) feat: resolve and apply compatible updates for all plugins ([#5](https://github.com/tilr-dev/gitpm/pull/5), 2026-10-16 22:12); [`a6addbe`](https://github.com/tilr-dev/gitpm/commit/a6addbe) release 0.3.0 ([#6](https://github.com/tilr-dev/gitpm/pull/6)).
