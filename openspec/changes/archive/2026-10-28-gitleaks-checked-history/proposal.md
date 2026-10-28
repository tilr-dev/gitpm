# Scan secrets in the checked commit history

## Why
The CI checkout fetches all branches with `fetch-depth: 0`, while the current
`gitleaks git` command defaults to `git log --all`. A secret committed only to an
unmerged branch can therefore fail a check of a clean integration commit.

## What Changes
- Keep full checkout history and restrict gitleaks to commits reachable from HEAD
  with `--log-opts=HEAD` on push, pull request and merge queue checks.
- Exercise the workflow command against isolated repositories containing an
  unrelated secret and a secret in the checked commit ancestry.
- Describe the scan scope in contributor guidance.

## Out of scope
- Changing event triggers, tool versions, secret rules or redaction.
- Changing local task-range checks or reusable release workflows.

## Impact
Only this repository's gitleaks check and its documentation change. No reusable
workflow interface or consumer changes.
