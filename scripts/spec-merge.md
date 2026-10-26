# Trusted specification merge preparation

`spec_merge.py` prepares `refs/inspection/merge` and `refs/inspection/base` in an
initialized bare repository whose `origin` and Git authentication are already
configured. It reads the PR with `gh api repos/$REPOSITORY/pulls/$PR_NUMBER`
before fetching candidate objects. It never checks out or executes PR files.

A false `mergeable` value fails immediately with a conflict diagnostic and
`git merge dev` instructions for the candidate branch. A null value or a merge
whose second parent differs from `HEAD_SHA` triggers another API read after two
seconds, for approximately 60 seconds in total. Only expiration produces the
GitHub-not-updated / re-run diagnostic. A valid result sets the comparison base
to its first parent. API/command failures fail closed.

## Deployment from main

The helper and deterministic tests are prepared in dev. Activation is a separate
explicit deployment to main in gitpm, cloud and desktop-mac. No workflow copy
belongs in dev or a task branch. The existing shared authorization checker and
its content pin remain unchanged.

In each main-only `.github/workflows/spec-change.yml`:

1. Add `pull-requests: read` alongside `contents: read` in `permissions`.
2. Add `TASK_BRANCH: ${{ github.event.pull_request.head.ref }}` to the environment
   of the "Fetch specification trees without a checkout" step.
3. Keep bare-repository initialization, origin and authentication setup. Replace
   the existing fetch/parent-check/update-ref block with the commands below.
   Download the helper from trusted gitpm main and verify its pinned digest
   before execution. Never load it from the candidate or its merge commit.

```bash
curl --fail --silent --show-error --location \
  https://raw.githubusercontent.com/tilr-dev/gitpm/main/scripts/spec_merge.py \
  --output "$RUNNER_TEMP/spec_merge.py"
printf 'HELPER_SHA256  %s\n' "$RUNNER_TEMP/spec_merge.py" | sha256sum --check --strict
python3 "$RUNNER_TEMP/spec_merge.py" \
  --repo "$RUNNER_TEMP/spec-inspection.git" \
  --repository "$REPOSITORY" --pr-number "$PR_NUMBER" \
  --head-sha "$HEAD_SHA" --branch "$TASK_BRANCH"
```

Replace `HELPER_SHA256` with `shasum -a 256 scripts/spec_merge.py` from the
reviewed helper deployed to gitpm main. The workflow's `GH_TOKEN` is used by gh;
the task credential is still supplied only to the authorization checker.

## Regression verification

```sh
python3 -m unittest discover -s scripts -p test_spec_merge.py
```

The JSON matrix injects API responses, fetched parents and virtual time. Tests
cover pending-to-current, stale-to-current, immediate and delayed conflicts,
pending/stale deadlines and a current result. A local bare Git test replaces an
actual synthetic merge ref between polls and checks both resulting refs, with
no network or real waiting. The CLI test verifies the conflict exit status and
message. All three components carry identical helper, fixtures and tests.
