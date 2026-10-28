# Specification authorization checker

`spec_change.py` is the single Python 3 standard-library checker shared by gitpm,
tilrd, cloud, and desktop-mac. It reads Git blobs as data without checking out or
executing candidate code. Exit 0 means authorization succeeds, including legacy
tasks created before the rule with trusted intake ready; exit 1 means uncovered changes or an input/API error.

```sh
export ITSAPLAN_READ_ONLY_TOKEN  # Dedicated CI read-only credential, never a bot key.
python3 /trusted/gitpm/scripts/spec_change.py \
  --repo /path/to/component --component cloud \
  --base origin/main --head HEAD --branch task/TILR-123
```

`--base` and `--head` identify commits. `--head` must be the merge result of the
candidate into the current main and `--base` that main commit, not the branch tip:
a branch that is behind main would otherwise "remove" every requirement main gained
after it forked. CI uses GitHub's test merge `refs/pull/<n>/merge` (its first
parent is main, its second must be the exact candidate SHA). The prepared
[merge readiness helper](../scripts/spec-merge.md) reads PR mergeability before
fetching: conflicts instruct the author to merge main and push, while pending or
stale results are polled for approximately 60 seconds before advising a re-run.
It requires `pull-requests: read` in addition to `contents: read`. Local merge
automation passes the main commit and its uncommitted squash result the same way.
Use `--comments-json /trusted/comments.json` for offline verification with an
exported object containing `issue` and `comments`. `issue` must include `id`,
`sequenceNumber`, `createdAt` (an ISO timestamp with timezone), and `columnId`;
`comments` is the complete comment activity array. A comments-only array is no
longer sufficient. The issue number must match the branch. Never accept that file
from a PR.

The checker reads `get_issue_by_number` and all `list_issue_activity` comment
pages through the read-only MCP endpoint. It accepts scope declarations from the
Intake Bot identity and authorization blocks from Dan's member identity, using
stable user IDs rather than display names. It does not use task descriptions or
bot-authored blocks as permission. Task metadata is required even when scope is absent. Comment arrays must be complete and preserve
activity metadata. Malformed scope or incomplete reads fail closed. The own
change is the single `change <id>` inside `own (...)` of the latest `Spec scope:`
line; no id, several ids, or a `change <id>` outside `own (...)` fail closed.

A legacy exception applies only when the task was created strictly before
`2026-10-22T21:02:26Z`, has a trusted intake-ready comment and no Spec scope, and is
neither Done nor Canceled. The cutoff is commit
`e31c1ac23de1b3f34323029f64dd3bb3a3629057`, where the original checker rule landed
in gitpm dev. A missing task, missing metadata, a closed task, or a newer task
without scope fails closed.

Dan's grants count only while the task is in Review, Supervise, or Merge. While
it is In Progress, additions and edits of the declared own change that require
no grant still pass; edits requiring a grant fail. After moving the task into
Review, rerun the required `spec-change` job to read its current column. The
checker fetches metadata on every invocation; moving a task does not itself
trigger a GitHub run.

Authorization blocks use exact component, capability, requirement and operation:

```text
spec-change:
- cloud: specs/sessions — «Session lifetime»: modify
- cloud: changes/session-rework: archive
```

Changing an existing line or inserting content into an existing requirement
requires a matching operation. Adding a requirement or
editing the own change declared in intake scope needs no block. A MODIFIED,
REMOVED or RENAMED delta in that own change still requires authorization for
existing requirements. Foreign changes require `edit` or `archive`; rewriting contents during an archive
requires both. A rename followed by a modification in an own delta requires
`modify` authorization for the original requirement name. Renaming a
requirement is authorized only when its body is preserved. Deleting or moving a
requirement heading, so that its text falls under another existing requirement,
requires `modify` for that requirement. An archive already in main counts as a
foreign change even when it carries the own change name. A new capability whose
name equals another one after case folding and Unicode NFC normalization fails
closed, and so does an own-change delta whose capability matches an existing one
only after that normalization. Changes outside a recognized specification/change layout fail closed.

Run the deterministic fixture, real Git diff/CLI and mocked MCP contract tests:

```sh
python3 -m unittest discover -s scripts -p test_spec_change.py
```

## Trusted deployment

A required status check must run trusted code. A `pull_request` workflow is
loaded from the candidate merge commit, so a PR can replace the checker job with
a successful no-op or capture the read-only task token. Checking out a trusted
script does not protect the workflow file itself.

For PR authorization, the deployed `.github/workflows/spec-change.yml`
is loaded from each component's main branch: GitHub loads `pull_request_target`
workflows from the default branch, so a task branch cannot change the workflow
that checks it. It downloads the shared checker scripts from gitpm (gitpm from
gitpm main; cloud, desktop-mac, and tilrd from a fixed gitpm commit), verifies
a SHA-256 digest pinned in main's workflow, and fetches candidate
Git objects into a bare repository solely for inspection. It targets PRs into main, has `contents: read`, supplies only the
read-only task token to the checker, and never executes candidate files. Its job
is named `spec-change`, with no path filter or job-level skip condition. The
fetch, download and authorization steps run only on `pull_request_target`.

The same workflow also handles `merge_group` with activity type
`checks_requested`. On that event, the `spec-change` job reports pass-through
success: it does not fetch PR trees, download or run the checker, or supply the
read-only task token. Queue admission requires each PR to have passed the trusted
`pull_request_target` authorization check; the queue's combined merge result is
tested by CI. This path therefore acknowledges authorization already performed
on the PRs rather than authorizing the combined specification again.

For `merge_group`, GitHub loads the workflow from the merge group commit, not
from main. It does not have the PR authorization path's trusted workflow source.
See GitHub's [merge group event documentation](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#merge_group).

The shared checker and trusted workflows are deployed in main for gitpm, cloud,
desktop-mac and tilrd, with `spec-change` required by their `main` rulesets.
In gitpm, desktop-mac and tilrd, pull requests into main go through the merge queue
(squash, all required checks green); cloud, a private repository, merges them directly.

A checker change (`scripts/spec_change.py` or `scripts/spec_merge.py`) lands in
gitpm main together with gitpm's own pinned digest (`shasum -a 256 scripts/<file>`).
A component that downloads the file from gitpm main fails the check closed until a
pull request into its main updates the digest; a component pinned to a gitpm commit
keeps the old checker until a pull request into its main moves the commit and the
digest together.

See GitHub's [trusted event behavior](https://docs.github.com/en/actions/reference/security/securely-using-pull_request_target)
and [required check evaluation](https://docs.github.com/en/pull-requests/how-tos/merge-and-close-pull-requests/troubleshooting-required-status-checks).

## Required-check source binding

The trusted workflows and required `main` checks are deployed in gitpm, cloud,
desktop-mac and tilrd. Protecting
workflow code and binding the required result to that workflow are separate controls.
GitHub's [ruleset troubleshooting documentation](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/troubleshooting-rules)
says that required status checks identify jobs by name and ignore the workflow
and event type. Restricting the source to the GitHub Actions app still permits
other Actions workflows to produce the same job name.

The `merge_group` pass-through uses that same `spec-change` context, but its
workflow comes from the merge group commit and does not run authorization.
A successful queue result is therefore not evidence of a fresh trusted checker
run. Its authorization rests on the prior trusted PR checks described above;
adding this event does not bind the required context to a particular workflow.

A candidate could add this independent workflow alongside an unauthorized spec
edit:

```yaml
name: Alternate check
on:
  pull_request:
    branches: [main]
jobs:
  spec-change:
    runs-on: ubuntu-24.04
    steps:
      - run: 'true'
```

The same-name collision was tested on 2026-10-22 in
[gitpm PR #42](https://github.com/tilr-dev/gitpm/pull/42): an unauthorized change to
the Branch flow requirement failed the trusted check while the independent
Probe job succeeded. GitHub reported the PR as BLOCKED. The probe commit was
then removed and the PR restored to draft. The trusted
[run 39761729014](https://github.com/tilr-dev/gitpm/actions/runs/39761729014)
reported the uncovered requirement. This experiment did not bypass the gate.
GitHub also documents that [test-merge results take precedence over head results](https://docs.github.com/en/pull-requests/how-tos/merge-and-close-pull-requests/troubleshooting-required-status-checks#conflicts-between-head-commit-and-test-merge-commit).
A content digest protects the checker executable, but does not bind the required
context to that executable.
