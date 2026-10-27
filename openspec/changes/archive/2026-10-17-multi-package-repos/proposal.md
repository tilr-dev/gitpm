# Proposal: Multi-package repositories

> Retro archive entry, reconstructed on 2026-10-18 from git history, PRs, Basic Memory notes and agent session logs. Spec deltas are not kept; the resulting behavior is in `openspec/specs/`.

## Why

The test bench needed ten Tilr packages and one repository per package was impractical (Claude S6; Basic Memory note "gitpm — монорепо: несколько пакетов-папок в одном git-репо"). Design rule from the owner: exactly one `a/b` notation (a package ID); a repository is always a path/URL and never derived from an ID.

## What Changes

- Root `gitpm.json` `{"packages": ["packages/*"]}`; per-folder `plugin.json` + `dist/`; tags `ns/name@VERSION`.
- One shared clone per source (`repos/<name>-<hash>`); `refs/gitpm/<ns>/<name>/{current,previous}` keep-alive refs; `forgotten` records; state `"format": 2`.
- `install REPO [ID]`/`--package`, `commit --package`, `init ID --into DIR`, `register --package`; catalog releases carry `path`.
- `fork ID [ID...] --ns NS --into DIR [--replace [--version V] [--force]]`: unforked siblings removed from the tree, dependencies between forks rewritten, adding to an existing fork.
- Review fixes B1–B6 (detached HEAD on fork, update after a package moved, `insteadOf`, forget of a shared clone, shared HEAD ownership, broken sibling).
- **BREAKING**: `fork ID NS/NAME` and `init --in` removed; `--into` required; `Manager::fork` with default namespace, namespace config and `scaffold::create*` removed; 0.4 is not downgrade-safe.

## Capabilities

### Modified Capabilities
- `multi-package-repos`
- `fork-replacement`
- `workspace-layout`
- `authoring`
- `catalog`

## Out of scope

- `fork REPO` without a workspace (open question; see change `fork-repository-without-workspace`).
- `**` globs.
- Real split of Tilr plugins with history (Tilr side).

## Impact

Implemented by: [`744809d`](https://github.com/tilr-dev/gitpm/commit/744809d) feat: multi-package repositories ([#9](https://github.com/tilr-dev/gitpm/pull/9), 2026-10-17 18:53); [`9200a27`](https://github.com/tilr-dev/gitpm/commit/9200a27) release 0.4.0 ([#10](https://github.com/tilr-dev/gitpm/pull/10)).
