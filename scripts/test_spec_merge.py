"""Deterministic API/Git boundary fixtures; no wall-clock waiting or network."""
import contextlib
import io
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import spec_merge

FIXTURES = Path(__file__).parent / 'fixtures/spec-merge'


class FixtureRunner:
    def __init__(self, case):
        self.case = case
        self.now = 0
        self.waits = []
        self.calls = []
        self.poll = -1
        self.base = None

    def clock(self):
        return self.now

    def wait(self, seconds):
        self.waits.append(seconds)
        self.now += seconds

    def run(self, args, **kwargs):
        self.calls.append(args)
        if args[0] == 'gh':
            assert args == ['gh', 'api', 'repos/owner/repo/pulls/69']
            self.poll += 1
            output = json.dumps(self.case['polls'][min(self.poll, len(self.case['polls']) - 1)])
        elif args[3] == 'cat-file':
            state = self.case['polls'][max(0, min(self.poll, len(self.case['polls']) - 1))]
            output = 'tree ignored\n' + ''.join('parent ' + p + '\n' for p in state.get('parents', ['base', 'old-head']))
        elif args[3] == 'update-ref':
            self.base = args[-1]
            output = ''
        elif args[3] == 'fetch':
            output = ''
        else:
            raise AssertionError(args)
        return subprocess.CompletedProcess(args, 0, output, '')


class MergeReadinessFixtures(unittest.TestCase):
    def test_outcomes_and_wait_deadline(self):
        paths = sorted(FIXTURES.glob('*.json'))
        self.assertEqual(7, len(paths))
        for path in paths:
            case = json.loads(path.read_text())
            runner = FixtureRunner(case)
            with self.subTest(case=path.stem):
                kwargs = dict(run=runner.run, clock=runner.clock, wait=runner.wait)
                if 'error' in case:
                    with self.assertRaises(RuntimeError) as error:
                        spec_merge.prepare_merge('bare.git', 'owner/repo', '69', 'candidate', 'task/example', **kwargs)
                    self.assertEqual('spec-change: ' + case['error'], str(error.exception))
                    self.assertIsNone(runner.base)
                else:
                    self.assertEqual(case['outcome'], spec_merge.prepare_merge(
                        'bare.git', 'owner/repo', '69', 'candidate', 'task/example', **kwargs))
                    self.assertEqual(case['outcome'], runner.base)
                self.assertEqual(case['waits'], runner.waits)
                self.assertEqual('gh', runner.calls[0][0])
                if path.stem == 'conflict':
                    self.assertEqual(1, len(runner.calls), 'conflict must fail before fetching stale refs')
                if path.stem == 'stale-current':
                    fetches = [c for c in runner.calls if c[0] == 'git' and c[3] == 'fetch']
                    self.assertEqual(2, len(fetches))
                    self.assertTrue(all(c[-1].startswith('+refs/pull/') for c in fetches), 'ref refresh must allow non-fast-forward updates')


    def test_real_git_refreshes_replaced_merge_and_sets_its_base(self):
        with tempfile.TemporaryDirectory() as tmp:
            origin = Path(tmp) / 'origin.git'
            inspection = Path(tmp) / 'inspection.git'

            def git(repo, *args, input=None):
                return subprocess.run(['git', '-C', str(repo), *args], input=input,
                                      text=True, check=True, capture_output=True).stdout.strip()

            subprocess.run(['git', 'init', '--bare', str(origin)], check=True, capture_output=True)
            subprocess.run(['git', 'init', '--bare', str(inspection)], check=True, capture_output=True)
            git(inspection, 'remote', 'add', 'origin', str(origin))
            tree = git(origin, 'mktree', input='')

            def commit(message, *parents):
                args = ['-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.test',
                        'commit-tree', tree, '-m', message]
                for parent in parents:
                    args.extend(['-p', parent])
                return git(origin, *args)

            base = commit('base')
            old_head = commit('old head', base)
            head = commit('candidate', old_head)
            old_merge = commit('old merge', base, old_head)
            merge = commit('current merge', base, head)
            git(origin, 'update-ref', 'refs/pull/69/merge', old_merge)
            runner = FixtureRunner({'polls': [{'mergeable': True}]})
            fetch_count = 0

            def run(args, **kwargs):
                nonlocal fetch_count
                if args[0] == 'gh':
                    return runner.run(args, **kwargs)
                if args[3] == 'fetch':
                    fetch_count += 1
                    if fetch_count == 2:
                        git(origin, 'update-ref', 'refs/pull/69/merge', merge)
                return subprocess.run(args, **kwargs)

            self.assertEqual(base, spec_merge.prepare_merge(
                inspection, 'owner/repo', '69', head, 'task/example',
                run=run, clock=runner.clock, wait=runner.wait))
            self.assertEqual([2], runner.waits)
            self.assertEqual(merge, git(inspection, 'rev-parse', 'refs/inspection/merge'))
            self.assertEqual(base, git(inspection, 'rev-parse', 'refs/inspection/base'))

    def test_invalid_api_response_fails_without_fetch_or_wait(self):
        for value in ({}, {'mergeable': 'false'}, {'mergeable': 0}):
            runner = FixtureRunner({'polls': [value]})
            with self.subTest(value=value), self.assertRaisesRegex(RuntimeError, 'invalid mergeable'):
                spec_merge.prepare_merge('bare.git', 'owner/repo', '69', 'candidate', 'task/example',
                                         run=runner.run, clock=runner.clock, wait=runner.wait)
            self.assertEqual([], runner.waits)
            self.assertEqual(1, len(runner.calls))

    def test_cli_reports_conflict_and_returns_failure(self):
        runner = FixtureRunner({'polls': [{'mergeable': False}]})
        stderr = io.StringIO()
        prepare = spec_merge.prepare_merge
        with patch.object(spec_merge, 'prepare_merge', side_effect=lambda *args: prepare(
                *args, run=runner.run, clock=runner.clock, wait=runner.wait)), contextlib.redirect_stderr(stderr):
            status = spec_merge.main(['--repo', 'bare.git', '--repository', 'owner/repo',
                                      '--pr-number', '69', '--head-sha', 'candidate', '--branch', 'task/example'])
        self.assertEqual(1, status)
        self.assertIn('PR conflicts with dev', stderr.getvalue())
        self.assertIn('git merge dev', stderr.getvalue())
        self.assertNotIn('re-run', stderr.getvalue())


if __name__ == '__main__':
    unittest.main()
