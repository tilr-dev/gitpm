# Proposal: OpenSpec for Claude Code and Codex

> Retro archive entry, reconstructed on 2026-10-18 from git history, PRs, Basic Memory notes and agent session logs. Spec deltas are not kept; the resulting behavior is in `openspec/specs/`.

## Why

Rationale not recorded beyond the commit: set up spec-driven workflow tooling for both agents. This migration of the existing design into OpenSpec specs, archive and backlog proposals followed on 2026-10-18 (uncommitted at the time of writing).

## What Changes

- OpenSpec 1.14.3 skills and `/opsx` commands for Claude Code (`.claude/`) and Codex (`.agents/`), `openspec/config.yaml`, empty `specs/` and `changes/archive/`.
- Follow-up (this migration): `config.yaml` context and rules, 18 capability specs of v0.5.0 + dev, retro archive, open backlog changes.

## Capabilities

### Modified Capabilities
- None (tooling/process only).

## Out of scope

- Any behavior change in crates/.

## Impact

Implemented by: [`0b146cf`](https://github.com/tilr-dev/gitpm/commit/0b146cf) chore: set up OpenSpec for Claude Code and Codex (2026-10-18 00:04).
