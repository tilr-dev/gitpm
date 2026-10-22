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

    def test_insertions_into_existing_requirement_need_modify(self):
        additions = [
            ORIGINAL.replace('work.\n', 'work.\nThe check MAY be skipped by any user.\n'),
            ORIGINAL + '- **AND** nothing is enforced\n',
            ORIGINAL + '\n#### Scenario: Bypass\n- **THEN** skip enforcement\n',
        ]
        for head in additions:
            with self.subTest(head=head):
                self.assertEqual(['gitpm: specs/terminal — «Existing»: modify'],
                                 checker.check({SPEC: ORIGINAL}, {SPEC: head}, [ready()], 'gitpm'))
                self.assertEqual([], checker.check({SPEC: ORIGINAL}, {SPEC: head},
                                 [ready(), grant('specs/terminal — «Existing»: modify')], 'gitpm'))

    def test_new_requirement_cannot_take_existing_scenario(self):
        scenario = '\n#### Scenario: Second behavior\n- **WHEN** called again\n- **THEN** it still works\n'
        old = ORIGINAL + scenario
        new = ORIGINAL + '\n### Requirement: New\nThe system SHALL add behavior.\n' + scenario
        self.assertEqual(['gitpm: specs/terminal — «Existing»: modify'],
                         checker.check({SPEC: old}, {SPEC: new}, [ready()], 'gitpm'))
        self.assertEqual([], checker.check({SPEC: old}, {SPEC: new},
                         [ready(), grant('specs/terminal — «Existing»: modify')], 'gitpm'))

    def test_new_requirements_are_free_at_each_boundary(self):
        added = '### Requirement: New\nThe system SHALL add behavior.\n\n'
        for head in [added + ORIGINAL, ORIGINAL + '\n' + added,
                     ORIGINAL.replace('### Requirement: Existing', added + '### Requirement: Existing'),
                     ORIGINAL + '\n\n']:
            with self.subTest(head=head):
                self.assertEqual([], checker.check({SPEC: ORIGINAL}, {SPEC: head}, [ready()], 'gitpm'))

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

    def test_renamed_modified_delta_requires_original_requirement_modify(self):
        path = 'openspec/changes/add-check/specs/terminal/spec.md'
        delta = ('## RENAMED Requirements\n'
                 '- FROM: `### Requirement: Existing`\n'
                 '- TO: `### Requirement: Renamed`\n'
                 '## MODIFIED Requirements\n'
                 '### Requirement: Renamed\nThe system SHALL change.\n')
        base = {SPEC: ORIGINAL}
        auth = [ready(), grant('specs/terminal — «Existing»: rename')]
        self.assertEqual(['gitpm: specs/terminal — «Existing»: modify (delta add-check)'],
                         checker.check(base, {**base, path: delta}, auth, 'gitpm'))
        self.assertEqual([], checker.check(base, {**base, path: delta},
                                          auth + [grant('specs/terminal — «Existing»: modify')], 'gitpm'))

    def test_chained_renames_require_original_authorization(self):
        path = 'openspec/changes/add-check/specs/terminal/spec.md'
        renames = ('## RENAMED Requirements\n'
                   '- FROM: `### Requirement: Existing`\n'
                   '- TO: `### Requirement: Intermediate`\n'
                   '- FROM: `### Requirement: Intermediate`\n'
                   '- TO: `### Requirement: Final`\n')
        base = {SPEC: ORIGINAL}
        auth = [ready(), grant('specs/terminal — «Existing»: rename')]
        self.assertEqual([], checker.check(base, {**base, path: renames}, auth, 'gitpm'))
        for section, operation in [('MODIFIED', 'modify'), ('REMOVED', 'remove')]:
            delta = renames + f'## {section} Requirements\n### Requirement: Final\nThe system SHALL change.\n'
            with self.subTest(operation=operation):
                self.assertEqual([f'gitpm: specs/terminal — «Existing»: {operation} (delta add-check)'],
                                 checker.check(base, {**base, path: delta}, auth, 'gitpm'))
                self.assertEqual([], checker.check(base, {**base, path: delta},
                                 auth + [grant(f'specs/terminal — «Existing»: {operation}')], 'gitpm'))

    def test_reused_rename_source_preserves_requirement_identity(self):
        path = 'openspec/changes/add-check/specs/terminal/spec.md'
        base = {SPEC: ORIGINAL + '\n### Requirement: Other\nThe system SHALL do other work.\n'}
        renames = ('## RENAMED Requirements\n'
                   '- FROM: `### Requirement: Existing`\n'
                   '- TO: `### Requirement: Final`\n'
                   '- FROM: `### Requirement: Other`\n'
                   '- TO: `### Requirement: Existing`\n')
        auth = [ready(), grant('specs/terminal — «Other»: rename'),
                grant('specs/terminal — «Other»: modify')]
        for section, operation in [('MODIFIED', 'modify'), ('REMOVED', 'remove')]:
            delta = renames + f'## {section} Requirements\n### Requirement: Final\nThe system SHALL change.\n'
            with self.subTest(operation=operation):
                self.assertEqual([
                    f'gitpm: specs/terminal — «Existing»: {operation} (delta add-check)',
                    'gitpm: specs/terminal — «Existing»: rename (delta add-check)',
                ], checker.check(base, {**base, path: delta}, auth, 'gitpm'))
                self.assertEqual([], checker.check(base, {**base, path: delta}, auth + [
                    grant('specs/terminal — «Existing»: rename'),
                    grant(f'specs/terminal — «Existing»: {operation}')], 'gitpm'))

    def test_invalid_rename_graph_fails_closed(self):
        for pairs in [[('Existing', 'Next'), ('Next', 'Existing')],
                      [('Existing', 'Final'), ('Other', 'Final')]]:
            delta = '## RENAMED Requirements\n' + ''.join(
                f'- FROM: `### Requirement: {old}`\n- TO: `### Requirement: {new}`\n'
                for old, new in pairs)
            with self.subTest(pairs=pairs), self.assertRaises(checker.CheckError):
                checker.check({SPEC: ORIGINAL},
                              {SPEC: ORIGINAL, 'openspec/changes/add-check/specs/terminal/spec.md': delta},
                              [ready(), grant('specs/terminal — «Existing»: rename')], 'gitpm')

    def test_foreign_archive_rewrite_requires_edit_as_well(self):
        source = 'openspec/changes/foreign/tasks.md'
        target = 'openspec/changes/archive/2026-10-22-foreign/tasks.md'
        base = {source: 'original'}
        auth = [ready(), grant('changes/foreign: archive')]
        self.assertEqual([], checker.check(base, {target: 'original'}, auth, 'gitpm'))
        self.assertEqual(['gitpm: changes/foreign: edit'],
                         checker.check(base, {target: 'rewritten'}, auth, 'gitpm'))
        self.assertEqual([], checker.check(base, {target: 'rewritten'},
                                          auth + [grant('changes/foreign: edit')], 'gitpm'))

    def test_foreign_archive_with_modified_surviving_source_needs_edit(self):
        source = 'openspec/changes/foreign/tasks.md'
        target = 'openspec/changes/archive/2026-10-22-foreign/tasks.md'
        base = {source: 'original'}
        head = {source: 'rewritten', target: 'original'}
        auth = [ready(), grant('changes/foreign: archive')]
        self.assertEqual(['gitpm: changes/foreign: edit'], checker.check(base, head, auth, 'gitpm'))
        self.assertEqual([], checker.check(base, head, auth + [grant('changes/foreign: edit')], 'gitpm'))
        self.assertEqual([], checker.check(base, {source: 'original', target: 'original'}, auth, 'gitpm'))

    def test_new_archive_does_not_authorize_rewriting_existing_archive(self):
        source = 'openspec/changes/foreign/tasks.md'
        previous = 'openspec/changes/archive/2026-10-20-foreign/tasks.md'
        target = 'openspec/changes/archive/2026-10-22-foreign/tasks.md'
        base = {source: 'original', previous: 'previous archive'}
        auth = [ready(), grant('changes/foreign: archive')]
        for archived in ['unauthorized rewrite', None]:
            head = {source: 'original', target: 'original'}
            if archived is not None:
                head[previous] = archived
            with self.subTest(archived=archived):
                self.assertEqual(['gitpm: changes/foreign: edit'], checker.check(base, head, auth, 'gitpm'))
                self.assertEqual([], checker.check(base, head, auth + [grant('changes/foreign: edit')], 'gitpm'))

    def test_foreign_archive_new_file_requires_edit_as_well(self):
        source = 'openspec/changes/foreign/tasks.md'
        target = 'openspec/changes/archive/2026-10-22-foreign/tasks.md'
        added = 'openspec/changes/archive/2026-10-22-foreign/design.md'
        base = {source: 'original'}
        head = {target: 'original', added: 'new design'}
        auth = [ready(), grant('changes/foreign: archive')]
        self.assertEqual(['gitpm: changes/foreign: edit'], checker.check(base, head, auth, 'gitpm'))
        self.assertEqual([], checker.check(base, head, auth + [grant('changes/foreign: edit')], 'gitpm'))

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

    def test_heading_only_removal_needs_modify_for_absorbing_requirement(self):
        removed = '\n### Requirement: Removed\nThe system SHALL also work.\n\n#### Scenario: Removed behavior\n- **WHEN** called later\n- **THEN** it also works\n'
        base = {SPEC: ORIGINAL + removed}
        head = {SPEC: ORIGINAL + removed.replace('### Requirement: Removed\n', '')}
        auth = [ready(), grant('specs/terminal — «Removed»: remove')]
        self.assertIn('gitpm: specs/terminal — «Existing»: modify', checker.check(base, head, auth, 'gitpm'))
        self.assertEqual([], checker.check(base, head, auth + [grant('specs/terminal — «Existing»: modify')], 'gitpm'))

    def test_scope_own_change_is_parsed_only_inside_own(self):
        foreign = 'openspec/changes/by/tasks.md'
        scope = 'Ready\nSpec scope: own (ADDED; change add-spec-change-check per repo); spec-change by Dan: https://plan.example/c/1'
        comments = [comment(checker.INTAKE, scope)]
        self.assertEqual({'add-spec-change-check'}, checker.trusted_scope(comments))
        self.assertEqual(['gitpm: changes/by: edit'], checker.check({}, {foreign: 'edited'}, comments, 'gitpm'))

    def test_scope_with_change_outside_own_or_two_ids_fails_closed(self):
        for scope in ['own (ADDED; change add-check) and must not change foreign-change',
                      'own (change add-check, change other-check)',
                      'own (ADDED to terminal)',
                      'change add-check']:
            with self.subTest(scope=scope), self.assertRaises(checker.CheckError):
                checker.trusted_scope([comment(checker.INTAKE, 'Ready\nSpec scope: ' + scope)])

    def test_existing_archive_with_own_name_needs_edit(self):
        path = 'openspec/changes/archive/2026-01-01-add-check/tasks.md'
        self.assertEqual(['gitpm: changes/add-check: edit'],
                         checker.check({path: 'old'}, {path: 'rewritten'}, [ready()], 'gitpm'))
        self.assertEqual([], checker.check({path: 'old'}, {path: 'rewritten'},
                                          [ready(), grant('changes/add-check: edit')], 'gitpm'))

    def test_capability_colliding_by_case_or_unicode_form_fails_closed(self):
        new_spec = '## Requirements\n\n### Requirement: New\nThe system SHALL add behavior.\n'
        cafe_nfc, cafe_nfd = 'openspec/specs/café/spec.md', 'openspec/specs/café/spec.md'
        for base, added in [({SPEC: ORIGINAL}, {'openspec/specs/Terminal/spec.md': new_spec}),
                            ({cafe_nfc: ORIGINAL}, {cafe_nfd: new_spec}),
                            ({}, {'openspec/specs/new/spec.md': new_spec, 'openspec/specs/NEW/spec.md': new_spec})]:
            with self.subTest(added=sorted(added)), self.assertRaises(checker.CheckError):
                checker.check(base, {**base, **added}, [ready()], 'gitpm')
        base = {SPEC: ORIGINAL}
        self.assertEqual([], checker.check(base, {**base, 'openspec/specs/terminal-2/spec.md': new_spec}, [ready()], 'gitpm'))


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
