"""Deterministic authorization fixtures plus a real Git/CLI boundary test."""
import contextlib
import io
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import spec_change as checker

FIXTURES = Path(__file__).parent / 'fixtures/spec-change'
SPEC = 'openspec/specs/terminal/spec.md'
ORIGINAL = '## Requirements\n\n### Requirement: Existing\nThe system SHALL work.\n\n#### Scenario: Existing behavior\n- **WHEN** called\n- **THEN** it works\n'


def comment(actor, body, **fields):
    return {'actorUserId': actor, 'body': body, 'kind': 'comment', **fields}


def ready():
    return comment(checker.INTAKE, 'Ready to work.\nSpec scope: own (change add-check per repo)')


def grant(line, actor=checker.DAN):
    return comment(actor, 'spec-change:\n- gitpm: ' + line)


class AuthorizationFixtures(unittest.TestCase):
    def test_requested_matrix(self):
        paths = sorted(FIXTURES.glob('*.json'))
        self.assertEqual(9, len(paths), 'fixture matrix must be present')
        for path in paths:
            case = json.loads(path.read_text())
            with self.subTest(path=path.name):
                result = checker.check(case['base'], case['head'], case['comments'], 'gitpm')
                self.assertEqual(case['uncovered'], result)

    def test_only_trusted_ready_intake_can_enable_legacy_exception(self):
        comments = [ready(), comment('worker', 'Ready\nSpec scope: own (change attacker)')]
        path = 'openspec/changes/attacker/tasks.md'
        self.assertEqual(['gitpm: changes/attacker: edit'], checker.check({}, {path: 'edited'}, comments, 'gitpm'))

    def test_removed_blank_base_line_also_requires_authorization(self):
        self.assertTrue(checker.check({SPEC: ORIGINAL}, {SPEC: ORIGINAL.replace('work.\n\n', 'work.\n')}, [ready()], 'gitpm'))

    def test_operation_and_component_are_exact(self):
        modified = ORIGINAL.replace('it works', 'it fails')
        for line in ['specs/terminal — «Existing»: remove', 'specs/other — «Existing»: modify']:
            with self.subTest(line=line):
                self.assertTrue(checker.check({SPEC: ORIGINAL}, {SPEC: modified}, [ready(), grant(line)], 'gitpm'))
        self.assertTrue(checker.check({SPEC: ORIGINAL}, {SPEC: modified}, [ready(), grant('specs/terminal — «Existing»: modify')], 'tilrd'))

    def test_rename_does_not_allow_arbitrary_removal(self):
        auth = [ready(), grant('specs/terminal — «Existing»: rename')]
        self.assertEqual([], checker.check({SPEC: ORIGINAL}, {SPEC: ORIGINAL.replace('Requirement: Existing', 'Requirement: Renamed')}, auth, 'gitpm'))
        self.assertTrue(checker.check({SPEC: ORIGINAL}, {SPEC: '## Requirements\n'}, auth, 'gitpm'))

    def test_archived_change_edits_need_edit_not_archive(self):
        path = 'openspec/changes/archive/2026-10-20-foreign/tasks.md'
        self.assertEqual([], checker.check({path: 'old'}, {path: 'new'}, [ready(), grant('changes/foreign: edit')], 'gitpm'))
        self.assertTrue(checker.check({path: 'old'}, {path: 'new'}, [ready(), grant('changes/foreign: archive')], 'gitpm'))

    def test_own_modified_delta_still_needs_requirement_grant(self):
        path = 'openspec/changes/add-check/specs/terminal/spec.md'
        base = {SPEC: ORIGINAL}
        head = {**base, path: '## MODIFIED Requirements\n### Requirement: Existing\nThe system SHALL change.\n'}
        self.assertTrue(checker.check(base, head, [ready()], 'gitpm'))
        self.assertEqual([], checker.check(base, head, [ready(), grant('specs/terminal — «Existing»: modify')], 'gitpm'))

    def test_own_archive_is_allowed(self):
        source = 'openspec/changes/add-check/tasks.md'
        target = 'openspec/changes/archive/2026-10-21-add-check/tasks.md'
        self.assertEqual([], checker.check({source: 'done'}, {target: 'done'}, [ready()], 'gitpm'))

    def test_malformed_scope_fails_closed(self):
        for scope in ['???', '', '   ', '\nUnrelated text', '\r\nUnrelated text']:
            for earlier in [[], [ready()]]:
                with self.subTest(scope=scope, earlier=earlier), self.assertRaises(checker.CheckError):
                    checker.check({SPEC: ORIGINAL}, {SPEC: ORIGINAL.replace('it works', 'it fails')},
                                  earlier + [comment(checker.INTAKE, 'Ready\nSpec scope:' + scope)], 'gitpm')


class GitCLIIntegration(unittest.TestCase):
    def test_real_diff_exit_status_and_uncovered_requirement(self):
        with tempfile.TemporaryDirectory() as directory:
            repo = Path(directory)
            def git(*args):
                return subprocess.check_output(['git', '-C', directory, *args], stderr=subprocess.DEVNULL).decode().strip()
            git('init', '-q')
            git('config', 'user.name', 'Fixture')
            git('config', 'user.email', 'fixture@example.invalid')
            p = repo / SPEC
            p.parent.mkdir(parents=True)
            p.write_text(ORIGINAL)
            git('add', '.')
            git('commit', '-qm', 'base')
            base = git('rev-parse', 'HEAD')
            p.write_text(ORIGINAL.replace('it works', 'it changes'))
            git('commit', '-qam', 'candidate')
            comments = repo / 'comments.json'
            comments.write_text(json.dumps([ready()]))
            args = ['--repo', directory, '--base', base, '--head', 'HEAD', '--branch', 'task/TILR-123', '--component', 'gitpm', '--comments-json', str(comments)]
            output = io.StringIO()
            with contextlib.redirect_stdout(output):
                self.assertEqual(1, checker.main(args))
            self.assertIn('specs/terminal — «Existing»: modify', output.getvalue())
            comments.write_text(json.dumps([ready(), grant('specs/terminal — «Existing»: modify')]))
            with contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(0, checker.main(args))


class MCPContract(unittest.TestCase):
    def test_sse_and_session_are_decoded_without_live_credentials(self):
        class Response:
            def __init__(self, payload, session=None):
                self.headers = {'Content-Type': 'text/event-stream'}
                if session:
                    self.headers['Mcp-Session-Id'] = session
                self.payload = payload
            def __enter__(self):
                return self
            def __exit__(self, *args):
                pass
            def read(self):
                return self.payload.encode()
        responses = [Response('event: message\ndata: {"jsonrpc":"2.0","id":1,"result":{}}\n\n', 'session'), Response(''), Response('data: {"jsonrpc":"2.0","id":3,"result":{"tools":[]}}\n\n')]
        with patch('urllib.request.urlopen', side_effect=responses) as request:
            client = checker.MCP('fixture-credential')
        self.assertEqual('session', client.session)
        self.assertEqual('session', request.call_args_list[2].args[0].get_header('Mcp-session-id'))
        for call in request.call_args_list:
            self.assertEqual(checker.USER_AGENT, call.args[0].get_header('User-agent'))

    def test_comment_pagination_is_complete_and_ordered(self):
        client = checker.MCP.__new__(checker.MCP)
        client.schemas = {
            'get_issue_by_number': {'properties': {'number': {}}},
            'list_issue_activity': {'properties': {'limit': {'maximum': 2}, 'offset': {}}},
        }
        rows = [comment(checker.INTAKE, 'Ready', id=1), ready(), grant('changes/foreign: edit')]
        with patch.object(client, 'call', side_effect=[{'id': 250}, rows[:2], rows[2:]]) as call:
            self.assertEqual(3, len(client.comments(245)))
            self.assertEqual(3, call.call_count)

    def test_truncated_comments_fail_closed(self):
        client = checker.MCP.__new__(checker.MCP)
        client.schemas = {
            'get_issue_by_number': {'properties': {'number': {}}},
            'list_issue_activity': {'properties': {'limit': {'maximum': 1}}},
        }
        with patch.object(client, 'call', side_effect=[{'id': 250}, [ready()]]):
            with self.assertRaisesRegex(checker.CheckError, 'complete'):
                client.comments(245)


if __name__ == '__main__':
    unittest.main()
