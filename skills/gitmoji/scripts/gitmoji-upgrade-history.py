#!/usr/bin/env python3
"""Preview retrospective GitHub PR captions and build a separate candidate branch."""

import argparse
from collections import defaultdict
from functools import lru_cache
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import unicodedata


QUERY = """
query($owner: String!, $name: String!, $endCursor: String) {
  repository(owner: $owner, name: $name) {
    nameWithOwner
    pullRequests(states: MERGED, first: 100, after: $endCursor) {
      nodes { number title mergeCommit { oid } headRefName baseRefName headRepositoryOwner { login } }
      pageInfo { hasNextPage endCursor }
    }
  }
}
"""
ASSOCIATIONS = """
query($owner: String!, $name: String!, $oid: GitObjectID!, $endCursor: String) {
  repository(owner: $owner, name: $name) {
    object(oid: $oid) { ... on Commit {
      associatedPullRequests(first: 100, after: $endCursor) {
        nodes { number repository { nameWithOwner } }
        pageInfo { hasNextPage endCursor }
      }
    } }
  }
}
"""
MEMBERSHIP = """
query($owner: String!, $name: String!, $number: Int!, $endCursor: String) {
  repository(owner: $owner, name: $name) {
    pullRequest(number: $number) {
      commits(first: 100, after: $endCursor) {
        nodes { commit { oid } }
        pageInfo { hasNextPage endCursor }
      }
    }
  }
}
"""
UNDO = re.compile(rb'^(?:revert(?:ed)?\b|reapply\b|rewind\b|\xe2\x8f\xaa)', re.I)
GENERATED = re.compile(rb'^(?:merge(?:d)?\b|sync\b|cherry-pick\b|(?:squash|fixup|amend)!)', re.I)
REVERT_TARGET = re.compile(
    rb'^This reverts commit ([0-9a-f]{40}|[0-9a-f]{64})(?:\.|, reversing)$', re.M
)


def run(*args, data=None):
    result = subprocess.run(args, input=data, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if result.returncode:
        raise ValueError(result.stderr.decode(errors='replace').strip() or f'{args[0]} failed')
    return result.stdout


def git(*args, data=None):
    return run('git', '--no-replace-objects', *args, data=data)


def resolve(ref):
    return git('rev-parse', '--verify', '--end-of-options', ref + '^{commit}').decode().strip()


class PullRequests:
    def __init__(self, repo, host):
        self.repo = repo
        self.host = host
        self.by_oid = defaultdict(list)
        self.by_number = {}
        for page in self.pages(QUERY):
            if page['nameWithOwner'].casefold() != repo.casefold():
                raise ValueError('GitHub returned a different repository')
            for pr in page['pullRequests']['nodes']:
                self.by_number[pr['number']] = pr
                if pr['mergeCommit']:
                    self.by_oid[pr['mergeCommit']['oid']].append(pr)

    def pages(self, query, *variables):
        owner, name = self.repo.split('/')
        result = json.loads(run('gh', 'api', '--hostname', self.host, 'graphql',
                                '--paginate', '--slurp', '-f', 'query=' + query,
                                '-f', 'owner=' + owner, '-f', 'name=' + name, *variables))
        pages = []
        for page in result:
            repository = (page.get('data') or {}).get('repository')
            if page.get('errors') or not repository:
                raise ValueError('GitHub could not read PR metadata for ' + self.repo)
            pages.append(repository)
        return pages

    @lru_cache(maxsize=64)
    def member_oids(self, number):
        pages = self.pages(MEMBERSHIP, '-F', f'number={number}')
        return {node['commit']['oid'] for page in pages
                for node in page['pullRequest']['commits']['nodes']}

    @lru_cache(maxsize=128)
    def containing_pr(self, oid):
        matches = set()
        for page in self.pages(ASSOCIATIONS, '-f', 'oid=' + oid):
            if not page['object']:
                continue
            for pr in page['object']['associatedPullRequests']['nodes']:
                number = pr['number']
                if (pr['repository']['nameWithOwner'].casefold() == self.repo.casefold()
                        and number in self.by_number and oid in self.member_oids(number)):
                    matches.add(number)
        if len(matches) == 1:
            return self.by_number[matches.pop()]
        return None


def read_commit(oid):
    header, message = git('cat-file', 'commit', oid).split(b'\n\n', 1)
    # Compound headers such as gpgsig and mergetag have space-prefixed continuations.
    fields = re.split(rb'\n(?! )', header)
    parents = [field[7:].decode('ascii') for field in fields if field.startswith(b'parent ')]
    return fields, parents, message


class ReviewNeeded(Exception):
    """Historical evidence is insufficient for an automatic subject."""


def checked_title(title, label):
    if not title.strip() or any(unicodedata.category(c) in ('Cc', 'Zl', 'Zp') for c in title):
        raise ReviewNeeded(f'{label} needs a nonempty, single-line subject without controls')
    return title


def target_subject(fields, message):
    encoding = next((f[9:] for f in fields if f.startswith(b'encoding ')), b'utf-8')
    try:
        subject = message.split(b'\n', 1)[0].decode(encoding.decode('ascii'))
    except (LookupError, UnicodeError) as error:
        raise ReviewNeeded('cannot decode the target subject with its declared encoding') from error
    return checked_title(subject, 'target commit')


def tree_changes(*trees):
    # Raw, unabbreviated entries include both blob IDs, modes and NUL-delimited paths.
    # Disabling rename detection gives the same representation in both directions.
    return git('diff-tree', '--no-commit-id', '--no-abbrev', '--raw', '-r', '-z', '--no-renames',
               '--no-relative', '--no-ext-diff', '--no-textconv', '--ignore-submodules=none', *trees, '--')


def revert_origin(oid, parent, message, prs):
    """Verify each inverse against its original Git objects, including targets outside the range."""
    depth = 0
    while True:
        matches = REVERT_TARGET.findall(message)
        if len(matches) != 1:
            raise ReviewNeeded('undo needs one full Git revert target ID')
        target = matches[0].decode('ascii')
        ancestor = subprocess.run(['git', '--no-replace-objects', 'merge-base', '--is-ancestor',
                                   target, parent], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        if ancestor.returncode:
            raise ReviewNeeded('undo target is outside its parent history')
        fields, parents, target_message = read_commit(target)
        if len(parents) > 1:
            mainline = (b'This reverts commit ' + matches[0] + b', reversing\nchanges made to '
                        + parents[0].encode() + b'.')
            if len(parents) != 2 or mainline not in message:
                raise ReviewNeeded('undo needs the first parent of a two-parent merge as its mainline')
        inverse = tree_changes(target, parents[0]) if parents else tree_changes('--root', '-R', target)
        if not inverse or inverse != tree_changes(parent, oid):
            raise ReviewNeeded('changed paths, modes or blobs do not prove an exact inverse')
        depth += 1
        candidates = prs.by_oid.get(target, [])
        if len(candidates) > 1:
            raise ReviewNeeded('multiple merged PRs claim the undo target')
        if len(parents) == 2 and candidates:
            return candidates[0], depth, parents, None, 'merge'
        if len(parents) == 1 and UNDO.match(target_message) and REVERT_TARGET.search(target_message):
            if candidates:
                raise ReviewNeeded('undo chain includes an ambiguous one-parent PR landing')
            oid, parent, message = target, parents[0], target_message
            continue
        if candidates:
            return candidates[0], depth, parents, target_subject(fields, target_message), 'landing commit'
        if len(parents) <= 1:
            member = prs.containing_pr(target)
            if member:
                return member, depth, parents, target_subject(fields, target_message), 'member commit'
        raise ReviewNeeded('undo target has no unique verified merged PR')


def merge_caption(pr, parents):
    checked_title(pr['title'], 'pull request')
    caption = run('gitmoji', 'merge-caption', '--pr', str(pr['number']), '--title=' + pr['title'],
                  parents[0], parents[1]).rstrip(b'\n')
    if not caption.startswith(f'Merge #{pr["number"]} '.encode()) or b'\n' in caption or b'\r' in caption:
        raise ValueError('gitmoji merge-caption did not return the expected subject')
    return caption


def proposal(oid, parents, message, prs):
    old = message.split(b'\n', 1)[0]
    matches = prs.by_oid.get(oid, [])
    if len(matches) > 1:
        return None, 'multiple merged PRs claim this commit', None
    if len(parents) == 2 and matches:
        pr = matches[0]
        number = pr['number']
        caption = merge_caption(pr, parents)
        # The PR record supplies names even if the branch refs have been deleted.
        owner = (pr['headRepositoryOwner'] or {}).get('login')
        source = (owner + ':' if owner else '') + pr['headRefName']
        route = f"Merged from {source} into {pr['baseRefName']} (PR #{number}).".encode()
        return caption, 'exact merged PR commit', route
    if matches:
        return None, 'one-parent or nonstandard PR landing requires review of the original operation', None
    if len(parents) == 1 and UNDO.match(old):
        if re.match(rb'^(?:Revert|Reapply) #[0-9]+ [^\n]+\n\nSource commit: [0-9a-f]+\nTarget:', message):
            return None, 'already captioned undo; retain its original provenance', None
        pr, depth, target_parents, subject, kind = revert_origin(oid, parents[0], message, prs)
        verb = b'Revert' if depth % 2 else b'Reapply'
        if subject is None:
            caption = verb + merge_caption(pr, target_parents)[len(b'Merge'):]
        else:
            caption = verb + f' #{pr["number"]} {subject}'.encode()
        route = f'Target: {kind} of PR #{pr["number"]}.'
        introducing = prs.containing_pr(oid)
        if introducing and introducing['number'] != pr['number']:
            route += f' Introduced by PR #{introducing["number"]}.'
        return caption, 'exact inverse and verified target PR', route.encode()
    if len(parents) >= 2:
        return None, 'merge without a unique two-parent PR landing', None
    if UNDO.match(old) or GENERATED.match(old) or REVERT_TARGET.search(message):
        return None, 'requires review of the original operation', None
    return None, None, None


def ensure_branch_available(candidate):
    # An unborn branch has no object ID. Its checked-out name still belongs to a worktree.
    worktrees = git('worktree', 'list', '--porcelain', '-z').split(b'\0')
    if b'branch ' + os.fsencode(candidate) in worktrees:
        raise ValueError('candidate branch is checked out in a worktree; choose a fresh name')
    symref = subprocess.run(['git', 'symbolic-ref', '--quiet', '--no-recurse', candidate],
                            stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    if symref.returncode == 0:
        raise ValueError('candidate is an existing symbolic ref; choose a fresh name')
    if symref.returncode != 1:
        raise ValueError('could not verify the candidate ref: ' + symref.stderr.decode(errors='replace'))
    exists = subprocess.run(['git', 'show-ref', '--verify', '--quiet', candidate])
    if exists.returncode == 0:
        raise ValueError('candidate branch already exists; choose a fresh name')
    if exists.returncode != 1:
        raise ValueError('could not verify whether the candidate branch already exists')


def make_candidate(args):
    if git('rev-parse', '--is-shallow-repository').strip() == b'true':
        raise ValueError('history is shallow; use a full clone')
    if Path(os.fsdecode(git('rev-parse', '--git-path', 'info/grafts').strip())).is_file():
        raise ValueError('use a complete checkout without grafted history')
    tip = resolve(args.ref)
    revisions = [tip]
    if args.base:
        base = resolve(args.base)
        if subprocess.run(['git', '--no-replace-objects', 'merge-base', '--is-ancestor', base, tip]).returncode:
            raise ValueError('--base must be an ancestor of the source tip')
        revisions.append('^' + base)
    candidate = 'refs/heads/' + args.apply if args.apply else None
    if candidate:
        git('check-ref-format', candidate)
        if args.apply.startswith('-'):
            raise ValueError('candidate branch name cannot begin with a dash')
        ensure_branch_available(candidate)
    commits = git('rev-list', '--reverse', '--topo-order', *revisions).decode().splitlines()
    prs = PullRequests(args.repo, args.hostname)
    remapped = {}
    changes = []
    signed = []
    blocked = []
    for oid in commits:
        fields, parents, message = read_commit(oid)
        subject = message.split(b'\n', 1)[0]
        try:
            new, reason, route = proposal(oid, parents, message, prs)
        except ReviewNeeded as error:
            new, reason, route = None, str(error), None
        if new is not None and new != subject:
            # Keep the original subject and complete body, including conflict notes and trailers.
            provenance = b'Source commit: ' + oid.encode() + b'\n'
            if route:
                provenance += route + b'\n'
            targets = REVERT_TARGET.findall(message)
            if len(targets) == 1 and targets[0].decode() in remapped:
                provenance += b'Candidate undo target: ' + remapped[targets[0].decode()].encode() + b'\n'
            message = new + b'\n\n' + provenance + b'\n' + message
        elif not any(p in remapped for p in parents):
            if new is None and reason:
                changes.append({'old': oid, 'subject': subject.decode(errors='replace'), 'review': reason})
            continue
        rebuilt = []
        for field in fields:
            key = field.split(b' ', 1)[0]
            if key in (b'gpgsig', b'gpgsig-sha256'):
                if oid not in signed:
                    signed.append(oid)
                continue
            if key == b'mergetag':
                blocked.append(oid)
            if key == b'encoding' and new is not None and new != subject:
                encoding = field[9:].strip().lower()
                if encoding not in (b'utf-8', b'utf8'):
                    blocked.append(oid)
            if key == b'parent':
                parent = field[7:].decode('ascii')
                field = b'parent ' + remapped.get(parent, parent).encode()
            rebuilt.append(field)
        raw = b'\n'.join(rebuilt) + b'\n\n' + message
        options = ['-w'] if candidate and not blocked and (not signed or args.allow_unsigned) else []
        result = git('hash-object', '-t', 'commit', *options, '--stdin', data=raw).decode().strip()
        remapped[oid] = result
        entry = {'old': oid, 'candidate': result, 'subject': subject.decode(errors='replace')}
        if new is not None and new != subject:
            entry.update(subject_new=new.decode(), evidence=reason)
        elif reason and new is None:
            entry['review'] = reason
        changes.append(entry)
    if candidate:
        if blocked:
            raise ValueError('candidate touches mergetags or non-UTF-8 captions; review the dry run')
        if signed and not args.allow_unsigned:
            raise ValueError('candidate invalidates signatures; review the dry run and --allow-unsigned')
        if not remapped:
            raise ValueError('no verified changes to apply')
        if args.expect_candidate and remapped[tip] != args.expect_candidate:
            raise ValueError('candidate tip differs from --expect-candidate; run a new preview')
        # create is compare-and-swap against absence: never move or replace an existing branch.
        new_tip = remapped[tip]
        ensure_branch_available(candidate)
        git('update-ref', '--no-deref', candidate, new_tip, '0' * len(tip))
    return {'repository': args.hostname + '/' + args.repo, 'source_tip': tip,
            'candidate_tip': remapped.get(tip, tip), 'created_branch': args.apply,
            'review_or_changed_commits': changes, 'invalidated_signatures': signed,
            'blocked_mergetags_or_encodings': blocked}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', required=True, metavar='OWNER/REPO', help='repository containing the merged PRs')
    parser.add_argument('--hostname', default=os.environ.get('GH_HOST') or 'github.com')
    parser.add_argument('--ref', default='HEAD', help='source tip (default: HEAD)')
    parser.add_argument('--base', help='exclude this ancestor and its history')
    parser.add_argument('--apply', metavar='NEW_LOCAL_BRANCH', help='create a candidate after the dry run')
    parser.add_argument('--expect-candidate', metavar='OID',
                        help='apply only if the candidate tip matches the reviewed dry run')
    parser.add_argument('--allow-unsigned', action='store_true', help='allow signatures to be removed from rewritten commits')
    args = parser.parse_args()
    if args.expect_candidate and not args.apply:
        parser.error('--expect-candidate requires --apply')
    if not re.fullmatch(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+', args.repo):
        parser.error('--repo requires OWNER/REPO')
    os.environ["GIT_NO_REPLACE_OBJECTS"] = "1"
    try:
        result = make_candidate(args)
    except (OSError, ValueError, KeyError, TypeError) as error:
        parser.exit(1, 'history upgrade: ' + str(error) + '\n')
    # A redirected report is always UTF-8, even when Python selected ASCII for stdout.
    sys.stdout.buffer.write((json.dumps(result, indent=2, ensure_ascii=False) + '\n').encode('utf-8'))


if __name__ == '__main__':
    main()
