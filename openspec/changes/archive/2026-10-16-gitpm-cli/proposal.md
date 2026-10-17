# Proposal: gitpm-cli crate with the `gitpm` binary

> Retro archive entry, reconstructed on 2026-10-18 from git history, PRs, Basic Memory notes and agent session logs. Spec deltas are not kept; the resulting behavior is in `openspec/specs/`.

## Why

The owner: "GitPM is a minimal but separate project that lives on its own" and wanted to play with local folders; the stand's `PM_*` variables leaked from his shell into "clean" checks. He did not want a config file or a namespace command (Claude S5).

## What Changes

- New crate `crates/gitpm-cli`, binary `gitpm` (`cargo install gitpm-cli`), released in lockstep with `gitpm` via release-please extra-files; prebuilt binaries for macOS/Linux arm64/x86_64 with cargo-binstall metadata.
- Stand-only `sync`/`--publish` (Cloudflare) dropped.
- Settings `GITPM_ROOT`/`GITPM_CATALOG`/`GITPM_SOURCES`; old `PM_*` ignored; default workspace `./gitpm-workspace`; no default catalog.
- `.gitpm.json`, `--config`, `gitpm namespace`, `--namespace` removed; namespace required in `create`/`init`.

## Capabilities

### Modified Capabilities
- `cli`
- `release-process`

## Out of scope

- Clap migration (hand-written parser kept).
- Publishing catalogs to a CDN.

## Impact

Implemented by: [`6ffb2ea`](https://github.com/tilr-dev/gitpm/commit/6ffb2ea) feat: add gitpm-cli crate with the gitpm binary ([#3](https://github.com/tilr-dev/gitpm/pull/3), 2026-10-16 20:20); [`9483193`](https://github.com/tilr-dev/gitpm/commit/9483193) release 0.2.0 ([#4](https://github.com/tilr-dev/gitpm/pull/4)).
