#!/usr/bin/env python3
"""Preview retrospective GitHub PR captions, build a candidate branch or update reviewed PR titles."""

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
from typing import NamedTuple, Optional


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


# Only allow a vocabulary inventory (validated by the CLI below) to serve as
# a subject-based link. A free-form "Merge #N" or a bare "(#N)" is too common.
LEGACY_CAPTION = re.compile(rb'^Merge #([1-9][0-9]{0,9}) (\[`.+?\] .+)$')
LANDED_CAPTION = re.compile(rb'^(\[`.+?\] .+) \(#([1-9][0-9]{0,9})\)$')


def local_caption(message):
    # A local caption can finish with the number of an *arriving* PR. The inventory
    # paragraph distinguishes it from a landing whose human title contains "into".
    subject, _, body = message.partition(b'\n\n')
    if subject.startswith(b'[`'):
        _, _, subject = subject.partition(b'] ')
    route, _, summary = subject.partition(b': ')
    sources, _, target = route.partition(b' into ')
    return bool(sources and target and summary and re.match(
        rb'Arrived from ' + re.escape(sources) + rb' \([0-9]+\):(?:\n|$)', body
    ))


def caption_number(subject):
    legacy = LEGACY_CAPTION.fullmatch(subject)
    if legacy:
        return int(legacy[1])
    landed = LANDED_CAPTION.fullmatch(subject)
    return int(landed[2]) if landed else None


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

    def index_captions(self, tip):
        # Include the whole source graph even when --base limits the planned edits.
        self.tip = tip
        self.caption_claims = defaultdict(set)
        for row in git('log', '--no-show-signature', '--merges', '--format=%H%x00%s', tip).split(b'\n'):
            if row:
                oid, subject = row.split(b'\0', 1)
                number = caption_number(subject)
                if number in self.by_number:
                    oid = oid.decode('ascii')
                    if b' into ' not in subject or not local_caption(read_commit(oid)[2]):
                        self.caption_claims[number].add(oid)

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
        if len(parents) == 2:
            landing = verified_landing(target, parents, target_message, prs)
            if landing:
                pr, caption, _ = landing
                return pr, depth, parents, caption['title'], 'merge'
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
    inventory = json.loads(run('gitmoji', 'merge-caption', '--json', '--pr', str(pr['number']),
                               '--title=' + pr['title'], parents[0], parents[1]))
    title = inventory.get('title') if isinstance(inventory, dict) else None
    if not isinstance(title, str):
        raise ValueError('gitmoji must supply the PR title and landed subject; check the CLI version')
    checked_title(title, 'caption')
    if inventory.get('subject') != f'{title} (#{pr["number"]})':
        raise ValueError('gitmoji merge-caption returned an inconsistent landed subject')
    return inventory


def landing_title(subject, pr, oid):
    # An individual landing's original heading can describe only that commit. Keep
    # its own inventory and any other issue references when removing its PR decoration.
    suffix = f' (#{pr["number"]})'
    while subject.endswith(suffix) and subject[:-len(suffix)].strip():
        subject = subject[:-len(suffix)]
    legacy = subject.removeprefix(f'Merge #{pr["number"]} ')
    if legacy != subject:
        # With an empty range the CLI supplies just the human title. Let its vocabulary
        # decide whether a bracket group is ours. Keep the original inventory on the undo.
        if legacy == pr['title'] or merge_caption({**pr, 'title': subject}, (oid, oid))['title'] != subject:
            subject = legacy
    return checked_title(subject, 'landing commit')


def verify_rewritten_trees(original, oid, parents):
    _, old_parents, _ = read_commit(original)
    if len(old_parents) != 2:
        raise ReviewNeeded(f'original landing {original} has {len(old_parents)} parents; expected two')
    old_ids = [original, *old_parents]
    new_ids = [oid, *parents]
    old_trees, new_trees = [
        git('rev-parse', *(item + '^{tree}' for item in ids)).splitlines()
        for ids in (old_ids, new_ids)
    ]
    labels = ['merge result', 'first parent', 'second parent']
    different = [label for label, old, new in zip(labels, old_trees, new_trees) if old != new]
    if not different:
        return 'matching merge and ordered parent trees'

    # A repository-wide edit can change every snapshot while leaving both sides
    # of the merge unchanged. Check each side separately, including paths,
    # old and new blob IDs, file modes and deletions. Empty merges provide no
    # independent patch evidence, even when their PR captions happen to agree.
    prefix = f'original landing {original} has different trees ({", ".join(different)})'
    has_changes = False
    for label, old_parent, parent in zip(labels[1:], old_parents, parents):
        old_delta = tree_changes(old_parent, original)
        if old_delta != tree_changes(parent, oid):
            raise ReviewNeeded(f'{prefix}; changes from the {label} differ')
        if old_delta:
            has_changes = True
    if not has_changes:
        raise ReviewNeeded(f'{prefix}; both parent changes are empty')

    # The raw author header carries the identity, timestamp and zone. Matching
    # this at all three positions corroborates the mapping independently of
    # captions and patches, which can recur on different branches.
    for label, before, after in zip(labels, old_ids, new_ids):
        old_fields, _, _ = read_commit(before)
        new_fields, _, _ = read_commit(after)
        authors = [[field for field in fields if field.startswith(b'author ')]
                   for fields in (old_fields, new_fields)]
        if len(authors[0]) != 1 or authors[0] != authors[1]:
            raise ReviewNeeded(f'{prefix}; author or author date differs at the {label}')
    return 'matching changes from both ordered parents and matching author records'


def verified_landing(oid, parents, message, prs):
    old = message.split(b'\n', 1)[0]
    matches = prs.by_oid.get(oid, [])
    if len(matches) > 1:
        raise ReviewNeeded('multiple merged PRs claim this commit')
    branch_caption = local_caption(message)
    number = None if branch_caption else caption_number(old)
    if matches:
        pr = matches[0]
        if number is not None and number != pr['number']:
            raise ReviewNeeded('caption number conflicts with the exact merged PR')
        # Preserve the historical wording of a landed title, including manual
        # edits that differ from the current hosted title. Rebuild its inventory.
        source = pr
        untagged_landing = old.endswith(f' (#{pr["number"]})'.encode()) and not old.startswith(b'Merge ')
        if number or (untagged_landing and not branch_caption):
            try:
                source = {**pr, 'title': old.decode('utf-8')}
            except UnicodeError as error:
                raise ReviewNeeded('cannot decode the landed title') from error
        return pr, merge_caption(source, parents), 'exact merged PR commit'
    if number is None:
        return None
    pr = prs.by_number.get(number)
    if not pr or not pr['mergeCommit']:
        raise ReviewNeeded('caption has no recorded merged PR in this repository')
    if prs.caption_claims[number] != {oid}:
        raise ReviewNeeded('multiple local merges carry this PR caption')
    original = pr['mergeCommit']['oid']
    if len(prs.by_oid[original]) != 1:
        raise ReviewNeeded('multiple merged PRs claim the original landing')
    object_evidence = ''
    # When the old objects are available, use them to rule out branch syncs,
    # squashes and different changes masquerading as a rewritten PR merge.
    if subprocess.run(['git', '--no-replace-objects', 'cat-file', '-e', original + '^{commit}'],
                      capture_output=True).returncode == 0:
        if subprocess.run(['git', '--no-replace-objects', 'merge-base', '--is-ancestor', original, prs.tip],
                          capture_output=True).returncode == 0:
            raise ReviewNeeded('GitHub’s actual merge is already in the source history')
        object_evidence = '; ' + verify_rewritten_trees(original, oid, parents)
    caption = merge_caption(pr, parents)
    if not caption['recognized']:
        raise ReviewNeeded('caption has no corroborating commit inventory')
    # Comparing to the renderer also validates the exact vocabulary, grouping
    # and normalized human wording. Remove only repeated references to this PR.
    local = old.removeprefix(f'Merge #{number} '.encode())
    suffix = f' (#{number})'.encode()
    while local.endswith(suffix):
        local = local[:-len(suffix)]
    valid_titles = {caption['title'].encode()}
    if 'previous_title' in caption:
        valid_titles.add(caption['previous_title'].encode())
    if local not in valid_titles:
        raise ReviewNeeded('caption does not match the merged PR title and incoming inventory')
    return pr, caption, 'unique caption matches merged PR title and incoming inventory' + object_evidence


class Proposal(NamedTuple):
    subject: Optional[bytes] = None
    evidence: Optional[str] = None
    context: Optional[bytes] = None
    landing: Optional[int] = None


def proposal(oid, parents, message, prs):
    old = message.split(b'\n', 1)[0]
    if len(parents) == 2:
        landing = verified_landing(oid, parents, message, prs)
        if landing:
            pr, result, evidence = landing
            number = pr['number']
            # The PR record supplies names even if the branch refs have been deleted.
            owner = (pr['headRepositoryOwner'] or {}).get('login')
            source = (owner + ':' if owner else '') + pr['headRefName']
            route = f"Merged from {source} into {pr['baseRefName']} (PR #{number}).".encode()
            return Proposal(result['subject'].encode(), evidence, route, number)
    matches = prs.by_oid.get(oid, [])
    if len(matches) > 1:
        return Proposal(evidence='multiple merged PRs claim this commit')
    if matches:
        return Proposal(evidence='one-parent or nonstandard PR landing requires review of the original operation')
    if len(parents) >= 2:
        if local_caption(message):
            return Proposal(subject=old)
        rendered = run('gitmoji', 'merge-message', '--historical', oid)
        if rendered:
            subject, separator, inventory = rendered.rstrip(b'\n').partition(b'\n\n')
            if not separator or not local_caption(rendered):
                raise ValueError('gitmoji merge-message returned an incomplete branch caption')
            checked_title(subject.decode('utf-8'), 'branch merge')
            return Proposal(subject, 'recorded branch direction and incoming Git parents', inventory)
        return Proposal(evidence='merge without a unique two-parent PR landing or recorded branch direction')
    if len(parents) == 1 and UNDO.match(old):
        if re.match(rb'^(?:Revert|Reapply) #[0-9]+ [^\n]+\n\nSource commit: [0-9a-f]+\nTarget:', message):
            return Proposal(evidence='already captioned undo; retain its original provenance')
        pr, depth, target_parents, subject, kind = revert_origin(oid, parents[0], message, prs)
        verb = b'Revert' if depth % 2 else b'Reapply'
        if subject is None:
            subject = merge_caption(pr, target_parents)['title']
        elif kind == 'landing commit':
            subject = landing_title(subject, pr, oid)
        caption = verb + f' #{pr["number"]} {subject}'.encode()
        route = f'Target: {kind} of PR #{pr["number"]}.'
        introducing = prs.containing_pr(oid)
        if introducing and introducing['number'] != pr['number']:
            route += f' Introduced by PR #{introducing["number"]}.'
        return Proposal(caption, 'exact inverse and verified target PR', route.encode())
    if UNDO.match(old) or GENERATED.match(old) or REVERT_TARGET.search(message):
        return Proposal(evidence='requires review of the original operation')
    return Proposal()


def candidate_inventory(inventory, parents, remapped, new_subjects):
    # Limit edits to list entries that match the original first-parent arrival
    # at that position, including its subject. A contributor can edit the body.
    rows = inventory.split(b'\n')
    original = git('log', '--first-parent', '--topo-order', '--no-patch', '--no-color',
                   '--no-notes', '--no-show-signature', '--encoding=UTF-8', '-z', '--format=%H%x00%s',
                   f'--max-count={len(rows) - 1}', '^' + parents[0], *parents[1:], '--').split(b'\0')
    for i, (full, subject) in enumerate(zip(original[::2], original[1::2]), start=1):
        oid = full.decode('ascii')
        if oid not in remapped or not rows[i].startswith(b'- '):
            continue
        short, separator, listed_subject = rows[i][2:].partition(b' ')
        if separator and len(short) >= 4 and full.startswith(short) and listed_subject == subject:
            # A full candidate ID also works in the read-only preview, before
            # the new object exists for Git to abbreviate.
            rows[i] = b'- ' + remapped[oid].encode() + b' ' + new_subjects.get(oid, subject)
    return b'\n'.join(rows)


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
    prs.index_captions(tip)
    title_preview = args.update_pr_titles or args.apply_pr_titles
    title_updates = []
    already_correct = unmatched = 0
    remapped = {}
    new_subjects = {}
    changes = []
    signed = []
    blocked = []
    for oid in commits:
        fields, parents, message = read_commit(oid)
        subject = message.split(b'\n', 1)[0]
        try:
            change = proposal(oid, parents, message, prs)
        except ReviewNeeded as error:
            change = Proposal(evidence=str(error))
        new, reason, route, landing_number = change
        inventory_updated = False
        if title_preview and new is None and (len(parents) >= 2 or oid in prs.by_oid):
            unmatched += 1
        if title_preview and landing_number is not None:
            title = new.decode().removesuffix(f' (#{landing_number})')
            pr = prs.by_number[landing_number]
            if title != pr['title']:
                title_updates.append({'number': pr['number'], 'merge_commit': pr['mergeCommit']['oid'],
                                      'source_commit': oid, 'before': pr['title'], 'after': title,
                                      'evidence': reason})
            else:
                already_correct += 1
        if new is not None and new != subject:
            new_subjects[oid] = new
            # Keep the original subject and complete body, including conflict notes and trailers.
            provenance = b'Source commit: ' + oid.encode() + b'\n'
            if route:
                if len(parents) >= 2 and landing_number is None:
                    # The first body paragraph also identifies a local merge when its
                    # summary ends with a reference to one of the arriving PRs.
                    inventory = candidate_inventory(route, parents, remapped, new_subjects) if remapped else route
                    provenance = inventory + b'\n\n' + provenance
                else:
                    provenance += route + b'\n'
            targets = REVERT_TARGET.findall(message)
            if len(targets) == 1 and targets[0].decode() in remapped:
                provenance += b'Candidate undo target: ' + remapped[targets[0].decode()].encode() + b'\n'
            message = new + b'\n\n' + provenance + b'\n' + message
        elif not any(p in remapped for p in parents):
            if new is None and reason:
                changes.append({'old': oid, 'subject': subject.decode(errors='replace'), 'review': reason})
            continue
        elif new == subject and landing_number is None and len(parents) >= 2:
            # A live caption (or one from a previous upgrade) already has a
            # user-visible heading. Only its managed Arrived paragraph may change.
            head, _, body = message.partition(b'\n\n')
            inventory, gap, remainder = body.partition(b'\n\n')
            updated = candidate_inventory(inventory, parents, remapped, new_subjects)
            if updated != inventory:
                message = head + b'\n\n' + updated + gap + remainder
                inventory_updated = True
        rebuilt = []
        for field in fields:
            key = field.split(b' ', 1)[0]
            if key in (b'gpgsig', b'gpgsig-sha256'):
                if oid not in signed:
                    signed.append(oid)
                continue
            if key == b'mergetag':
                blocked.append(oid)
            if key == b'encoding' and (inventory_updated or (new is not None and new != subject)):
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
        elif inventory_updated:
            entry.update(inventory_updated=True, evidence='arriving commit rewritten in this candidate')
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
    result = {'repository': args.hostname + '/' + args.repo, 'source_tip': tip,
            'candidate_tip': remapped.get(tip, tip), 'created_branch': args.apply,
            'review_or_changed_commits': changes, 'invalidated_signatures': signed,
            'blocked_mergetags_or_encodings': blocked}
    if title_preview:
        result['pr_title_summary'] = {'to_change': len(title_updates), 'already_correct': already_correct,
                                      'unmatched': unmatched}
        result['pr_title_updates'] = title_updates
        result['base'] = base if args.base else None
    if args.apply_pr_titles:
        reviewed = json.loads(Path(args.apply_pr_titles).read_text(encoding='utf-8'))
        result['updated_pr_titles'] = apply_pr_titles(prs, reviewed, result)
    return result


def apply_pr_titles(prs, reviewed, fresh):
    for key in ('repository', 'source_tip', 'base', 'candidate_tip'):
        if key not in reviewed or reviewed[key] != fresh[key]:
            raise ValueError(f'reviewed {key} changed; generate a fresh PR title preview')
    plan = reviewed.get('pr_title_updates')
    if not isinstance(plan, list):
        raise ValueError('use a reviewed --update-pr-titles JSON preview')
    expected = {row['number']: row for row in fresh['pr_title_updates']}
    numbers = set()
    pending = []
    for row in plan:
        number = row['number']
        if type(number) is not int or number in numbers or number not in prs.by_number:
            raise ValueError('invalid or duplicate merged PR in title plan')
        numbers.add(number)
        pr = prs.by_number[number]
        if not pr['mergeCommit'] or pr['mergeCommit']['oid'] != row['merge_commit']:
            raise ValueError(f'PR #{number} landing changed; generate a new preview')
        if pr['title'] == row['after'] and number not in expected:
            continue  # Already applied; a partial run can be retried.
        if row != expected.pop(number, None):
            raise ValueError(f'PR #{number} title plan changed; generate a new preview')
        pending.append(row)
    if expected:
        raise ValueError('additional PR title changes found; generate a new preview')
    updated = []
    for row in pending:
        number = row['number']
        path = f'repos/{prs.repo}/pulls/{number}'
        api = ('gh', 'api', '--hostname', prs.host)
        try:
            live = json.loads(run(*api, path))
            if (live['number'] != number or live['state'] != 'closed' or not live['merged_at']
                    or live['base']['repo']['full_name'].casefold() != prs.repo.casefold()
                    or live['merge_commit_sha'] != row['merge_commit']):
                raise ValueError('merged PR identity changed')
            if live['title'] == row['after']:
                continue
            if live['title'] != row['before']:
                raise ValueError('title changed since preview')
            # REST does not accept an expected title. Re-read immediately before
            # PATCH, send only the title, and verify the server's response.
            response = json.loads(run(*api, '--method', 'PATCH', path, '-f', 'title=' + row['after']))
            if response['number'] != number or response['title'] != row['after']:
                raise ValueError('server returned a different title')
        except (ValueError, KeyError, TypeError, OSError) as error:
            raise ValueError(f'PR #{number}: {error}. Inspect the PR and rerun the preview; '
                             f'earlier verified updates: {updated}') from error
        updated.append(number)
    return updated


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
    parser.add_argument('--update-pr-titles', action='store_true',
                        help='include proposed merged PR title updates in the read-only JSON preview')
    parser.add_argument('--apply-pr-titles', metavar='REVIEWED_JSON',
                        help='apply hosted title updates from a reviewed --update-pr-titles preview')
    args = parser.parse_args()
    if args.apply and args.apply_pr_titles:
        parser.error('apply the candidate branch and hosted title changes separately')
    if args.expect_candidate and not args.apply:
        parser.error('--expect-candidate requires --apply')
    if (not re.fullmatch(r'[A-Za-z0-9_-]+/[A-Za-z0-9_.-]+', args.repo)
            or args.repo.split('/')[1] in {'.', '..'}):
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
