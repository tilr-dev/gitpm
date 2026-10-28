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


def task(**fields):
    return {'id': 250, 'sequenceNumber': 123, 'createdAt': '2026-10-20T00:00:00Z',
            'columnId': 21, **fields}


def export(comments, **fields):
    return {'issue': task(**fields), 'comments': comments}


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
                result = checker.check(case['base'], case['head'], case['comments'], 'gitpm', issue=task())
                self.assertEqual(case['uncovered'], result)

    def test_only_trusted_ready_intake_can_enable_legacy_exception(self):
        comments = [ready(), comment('worker', 'Ready\nSpec scope: own (change attacker)')]
        path = 'openspec/changes/attacker/tasks.md'
        self.assertEqual(['gitpm: changes/attacker: edit'], checker.check({}, {path: 'edited'}, comments, 'gitpm', issue=task()))

    def test_removed_blank_base_line_also_requires_authorization(self):
        self.assertTrue(checker.check({SPEC: ORIGINAL}, {SPEC: ORIGINAL.replace('work.\n\n', 'work.\n')}, [ready()], 'gitpm', issue=task()))

    def test_insertions_into_existing_requirement_need_modify(self):
        additions = [
            ORIGINAL.replace('work.\n', 'work.\nThe check MAY be skipped by any user.\n'),
            ORIGINAL + '- **AND** nothing is enforced\n',
            ORIGINAL + '\n#### Scenario: Bypass\n- **THEN** skip enforcement\n',
        ]
        for head in additions:
            with self.subTest(head=head):
                self.assertEqual(['gitpm: specs/terminal — «Existing»: modify'],
                                 checker.check({SPEC: ORIGINAL}, {SPEC: head}, [ready()], 'gitpm', issue=task()))
                self.assertEqual([], checker.check({SPEC: ORIGINAL}, {SPEC: head},
                                 [ready(), grant('specs/terminal — «Existing»: modify')], 'gitpm', issue=task()))

    def test_new_requirement_cannot_take_existing_scenario(self):
        scenario = '\n#### Scenario: Second behavior\n- **WHEN** called again\n- **THEN** it still works\n'
        old = ORIGINAL + scenario
        new = ORIGINAL + '\n### Requirement: New\nThe system SHALL add behavior.\n' + scenario
        self.assertEqual(['gitpm: specs/terminal — «Existing»: modify'],
                         checker.check({SPEC: old}, {SPEC: new}, [ready()], 'gitpm', issue=task()))
        self.assertEqual([], checker.check({SPEC: old}, {SPEC: new},
                         [ready(), grant('specs/terminal — «Existing»: modify')], 'gitpm', issue=task()))

    def test_new_requirements_are_free_at_each_boundary(self):
        added = '### Requirement: New\nThe system SHALL add behavior.\n\n'
        for head in [added + ORIGINAL, ORIGINAL + '\n' + added,
                     ORIGINAL.replace('### Requirement: Existing', added + '### Requirement: Existing'),
                     ORIGINAL + '\n\n']:
            with self.subTest(head=head):
                self.assertEqual([], checker.check({SPEC: ORIGINAL}, {SPEC: head}, [ready()], 'gitpm', issue=task()))

    def test_added_requirement_before_trailing_section_is_free(self):
        added = '### Requirement: New\nThe system SHALL add behavior.\n\n'
        for heading in ['# Notes', '## Notes', '### Notes']:
            with self.subTest(heading=heading):
                trailing = heading + '\nUnchanged notes.\n'
                old = ORIGINAL + '\n' + trailing
                new = ORIGINAL + '\n' + added + trailing
                self.assertEqual(set(), checker.changed_requirements(old, new))
                self.assertEqual([], checker.check({SPEC: old}, {SPEC: new}, [ready()],
                                                  'gitpm', issue=task()))

    def test_requirement_owner_ends_at_peer_or_parent_heading(self):
        for heading in ['# Notes', '## Notes', '### Notes']:
            with self.subTest(heading=heading):
                lines = (ORIGINAL + heading + '\nNotes.\n'
                         '### Requirement: Next\nNext body.\n').splitlines()
                boundary = lines.index(heading)
                self.assertEqual('Existing', checker.requirement_at(lines, boundary - 1))
                self.assertIsNone(checker.requirement_at(lines, boundary))
                self.assertIsNone(checker.requirement_at(lines, boundary + 1))
                self.assertEqual('Next', checker.requirement_at(lines, len(lines) - 1))

    def test_operation_and_component_are_exact(self):
        modified = ORIGINAL.replace('it works', 'it fails')
        for line in ['specs/terminal — «Existing»: remove', 'specs/other — «Existing»: modify']:
            with self.subTest(line=line):
                self.assertTrue(checker.check({SPEC: ORIGINAL}, {SPEC: modified}, [ready(), grant(line)], 'gitpm', issue=task()))
        self.assertTrue(checker.check({SPEC: ORIGINAL}, {SPEC: modified}, [ready(), grant('specs/terminal — «Existing»: modify')], 'tilrd', issue=task()))

    def test_rename_does_not_allow_arbitrary_removal(self):
        auth = [ready(), grant('specs/terminal — «Existing»: rename')]
        self.assertEqual([], checker.check({SPEC: ORIGINAL}, {SPEC: ORIGINAL.replace('Requirement: Existing', 'Requirement: Renamed')}, auth, 'gitpm', issue=task()))
        self.assertTrue(checker.check({SPEC: ORIGINAL}, {SPEC: '## Requirements\n'}, auth, 'gitpm', issue=task()))

    def test_archived_change_edits_need_edit_not_archive(self):
        path = 'openspec/changes/archive/2026-10-20-foreign/tasks.md'
        self.assertEqual([], checker.check({path: 'old'}, {path: 'new'}, [ready(), grant('changes/foreign: edit')], 'gitpm', issue=task()))
        self.assertTrue(checker.check({path: 'old'}, {path: 'new'}, [ready(), grant('changes/foreign: archive')], 'gitpm', issue=task()))

    def test_own_modified_delta_still_needs_requirement_grant(self):
        path = 'openspec/changes/add-check/specs/terminal/spec.md'
        base = {SPEC: ORIGINAL}
        head = {**base, path: '## MODIFIED Requirements\n### Requirement: Existing\nThe system SHALL change.\n'}
        self.assertTrue(checker.check(base, head, [ready()], 'gitpm', issue=task()))
        self.assertEqual([], checker.check(base, head, [ready(), grant('specs/terminal — «Existing»: modify')], 'gitpm', issue=task()))

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
                         checker.check(base, {**base, path: delta}, auth, 'gitpm', issue=task()))
        self.assertEqual([], checker.check(base, {**base, path: delta},
                                          auth + [grant('specs/terminal — «Existing»: modify')], 'gitpm', issue=task()))

    def test_chained_renames_require_original_authorization(self):
        path = 'openspec/changes/add-check/specs/terminal/spec.md'
        renames = ('## RENAMED Requirements\n'
                   '- FROM: `### Requirement: Existing`\n'
                   '- TO: `### Requirement: Intermediate`\n'
                   '- FROM: `### Requirement: Intermediate`\n'
                   '- TO: `### Requirement: Final`\n')
        base = {SPEC: ORIGINAL}
        auth = [ready(), grant('specs/terminal — «Existing»: rename')]
        self.assertEqual([], checker.check(base, {**base, path: renames}, auth, 'gitpm', issue=task()))
        for section, operation in [('MODIFIED', 'modify'), ('REMOVED', 'remove')]:
            delta = renames + f'## {section} Requirements\n### Requirement: Final\nThe system SHALL change.\n'
            with self.subTest(operation=operation):
                self.assertEqual([f'gitpm: specs/terminal — «Existing»: {operation} (delta add-check)'],
                                 checker.check(base, {**base, path: delta}, auth, 'gitpm', issue=task()))
                self.assertEqual([], checker.check(base, {**base, path: delta},
                                 auth + [grant(f'specs/terminal — «Existing»: {operation}')], 'gitpm', issue=task()))

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
                ], checker.check(base, {**base, path: delta}, auth, 'gitpm', issue=task()))
                self.assertEqual([], checker.check(base, {**base, path: delta}, auth + [
                    grant('specs/terminal — «Existing»: rename'),
                    grant(f'specs/terminal — «Existing»: {operation}')], 'gitpm', issue=task()))

    def test_invalid_rename_graph_fails_closed(self):
        for pairs in [[('Existing', 'Next'), ('Next', 'Existing')],
                      [('Existing', 'Final'), ('Other', 'Final')]]:
            delta = '## RENAMED Requirements\n' + ''.join(
                f'- FROM: `### Requirement: {old}`\n- TO: `### Requirement: {new}`\n'
                for old, new in pairs)
            with self.subTest(pairs=pairs), self.assertRaises(checker.CheckError):
                checker.check({SPEC: ORIGINAL},
                              {SPEC: ORIGINAL, 'openspec/changes/add-check/specs/terminal/spec.md': delta},
                              [ready(), grant('specs/terminal — «Existing»: rename')], 'gitpm', issue=task())

    def test_foreign_archive_rewrite_requires_edit_as_well(self):
        source = 'openspec/changes/foreign/tasks.md'
        target = 'openspec/changes/archive/2026-10-22-foreign/tasks.md'
        base = {source: 'original'}
        auth = [ready(), grant('changes/foreign: archive')]
        self.assertEqual([], checker.check(base, {target: 'original'}, auth, 'gitpm', issue=task()))
        self.assertEqual(['gitpm: changes/foreign: edit'],
                         checker.check(base, {target: 'rewritten'}, auth, 'gitpm', issue=task()))
        self.assertEqual([], checker.check(base, {target: 'rewritten'},
                                          auth + [grant('changes/foreign: edit')], 'gitpm', issue=task()))

    def test_foreign_archive_with_modified_surviving_source_needs_edit(self):
        source = 'openspec/changes/foreign/tasks.md'
        target = 'openspec/changes/archive/2026-10-22-foreign/tasks.md'
        base = {source: 'original'}
        head = {source: 'rewritten', target: 'original'}
        auth = [ready(), grant('changes/foreign: archive')]
        self.assertEqual(['gitpm: changes/foreign: edit'], checker.check(base, head, auth, 'gitpm', issue=task()))
        self.assertEqual([], checker.check(base, head, auth + [grant('changes/foreign: edit')], 'gitpm', issue=task()))
        self.assertEqual([], checker.check(base, {source: 'original', target: 'original'}, auth, 'gitpm', issue=task()))

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
                self.assertEqual(['gitpm: changes/foreign: edit'], checker.check(base, head, auth, 'gitpm', issue=task()))
                self.assertEqual([], checker.check(base, head, auth + [grant('changes/foreign: edit')], 'gitpm', issue=task()))

    def test_foreign_archive_new_file_requires_edit_as_well(self):
        source = 'openspec/changes/foreign/tasks.md'
        target = 'openspec/changes/archive/2026-10-22-foreign/tasks.md'
        added = 'openspec/changes/archive/2026-10-22-foreign/design.md'
        base = {source: 'original'}
        head = {target: 'original', added: 'new design'}
        auth = [ready(), grant('changes/foreign: archive')]
        self.assertEqual(['gitpm: changes/foreign: edit'], checker.check(base, head, auth, 'gitpm', issue=task()))
        self.assertEqual([], checker.check(base, head, auth + [grant('changes/foreign: edit')], 'gitpm', issue=task()))

    def test_own_archive_is_allowed(self):
        source = 'openspec/changes/add-check/tasks.md'
        target = 'openspec/changes/archive/2026-10-21-add-check/tasks.md'
        self.assertEqual([], checker.check({source: 'done'}, {target: 'done'}, [ready()], 'gitpm', issue=task()))

    def test_malformed_scope_fails_closed(self):
        for scope in ['???', '', '   ', '\nUnrelated text', '\r\nUnrelated text']:
            for earlier in [[], [ready()]]:
                with self.subTest(scope=scope, earlier=earlier), self.assertRaises(checker.CheckError):
                    checker.check({SPEC: ORIGINAL}, {SPEC: ORIGINAL.replace('it works', 'it fails')},
                                  earlier + [comment(checker.INTAKE, 'Ready\nSpec scope:' + scope)], 'gitpm', issue=task())

    def test_heading_only_removal_needs_modify_for_absorbing_requirement(self):
        removed = '\n### Requirement: Removed\nThe system SHALL also work.\n\n#### Scenario: Removed behavior\n- **WHEN** called later\n- **THEN** it also works\n'
        base = {SPEC: ORIGINAL + removed}
        head = {SPEC: ORIGINAL + removed.replace('### Requirement: Removed\n', '')}
        auth = [ready(), grant('specs/terminal — «Removed»: remove')]
        self.assertIn('gitpm: specs/terminal — «Existing»: modify', checker.check(base, head, auth, 'gitpm', issue=task()))
        self.assertEqual([], checker.check(base, head, auth + [grant('specs/terminal — «Existing»: modify')], 'gitpm', issue=task()))

    def test_heading_moved_below_scenario_needs_modify_for_absorbing_requirement(self):
        moved = '\n#### Scenario: Moved behavior\n- **WHEN** called later\n- **THEN** it also works\n'
        rest = 'The system SHALL also work.\n\n#### Scenario: Kept behavior\n- **WHEN** called last\n- **THEN** it still works\n'
        base = {SPEC: ORIGINAL + '\n### Requirement: Moved\n' + rest.replace('work.\n', 'work.\n' + moved, 1)}
        head = {SPEC: ORIGINAL + moved + '\n### Requirement: Moved\n' + rest}
        auth = [ready(), grant('specs/terminal — «Moved»: modify')]
        self.assertIn('gitpm: specs/terminal — «Existing»: modify', checker.check(base, head, auth, 'gitpm', issue=task()))
        self.assertEqual([], checker.check(base, head, auth + [grant('specs/terminal — «Existing»: modify')], 'gitpm', issue=task()))

    def test_text_moved_between_surviving_requirements_needs_modify_for_both(self):
        # Other's heading moves up, so Existing's scenario falls under Other while both survive.
        base = {SPEC: ORIGINAL + '\n### Requirement: Other\nThe system SHALL do other work.\n'}
        head = {SPEC: ORIGINAL.replace('#### Scenario', '### Requirement: Other\n#### Scenario')
                + '\nThe system SHALL do other work.\n'}
        self.assertEqual(['gitpm: specs/terminal — «Existing»: modify', 'gitpm: specs/terminal — «Other»: modify'],
                         checker.check(base, head, [ready()], 'gitpm', issue=task()))
        self.assertEqual([], checker.check(base, head, [ready(), grant('specs/terminal — «Existing»: modify'),
                                                        grant('specs/terminal — «Other»: modify')], 'gitpm', issue=task()))

    def test_scope_own_change_is_parsed_only_inside_own(self):
        foreign = 'openspec/changes/by/tasks.md'
        scope = 'Ready\nSpec scope: own (ADDED; change add-spec-change-check per repo); spec-change by Dan: https://plan.example/c/1'
        comments = [comment(checker.INTAKE, scope)]
        self.assertEqual({'add-spec-change-check'}, checker.trusted_scope(comments))
        self.assertEqual(['gitpm: changes/by: edit'], checker.check({}, {foreign: 'edited'}, comments, 'gitpm', issue=task()))

    def test_scope_with_change_outside_own_or_two_ids_fails_closed(self):
        for scope in ['own (ADDED; change add-check) and must not change foreign-change',
                      'own (change add-check, change other-check)',
                      'own (change add-check) own (ADDED to terminal)',
                      'own (ADDED to terminal)',
                      'change add-check']:
            with self.subTest(scope=scope), self.assertRaises(checker.CheckError):
                checker.trusted_scope([comment(checker.INTAKE, 'Ready\nSpec scope: ' + scope)])

    def test_existing_archive_with_own_name_needs_edit(self):
        path = 'openspec/changes/archive/2026-01-01-add-check/tasks.md'
        self.assertEqual(['gitpm: changes/add-check: edit'],
                         checker.check({path: 'old'}, {path: 'rewritten'}, [ready()], 'gitpm', issue=task()))
        self.assertEqual([], checker.check({path: 'old'}, {path: 'rewritten'},
                                          [ready(), grant('changes/add-check: edit')], 'gitpm', issue=task()))

    def test_own_delta_capability_differing_by_case_or_unicode_form_fails_closed(self):
        delta = '## MODIFIED Requirements\n### Requirement: Existing\nThe system SHALL change.\n'
        cafe_nfc, cafe_nfd = 'caf\u00e9', 'cafe\u0301'
        for existing, spelled in [('terminal', 'Terminal'), (cafe_nfc, cafe_nfd)]:
            base = {f'openspec/specs/{existing}/spec.md': ORIGINAL}
            head = {**base, f'openspec/changes/add-check/specs/{spelled}/spec.md': delta}
            with self.subTest(spelled=spelled), self.assertRaises(checker.CheckError):
                checker.check(base, head, [ready()], 'gitpm', issue=task())
        base = {SPEC: ORIGINAL}
        self.assertEqual(['gitpm: specs/terminal — «Existing»: modify (delta add-check)'], checker.check(
            base, {**base, 'openspec/changes/add-check/specs/terminal/spec.md': delta}, [ready()], 'gitpm', issue=task()))

    def test_capability_colliding_by_case_or_unicode_form_fails_closed(self):
        new_spec = '## Requirements\n\n### Requirement: New\nThe system SHALL add behavior.\n'
        cafe_nfc, cafe_nfd = 'openspec/specs/café/spec.md', 'openspec/specs/café/spec.md'
        for base, added in [({SPEC: ORIGINAL}, {'openspec/specs/Terminal/spec.md': new_spec}),
                            ({cafe_nfc: ORIGINAL}, {cafe_nfd: new_spec}),
                            ({}, {'openspec/specs/new/spec.md': new_spec, 'openspec/specs/NEW/spec.md': new_spec})]:
            with self.subTest(added=sorted(added)), self.assertRaises(checker.CheckError):
                checker.check(base, {**base, **added}, [ready()], 'gitpm', issue=task())
        base = {SPEC: ORIGINAL}
        self.assertEqual([], checker.check(base, {**base, 'openspec/specs/terminal-2/spec.md': new_spec}, [ready()], 'gitpm', issue=task()))


class TaskBinding(unittest.TestCase):
    def run_check(self, issue, comments):
        return checker.check({SPEC: ORIGINAL}, {SPEC: '## Requirements\n\n'}, comments, 'gitpm', issue=issue)

    def test_missing_or_incomplete_task_metadata_fails_closed(self):
        for issue in [None, {}, {'id': 1}, task(id=0), task(id=True), task(columnId=None),
                      task(columnId=True), task(createdAt=None), task(sequenceNumber=None),
                      task(createdAt='invalid'), task(createdAt='2026-10-20T00:00:00')]:
            with self.subTest(issue=issue), self.assertRaises(checker.CheckError):
                self.run_check(issue, [])

    def test_closed_tasks_fail_with_and_without_scope(self):
        for column in [9, 10]:
            for comments in [[comment(checker.INTAKE, 'Ready')],
                             [ready(), grant('specs/terminal — «Existing»: remove')]]:
                with self.subTest(column=column, comments=comments), self.assertRaisesRegex(checker.CheckError, 'Done or Canceled'):
                    self.run_check(task(columnId=column), comments)

    def test_legacy_requires_ready_from_intake(self):
        for comments in [[], [comment('worker', 'Ready')],
                         [comment(checker.INTAKE, 'Ready', kind='update')],
                         [comment(checker.INTAKE, 'Not ready')]]:
            with self.subTest(comments=comments), self.assertRaisesRegex(checker.CheckError, 'trusted intake ready'):
                self.run_check(task(columnId=8), comments)
        self.assertEqual([], self.run_check(task(columnId=8), [comment(checker.INTAKE, 'Ready to work.')]))

    def test_legacy_cutoff_is_strict_and_timezone_aware(self):
        comments = [comment(checker.INTAKE, 'Ready')]
        for created in ['2026-10-22T21:02:25.999999Z', '2026-10-22T23:02:25+02:00']:
            self.assertEqual([], self.run_check(task(createdAt=created), comments))
        for created in ['2026-10-22T21:02:26Z', '2026-10-22T23:02:26+02:00', '2026-10-23T00:00:00Z']:
            with self.subTest(created=created), self.assertRaisesRegex(checker.CheckError, 'requires intake Spec scope'):
                self.run_check(task(createdAt=created), comments)
        with self.assertRaisesRegex(checker.CheckError, 'trusted intake ready'):
            self.run_check(task(createdAt='2026-10-23T00:00:00Z'), [])

    def test_grants_only_count_in_review_supervise_merge(self):
        comments = [ready(), grant('specs/terminal — «Existing»: remove')]
        for column in [21, 22, 23]:
            self.assertEqual([], self.run_check(task(columnId=column), comments))
        for column in [6, 7, 8, 30]:
            with self.subTest(column=column):
                self.assertEqual(['gitpm: specs/terminal — «Existing»: remove'],
                                 self.run_check(task(columnId=column), comments))
                self.assertEqual([], checker.check({}, {'openspec/changes/add-check/tasks.md': 'own'},
                                                  comments, 'gitpm', issue=task(columnId=column)))
                self.assertEqual(['gitpm: changes/foreign: edit'], checker.check(
                    {}, {'openspec/changes/foreign/tasks.md': 'foreign'},
                    comments + [grant('changes/foreign: edit')], 'gitpm', issue=task(columnId=column)))

    def test_cli_task_matrix_and_current_column_recheck(self):
        with tempfile.TemporaryDirectory() as directory:
            exported = Path(directory) / 'task.json'
            args = ['--base', 'main', '--head', 'HEAD', '--branch', 'task/TILR-123',
                    '--component', 'gitpm', '--comments-json', str(exported)]
            legacy = [comment(checker.INTAKE, 'Ready')]
            grants = [ready(), grant('specs/terminal — «Existing»: remove')]
            cases = [(None, [], 1), (task(columnId=9), legacy, 1), (task(columnId=10), legacy, 1),
                     (task(createdAt='2026-10-23T00:00:00Z'), [], 1),
                     (task(createdAt='2026-10-23T00:00:00Z'), legacy, 1),
                     (task(columnId=8), legacy, 0), (task(columnId=8), grants, 1),
                     (task(columnId=21), grants, 0), (task(sequenceNumber=9999), legacy, 1)]
            for issue, comments, expected in cases:
                exported.write_text(json.dumps({'issue': issue, 'comments': comments}))
                with self.subTest(issue=issue, comments=comments), \
                        patch.object(checker, 'tree', side_effect=[{SPEC: ORIGINAL}, {SPEC: '## Requirements\n\n'}]), \
                        contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                    self.assertEqual(expected, checker.main(args))


class GitCLIIntegration(unittest.TestCase):
    def test_component_is_one_of_the_workflow_components(self):
        base = ['--base', 'main', '--head', 'HEAD', '--branch', 'task/TILR-9999', '--comments-json', '/nonexistent']
        for component in ['gitpm', 'tilrd', 'cloud', 'desktop-mac', 'github']:
            with self.subTest(component=component), contextlib.redirect_stderr(io.StringIO()):
                # Parsed, then fails closed on the missing export: exit 1, not argparse's 2.
                self.assertEqual(1, checker.main(base + ['--component', component]))
        for component in ['wf', '.github', 'Github']:
            with self.subTest(component=component), contextlib.redirect_stderr(io.StringIO()), \
                    self.assertRaises(SystemExit) as raised:
                checker.main(base + ['--component', component])
            self.assertEqual(2, raised.exception.code)

    def test_comments_only_export_cannot_prove_legacy_task(self):
        with tempfile.TemporaryDirectory() as directory:
            exported = Path(directory) / 'comments.json'
            exported.write_text('[]')
            args = ['--base', 'main', '--head', 'HEAD', '--branch', 'task/TILR-9999',
                    '--component', 'gitpm', '--comments-json', str(exported)]
            with patch.object(checker, 'tree', side_effect=[{SPEC: ORIGINAL}, {SPEC: '## Requirements\n\n'}]), \
                    contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(1, checker.main(args))

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
            comments.write_text(json.dumps(export([ready()])))
            args = ['--repo', directory, '--base', base, '--head', 'HEAD', '--branch', 'task/TILR-123', '--component', 'gitpm', '--comments-json', str(comments)]
            output = io.StringIO()
            with contextlib.redirect_stdout(output):
                self.assertEqual(1, checker.main(args))
            self.assertIn('specs/terminal — «Existing»: modify', output.getvalue())
            comments.write_text(json.dumps(export([ready(), grant('specs/terminal — «Existing»: modify')])))
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
        with patch.object(client, 'call', side_effect=[task(sequenceNumber=245), rows[:2], rows[2:]]) as call:
            self.assertEqual(3, len(client.task(245)['comments']))
            self.assertEqual(3, call.call_count)

    def test_task_lookup_must_return_the_matching_active_issue(self):
        client = checker.MCP.__new__(checker.MCP)
        client.schemas = {'get_issue_by_number': {'properties': {'number': {}}}}
        for issue in [None, {}, task(sequenceNumber=9999), task(columnId=9), task(columnId=10)]:
            with self.subTest(issue=issue), patch.object(client, 'call', return_value=issue) as call:
                with self.assertRaises(checker.CheckError):
                    client.task(123)
                self.assertEqual(1, call.call_count)

    def test_truncated_comments_fail_closed(self):
        client = checker.MCP.__new__(checker.MCP)
        client.schemas = {
            'get_issue_by_number': {'properties': {'number': {}}},
            'list_issue_activity': {'properties': {'limit': {'maximum': 1}}},
        }
        with patch.object(client, 'call', side_effect=[task(sequenceNumber=245), [ready()]]):
            with self.assertRaisesRegex(checker.CheckError, 'complete'):
                client.task(245)


if __name__ == '__main__':
    unittest.main()
