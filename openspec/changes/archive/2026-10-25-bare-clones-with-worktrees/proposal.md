# Bare clones with package worktrees

## Why
A checked-out source clone gives packages from the same repository one shared HEAD. Independent package revisions need independent working trees and one uniform local-work guard.

## What changes
- Store source repositories as bare clones under `repos/<key>`.
- Give each Git package a detached worktree at its recorded revision under `worktrees/<namespace>/<name>`.
- Keep prebuilt installations free of worktrees; preserve Git rollback objects only when needed.
- Apply package-local guards and preserve transactional staging, recovery and managed-path protection.

## Impact
Workspace storage, Git transport, installation lifecycle, transactions, tests and README. State stays at format 1, without migration of old checked-out clones.
