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
parent is dev, its second must be the exact candidate SHA). Local merge
automation passes the dev commit and its uncommitted squash result the same way.
Use `--comments-json /trusted/comments.json` for offline verification with an
exported array of comment activity records. Never accept that file from a PR.

The checker reads `get_issue_by_number` and all `list_issue_activity` comment
pages through the read-only MCP endpoint. It accepts scope declarations from the
Intake Bot identity and authorization blocks from Dan's member identity, using
stable user IDs rather than display names. It does not use task descriptions or
bot-authored blocks as permission. Comment arrays must be complete and preserve
activity metadata. Malformed scope or incomplete reads fail closed.

Authorization blocks use exact component, capability, requirement and operation:

```text
spec-change:
- cloud: specs/sessions — «Session lifetime»: modify
- cloud: changes/session-rework: archive
```

Changing an existing line requires a matching operation. Adding a requirement or
editing the own change declared in intake scope needs no block. A MODIFIED,
REMOVED or RENAMED delta in that own change still requires authorization for
existing requirements. Foreign changes require `edit` or `archive`; renaming a
requirement is authorized only when its body is preserved. Changes outside a
recognized specification/change layout fail closed.

Run the deterministic fixture, real Git diff/CLI and mocked MCP contract tests:

```sh
python3 -m unittest discover -s scripts -p test_spec_change.py
```

## Deployment decision

A required status check must run trusted code. A `pull_request` workflow is
loaded from the candidate merge commit, so a PR can replace the checker job with
a successful no-op or capture the read-only task token. Checking out a trusted
script does not protect the workflow file itself.

The prepared `pull_request_target` workflow uses the default branch's workflow,
downloads a shared checker with a pinned SHA-256 digest, and fetches candidate
Git objects into a bare repository solely for inspection. It targets PRs into dev, has `contents: read`, supplies only the
read-only task token to the checker, and never executes candidate files. Its job
is named `spec-change`, with no path filter or skip condition. The existing main
checks remain as they are; the desktop workflow limits full PR CI to main.

GitHub loads `pull_request_target` from the default branch, so deployment must
include that branch. On 2026-10-21 all four default branches are main (verify with
`gh api repos/tilr-dev/REPO --jq .default_branch`). The one-time bootstrap into main has been authorized. Required-check
activation also needs a bootstrap order: publish the shared checker, deploy the
trusted workflows, observe the check, then add `spec-change` to each dev ruleset
while preserving all existing rules and main protections. Adding the rule first
would strand the bootstrap PR without its new workflow.

See GitHub's [trusted event behavior](https://docs.github.com/en/actions/reference/security/securely-using-pull_request_target)
and [required check evaluation](https://docs.github.com/en/pull-requests/how-tos/merge-and-close-pull-requests/troubleshooting-required-status-checks).

## Required-check source binding

The trusted workflows and required dev checks are deployed in gitpm, cloud, and
desktop-mac. The tilrd activation follows its release from dev to main. Protecting
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

The deployment order is: bootstrap the trusted workflows and shared checker into
main, activate the required check in each dev ruleset, then validate this behavior
against the active protection. Before creating or closing the disposable,
non-merged test PR, obtain separate explicit authorization. The test PR must
remain unmergeable with both an unauthorized spec edit and an independent
successful job named `spec-change`. Deployment alone does not complete this
verification. If the bypass is confirmed, enforcement changes need a new decision.
The [required-workflow rule](https://docs.github.com/en/enterprise-cloud%40latest/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/available-rules-for-rulesets#require-workflows-to-pass-before-merging)
is documented for Enterprise Cloud. On 2026-10-21 the organization reports Team
(`gh api orgs/tilr-dev --jq .plan.name`). Changing the enforcement architecture or
accepting this limitation requires an explicit decision.
