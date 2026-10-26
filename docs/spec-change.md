# Specification authorization checker

`spec_change.py` is the single Python 3 standard-library checker shared by gitpm,
tilrd, cloud, and desktop-mac. It reads Git blobs as data without checking out or
executing candidate code. Exit 0 means authorization succeeds, including legacy
tasks without intake scope; exit 1 means uncovered changes or an input/API error.

```sh
export ITSAPLAN_READ_ONLY_TOKEN  # Dedicated CI read-only credential, never a bot key.
python3 /trusted/gitpm/scripts/spec_change.py \
  --repo /path/to/component --component cloud \
  --base origin/dev --head HEAD --branch task/TILR-123
```

`--base` and `--head` identify commits. `--head` must be the merge result of the
candidate into the current dev and `--base` that dev commit, not the branch tip:
a branch that is behind dev would otherwise "remove" every requirement dev gained
after it forked. CI uses GitHub's test merge `refs/pull/<n>/merge` (its first
parent is dev, its second must be the exact candidate SHA). The prepared
[merge readiness helper](../scripts/spec-merge.md) reads PR mergeability before
fetching: conflicts instruct the author to merge dev and push, while pending or
stale results are polled for approximately 60 seconds before advising a re-run.
It requires `pull-requests: read` in addition to `contents: read`; activation in
the main-only trusted workflows is a separate explicit deployment. Local merge
automation passes the dev commit and its uncommitted squash result the same way.
Use `--comments-json /trusted/comments.json` for offline verification with an
exported array of comment activity records. Never accept that file from a PR.

The checker reads `get_issue_by_number` and all `list_issue_activity` comment
pages through the read-only MCP endpoint. It accepts scope declarations from the
Intake Bot identity and authorization blocks from Dan's member identity, using
stable user IDs rather than display names. It does not use task descriptions or
bot-authored blocks as permission. Comment arrays must be complete and preserve
activity metadata. Malformed scope or incomplete reads fail closed. The own
change is the single `change <id>` inside `own (...)` of the latest `Spec scope:`
line; no id, several ids, or a `change <id>` outside `own (...)` fail closed.

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
requires `modify` for that requirement. An archive already in dev counts as a
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

The deployed `pull_request_target` workflow `.github/workflows/spec-change.yml`
exists only in each component's main branch: GitHub loads `pull_request_target`
workflows from the default branch, so dev trees and task branches carry no copy
and cannot change it. It downloads the shared checker from gitpm main, verifies a
SHA-256 digest pinned in main's workflow, and fetches candidate
Git objects into a bare repository solely for inspection. It targets PRs into dev, has `contents: read`, supplies only the
read-only task token to the checker, and never executes candidate files. Its job
is named `spec-change`, with no path filter or skip condition. The existing main
checks remain as they are; the desktop workflow limits full PR CI to main.

The shared checker and trusted workflows are deployed in main for gitpm, cloud,
and desktop-mac, with `spec-change` required by their dev rulesets. The tilrd
rollout follows once its main carries the workflow. Existing main protections
remain in place.

A checker change reaches CI in two steps: the new `scripts/spec_change.py`
lands in gitpm main, and a pull request into main of each component updates the
pinned digest (`shasum -a 256 scripts/spec_change.py`) in its workflow. Until the
digest is updated, the download fails the check closed.

See GitHub's [trusted event behavior](https://docs.github.com/en/actions/reference/security/securely-using-pull_request_target)
and [required check evaluation](https://docs.github.com/en/pull-requests/how-tos/merge-and-close-pull-requests/troubleshooting-required-status-checks).

## Required-check source binding

The trusted workflows and required dev checks are deployed in gitpm, cloud, and
desktop-mac. The tilrd activation follows once its main carries the workflow. Protecting
workflow code and binding the required result to that workflow are separate controls.
GitHub's [ruleset troubleshooting documentation](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/troubleshooting-rules)
says that required status checks identify jobs by name and ignore the workflow
and event type. Restricting the source to the GitHub Actions app still permits
other Actions workflows to produce the same job name.

A candidate could add this independent workflow alongside an unauthorized spec
edit:

```yaml
name: Alternate check
on:
  pull_request:
    branches: [dev]
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
