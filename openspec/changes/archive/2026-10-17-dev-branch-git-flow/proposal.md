# Proposal: Development on `dev`, main and releases only on command

> Retro archive entry, reconstructed on 2026-10-18 from git history, PRs, Basic Memory notes and agent session logs. Spec deltas are not kept; the resulting behavior is in `openspec/specs/`.

## Why

An earlier "merge everything" reached `main` and opened a release PR. The owner: "I don't want to push every change to master and release; too long and wrong" (Claude S6; Basic Memory note, section "Git-поток tilr-dev").

## What Changes

- Work on `dev` (or feature branches off `dev`), tested locally; nothing to `main` without the explicit word "в main"; "merge" without a target means `dev`; a release is a separate explicit step.
- Rule recorded in `~/.claude/CLAUDE.md` and Basic Memory; #13 and #14 merged separately so the changelog has distinct entries; release PR #12 held until "release".
- After 0.5.0: `dev` stops tracking `origin/main`, stale branches removed (Claude S7).

## Capabilities

### Modified Capabilities
- `release-process`

## Out of scope

- Changes to GitHub rulesets (unchanged).

## Impact

Implemented by: Process change, no code commit. Applied from 2026-10-17 evening.
