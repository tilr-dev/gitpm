"""Prepare trusted specification refs without checking out candidate code.

Deploy this helper from main with a content digest pinned in the trusted workflow.
The caller initializes the bare repository, its origin and Git authentication.
"""
import argparse
import json
import subprocess
import sys
import time


def prepare_merge(repo, repository, pr_number, head_sha, branch, *,
                  run=subprocess.run, clock=time.monotonic, wait=time.sleep):
    """Return the verified base SHA; injected boundaries keep tests deterministic."""
    deadline = clock() + 60

    def command(args):
        return run(args, check=True, text=True, capture_output=True, timeout=15).stdout

    def git(*args):
        return command(['git', '-C', str(repo), *args])

    while True:
        pr = json.loads(command(['gh', 'api', f'repos/{repository}/pulls/{pr_number}']))
        if 'mergeable' not in pr or not any(pr['mergeable'] is v for v in (None, True, False)):
            raise RuntimeError('spec-change: invalid mergeable value in GitHub PR response')
        if pr['mergeable'] is False:
            raise RuntimeError(
                f'spec-change: PR conflicts with dev: merge dev into {branch} (`git merge dev`) and push')
        if pr['mergeable'] is True:
            # GitHub replaces the synthetic merge; refreshing it is not fast-forward.
            git('fetch', '--no-tags', '--depth=2', 'origin',
                f'+refs/pull/{pr_number}/merge:refs/inspection/merge')
            parents = [line[7:] for line in git('cat-file', '-p', 'refs/inspection/merge').splitlines()
                       if line.startswith('parent ')]
            if len(parents) == 2 and parents[1] == head_sha:
                git('update-ref', 'refs/inspection/base', parents[0])
                return parents[0]
        remaining = deadline - clock()
        if remaining <= 0:
            raise RuntimeError(
                f'spec-change: GitHub has not updated the merge result for {head_sha} yet; re-run the job')
        wait(min(2, remaining))


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('repo', 'repository', 'pr-number', 'head-sha', 'branch'):
        parser.add_argument('--' + name, required=True)
    args = parser.parse_args(argv)
    try:
        prepare_merge(args.repo, args.repository, args.pr_number, args.head_sha, args.branch)
    except (RuntimeError, ValueError, OSError, subprocess.SubprocessError) as error:
        print(str(error), file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
