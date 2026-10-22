#!/usr/bin/env python3
"""Authorize OpenSpec changes using trusted task comments; stdlib only."""
import argparse
import difflib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import unicodedata
import urllib.request

DAN = 'Iu7nTn6V69XRWEmaZjjjeNVn9JyMwdsA'
INTAKE = 'de530293-4ccc-44bc-8d3d-d24a781a8351'
ENDPOINT = 'https://api-plan.rexuni.dev/mcp'
# Cloudflare in front of Plan answers 403 to the default Python-urllib User-Agent.
USER_AGENT = 'tilr-spec-change/1'
REQUIREMENT = re.compile(r'^### Requirement: (.+?)\s*$')
ENTRY = re.compile(r'^- ([\w-]+): (?:specs/([^\s]+) — «(.+)»: (modify|remove|rename)|changes/([^\s:]+): (edit|archive))\s*$')


class CheckError(Exception):
    pass


def trusted_scope(comments):
    scopes = []
    for comment in comments:
        body = comment.get('body', '')
        if comment.get('actorUserId') != INTAKE or comment.get('kind', 'comment') != 'comment':
            continue
        if not re.search(r'^Ready\b', body, re.M | re.I):
            continue
        scopes.extend(re.findall(r'^Spec scope:([^\r\n]*)', body, re.M))
    if not scopes:
        return None
    # Latest intake scope supersedes earlier ready comments.
    scope = scopes[-1]
    # Only `change <id>` inside a single own (...) names the own change; "spec-change" is not the word.
    word = r'(?<![\w-])change\s+`?([a-z0-9][a-z0-9-]*)`?'
    groups = re.findall(r'\bown\s*\(([^()]*)\)', scope)
    own = re.findall(word, groups[0]) if len(groups) == 1 else []
    if len(own) != 1 or len(re.findall(word, scope)) != 1:
        raise CheckError('Spec scope does not identify exactly one own change')
    return set(own)


def permissions(comments, component):
    allowed = set()
    for comment in comments:
        if comment.get('actorUserId') != DAN or comment.get('kind', 'comment') != 'comment':
            continue
        in_block = False
        for line in comment.get('body', '').splitlines():
            if line.strip() == 'spec-change:':
                in_block = True
                continue
            if not in_block:
                continue
            match = ENTRY.fullmatch(line.strip())
            if not match:
                if line.strip():
                    in_block = False
                continue
            comp, capability, requirement, operation, change, change_operation = match.groups()
            if comp == component:
                allowed.add(('spec', capability, requirement, operation) if capability else ('change', change, change_operation))
    return allowed


def requirement_at(lines, index):
    name = None
    for line in lines[:index + 1]:
        match = REQUIREMENT.fullmatch(line)
        if match:
            name = match[1]
    return name


def changed_requirements(old, new):
    """Map edits, including inserted content, to existing requirements."""
    before, after = old.splitlines(), new.splitlines()
    names_before = {m[1] for line in before if (m := REQUIREMENT.fullmatch(line))}
    names_after = {m[1] for line in after if (m := REQUIREMENT.fullmatch(line))}
    result = set()
    for tag, i, j, k, l in difflib.SequenceMatcher(None, before, after, autojunk=False).get_opcodes():
        if tag == 'equal':
            for old_index, new_index in zip(range(i, j), range(k, l)):
                name, owner = requirement_at(before, old_index), requirement_at(after, new_index)
                if before[old_index].strip() and name in names_before and name != owner:
                    if name in names_after:
                        result.add((name, 'modify'))
                    elif owner in names_before:
                        # A removed heading must not hand its text to an existing requirement.
                        result.add((owner, 'modify'))
            continue
        for index in range(i, j):
            name = requirement_at(before, index)
            operation = 'modify' if name in names_after else 'remove'
            result.add((name, operation))
        for index in range(k, l):
            name = requirement_at(after, index)
            if after[index].strip() and name in names_before:
                result.add((name, 'modify'))
    return result


def requirement_sections(body):
    sections = {}
    name = None
    for line in body.splitlines():
        match = REQUIREMENT.fullmatch(line)
        if match:
            name = match[1]
            sections[name] = []
        elif name:
            sections[name].append(line)
    return sections


def is_heading_rename(old, new, name):
    before, after = requirement_sections(old), requirement_sections(new)
    return name in before and any(
        candidate not in before and lines == before[name]
        for candidate, lines in after.items())


def delta_operations(body, existing):
    section = None
    operations = []
    renamed_from = None
    identities = {name: name for name in existing}
    histories = {name: {name} for name in existing}
    renames = []
    for line in body.splitlines():
        match = re.fullmatch(r'## (ADDED|MODIFIED|REMOVED|RENAMED) Requirements\s*', line)
        if match:
            section = match[1]
        match = REQUIREMENT.fullmatch(line)
        if match and section in ('MODIFIED', 'REMOVED'):
            operations.append((match[1], 'modify' if section == 'MODIFIED' else 'remove'))
        if section == 'RENAMED':
            match = re.fullmatch(r'- FROM: `### Requirement: (.+)`\s*', line)
            if match:
                renamed_from = match[1]
            match = re.fullmatch(r'- TO: `### Requirement: (.+)`\s*', line)
            if match and renamed_from:
                target = match[1]
                original = identities.get(renamed_from, renamed_from)
                history = histories.setdefault(original, {renamed_from})
                if target in history:
                    raise CheckError('Cyclic requirement rename: ' + target)
                if target in identities:
                    raise CheckError('Ambiguous requirement rename: ' + target)
                history.add(target)
                identities.pop(renamed_from, None)
                identities[target] = original
                renames.append((original, 'rename'))
                renamed_from = None
    yield from renames
    for name, operation in operations:
        yield identities.get(name, name), operation


def capability_collisions(base, head):
    """New capability folders may not differ from others only by case or Unicode form."""
    def capabilities(tree):
        return {m[1] for path in tree if (m := re.match(r'openspec/specs/([^/]+)/', path))}
    existing, current = capabilities(base), capabilities(head)
    key = lambda name: unicodedata.normalize('NFC', unicodedata.normalize('NFC', name).casefold())
    for name in sorted(current - existing):
        clash = sorted(other for other in existing | current if other != name and key(other) == key(name))
        if clash:
            raise CheckError(f'Capability {name} collides with {clash[0]} by case or Unicode form')


def check(base, head, comments, component):
    """Trees map OpenSpec paths to UTF-8 text; no candidate code is executed."""
    own = trusted_scope(comments)
    if own is None:
        return []
    capability_collisions(base, head)
    allowed = permissions(comments, component)
    uncovered = set()
    for path in sorted(base.keys() | head.keys()):
        old, new = base.get(path, ''), head.get(path, '')
        if old == new:
            continue
        match = re.fullmatch(r'openspec/specs/([^/]+)/spec\.md', path)
        if match:
            capability = match[1]
            for name, operation in changed_requirements(old, new):
                # A heading replacement can be authorized as rename.
                acceptable = {('spec', capability, name, operation)}
                if operation == 'remove' and is_heading_rename(old, new, name):
                    acceptable.add(('spec', capability, name, 'rename'))
                if not acceptable & allowed:
                    uncovered.add(f'{component}: specs/{capability} — «{name or "<document metadata>"}»: {operation}')
            continue
        match = re.fullmatch(r'openspec/changes/(?:archive/\d{4}-\d{2}-\d{2}-)?([^/]+)/(.+)', path)
        if not match:
            # Unknown OpenSpec paths may not hide a specification edit.
            uncovered.add(f'{component}: unsupported OpenSpec path {path}')
            continue
        change, suffix = match.groups()
        is_archive = path.startswith('openspec/changes/archive/')
        # An archive already in dev is foreign even when it carries the own change name.
        folder = '/'.join(path.split('/')[:4]) + '/'
        is_own = change in own and not (is_archive and any(p.startswith(folder) for p in base))
        if not is_own:
            # An unchanged archive copy still needs the archive grant for its source.
            archived = (path not in base and is_archive) or (not is_archive and any(
                re.fullmatch(r'openspec/changes/archive/\d{4}-\d{2}-\d{2}-' + re.escape(change + '/' + suffix), p)
                for p in head.keys() - base.keys()))
            operation = 'archive' if archived else 'edit'
            if ('change', change, operation) not in allowed:
                uncovered.add(f'{component}: changes/{change}: {operation}')
            if archived:
                source = f'openspec/changes/{change}/{suffix}'
                targets = [p for p in head.keys() - base.keys() if re.fullmatch(
                    r'openspec/changes/archive/\d{4}-\d{2}-\d{2}-' + re.escape(change + '/' + suffix), p)]
                source_edited = source in head and head[source] != base.get(source, '')
                if source_edited or any(source not in base or head[p] != base[source] for p in targets):
                    if ('change', change, 'edit') not in allowed:
                        uncovered.add(f'{component}: changes/{change}: edit')
        delta = re.fullmatch(r'specs/([^/]+)/spec\.md', suffix)
        if is_own and delta:
            capability = delta[1]
            baseline = base.get(f'openspec/specs/{capability}/spec.md', '')
            existing = {m[1] for line in baseline.splitlines() if (m := REQUIREMENT.fullmatch(line))}
            for name, operation in delta_operations(new, existing):
                if name in existing and ('spec', capability, name, operation) not in allowed:
                    uncovered.add(f'{component}: specs/{capability} — «{name}»: {operation} (delta {change})')
    return sorted(uncovered)


def git(repo, *args):
    process = subprocess.run(['git', '-C', str(repo), *args], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if process.returncode:
        raise CheckError('git failed: ' + process.stderr.decode(errors='replace').strip())
    return process.stdout


def tree(repo, revision):
    commit = git(repo, 'rev-parse', '--verify', revision + '^{commit}').decode().strip()
    paths = git(repo, 'ls-tree', '-rz', commit, '--', 'openspec/').split(b'\0')
    result = {}
    for entry in paths:
        if not entry:
            continue
        metadata, path = entry.split(b'\t', 1)
        mode, kind, blob = metadata.split()
        if kind != b'blob' or mode not in (b'100644', b'100755'):
            raise CheckError('OpenSpec must contain regular text files')
        name = path.decode('utf-8')
        # config and project-level documentation are not specifications/changes.
        if name.startswith(('openspec/specs/', 'openspec/changes/')):
            result[name] = git(repo, 'cat-file', 'blob', blob.decode()).decode('utf-8')
    return result


class MCP:
    def __init__(self, token):
        self.token, self.session, self.counter = token, None, 0
        self.protocol = '2024-11-05'
        initialization = self.request('initialize', {'protocolVersion': '2024-11-05', 'capabilities': {}, 'clientInfo': {'name': 'spec-change', 'version': '1'}})
        self.protocol = initialization.get('protocolVersion', self.protocol)
        self.request('notifications/initialized', notification=True)
        self.schemas = {t['name']: t['inputSchema'] for t in self.request('tools/list')['tools']}

    def request(self, method, params=None, notification=False):
        self.counter += 1
        body = {'jsonrpc': '2.0', 'method': method, 'params': params or {}}
        if not notification:
            body['id'] = self.counter
        headers = {'Authorization': 'Bearer ' + self.token, 'Accept': 'application/json, text/event-stream', 'Content-Type': 'application/json', 'User-Agent': USER_AGENT}
        if self.session:
            headers['Mcp-Session-Id'] = self.session
            headers['MCP-Protocol-Version'] = self.protocol
        request = urllib.request.Request(ENDPOINT, json.dumps(body).encode(), headers)
        with urllib.request.urlopen(request, timeout=30) as response:
            self.session = response.headers.get('Mcp-Session-Id', self.session)
            raw = response.read().decode()
            content_type = response.headers.get('Content-Type', '')
        if notification:
            return None
        if 'text/event-stream' in content_type:
            messages = []
            for event in re.split(r'\r?\n\r?\n', raw):
                data = '\n'.join(line[5:].lstrip() for line in event.splitlines() if line.startswith('data:'))
                if data:
                    messages.append(json.loads(data))
            payload = next((m for m in messages if m.get('id') == self.counter), None)
            if payload is None:
                raise CheckError('MCP response contains no matching message')
        else:
            payload = json.loads(raw)
        if 'error' in payload:
            raise CheckError('MCP rejected ' + method)
        return payload['result']

    def call(self, name, arguments):
        result = self.request('tools/call', {'name': name, 'arguments': arguments})
        if result.get('isError'):
            raise CheckError('Task API rejected ' + name)
        data = result.get('structuredContent')
        if data is None:
            texts = [c['text'] for c in result.get('content', []) if c.get('type') == 'text']
            if len(texts) != 1:
                raise CheckError('Unexpected task API content')
            data = json.loads(texts[0])
        if isinstance(data, dict) and data.get('ok') is False:
            raise CheckError('Task API read failed')
        return data.get('data', data) if isinstance(data, dict) else data

    def comments(self, number):
        schema = self.schemas['get_issue_by_number']['properties']
        number_key = next((key for key in ('number', 'issueNumber', 'sequenceNumber') if key in schema), None)
        if not number_key:
            raise CheckError('Unsupported task lookup schema')
        issue = self.call('get_issue_by_number', {'projectKey': 'tilr.TILR', number_key: number})
        schema = self.schemas['list_issue_activity']['properties']
        limit = min(schema.get('limit', {}).get('maximum', 100), 100)
        args = {'issueId': issue['id'], 'filter': 'comments', 'limit': limit}
        result = []
        if 'offset' in schema:
            args['offset'] = 0
        while True:
            page = self.call('list_issue_activity', args)
            rows = page if isinstance(page, list) else page.get('items', page.get('activities', page.get('comments')))
            if not isinstance(rows, list):
                raise CheckError('Unsupported comments response')
            result.extend(rows)
            cursor = page.get('nextCursor') if isinstance(page, dict) else None
            if cursor and 'cursor' in schema:
                args['cursor'] = cursor
            elif len(rows) < limit:
                break
            elif 'offset' in schema:
                args['offset'] += len(rows)
            else:
                raise CheckError('Cannot prove comments are complete: pagination unavailable')
        return sorted(result, key=lambda c: (c.get('createdAt', ''), c.get('id', 0)))


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', type=Path, default=Path('.'))
    parser.add_argument('--base', required=True)
    parser.add_argument('--head', required=True)
    parser.add_argument('--branch', required=True)
    parser.add_argument('--component', required=True, choices=['gitpm', 'tilrd', 'cloud', 'desktop-mac'])
    parser.add_argument('--comments-json', type=Path, help='trusted exported comment array for offline checks')
    args = parser.parse_args(argv)
    try:
        match = re.fullmatch(r'task/TILR-([1-9][0-9]*)', args.branch)
        if not match:
            raise CheckError('Expected branch task/TILR-<number>')
        if args.comments_json:
            comments = json.loads(args.comments_json.read_text())
        else:
            token = os.environ.get('ITSAPLAN_READ_ONLY_TOKEN')
            if not token:
                raise CheckError('Set ITSAPLAN_READ_ONLY_TOKEN to the dedicated read-only CI credential')
            comments = MCP(token).comments(int(match[1]))
        failures = check(tree(args.repo, args.base), tree(args.repo, args.head), comments, args.component)
        if failures:
            print('spec-change: uncovered changes:\n' + '\n'.join('- ' + f for f in failures))
            return 1
        print('spec-change: authorized (or legacy task without Spec scope)')
        return 0
    except (CheckError, ValueError, KeyError, TypeError, OSError) as error:
        # Never print HTTP request headers or credentials.
        print('spec-change: ' + str(error), file=sys.stderr)
        return 1


if __name__ == '__main__':
    sys.exit(main())
