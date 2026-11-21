"""Run with python3 -m unittest discover -s package/tests/python."""

import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
from types import SimpleNamespace


SCRIPT = Path(__file__).resolve().parents[3] / 'skills/gitmoji/scripts/gitmoji-upgrade-history.py'
spec = importlib.util.spec_from_file_location('gitmoji_upgrade', SCRIPT)
upgrade = importlib.util.module_from_spec(spec)
spec.loader.exec_module(upgrade)


class UpgradeHistory(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        old = os.getcwd()
        os.chdir(directory.name)
        self.addCleanup(os.chdir, old)
        self.git('init', '-q', '-b', 'main')
        self.git('config', 'user.name', 'Contributor')
        self.git('config', 'user.email', 'test@example.org')
        self.git('config', 'commit.gpgsign', 'false')
        self.git('config', 'core.hooksPath', os.devnull)
        self.git('commit', '--allow-empty', '-qm', 'root')
        self.root = self.git('rev-parse', 'HEAD')
        self.git('switch', '-qc', 'topic')
        Path('feature').write_text('one')
        self.git('add', 'feature')
        self.git('commit', '-qm', '🎨✨ #9 (preview): add preview')
        self.member = self.git('rev-parse', 'HEAD')
        self.git('switch', '-q', 'main')
        self.git('merge', '--no-ff', 'topic', '-m', 'Merge pull request #7 from fork/topic',
                 '-m', 'Original notes.\n\nReviewed-by: A Reviewer')
        self.merge = self.git('rev-parse', 'HEAD')
        self.pr = {'number': 7, 'title': 'Show preview', 'mergeCommit': {'oid': self.merge},
                   'headRepositoryOwner': {'login': 'fork'}, 'headRefName': 'topic', 'baseRefName': 'main'}
        self.prs = [self.pr]
        self.members = {7: [self.member]}
        self.patches = []
        self.live_overrides = {}
        command = upgrade.run

        def fake_run(*args, data=None):
            if args[0] == 'gh':
                if 'graphql' not in args:
                    self.assertEqual(args[args.index('--hostname') + 1], 'git.example.com')
                    path = next(arg for arg in args if arg.startswith('repos/'))
                    self.assertRegex(path, r'^repos/sample/repo/pulls/[0-9]+$')
                    number = int(path.rsplit('/', 1)[1])
                    pr = next(pr for pr in self.prs if pr['number'] == number)
                    record = {'number': number, 'title': pr['title'], 'state': 'closed',
                              'merged_at': '2024-10-01', 'merge_commit_sha': pr['mergeCommit']['oid'],
                              'base': {'repo': {'full_name': 'sample/repo'}}}
                    record.update(self.live_overrides.get(number, {}))
                    if '--method' in args:
                        self.assertEqual(args[args.index('--method') + 1], 'PATCH')
                        title = args[args.index('-f') + 1].removeprefix('title=')
                        record['title'] = pr['title'] = title
                        self.patches.append((number, title))
                    return json.dumps(record).encode()
                query = args[args.index('graphql') + 4]
                self.assertEqual(args[args.index('--hostname') + 1], 'git.example.com')
                if query == 'query=' + upgrade.QUERY:
                    page = {'nameWithOwner': 'sample/repo', 'pullRequests': {'nodes': self.prs}}
                elif query == 'query=' + upgrade.ASSOCIATIONS:
                    page = {'object': {'associatedPullRequests': {'nodes': [
                        {'number': pr['number'], 'repository': {'nameWithOwner': 'sample/repo'}}
                        for pr in self.prs]}}}
                elif query == 'query=' + upgrade.MEMBERSHIP:
                    number = int(next(arg[7:] for arg in args if arg.startswith('number=')))
                    page = {'pullRequest': {'commits': {'nodes': [
                        {'commit': {'oid': oid}} for oid in self.members.get(number, [])]}}}
                else:
                    self.fail(query)
                return json.dumps([{'data': {'repository': page}}]).encode()
            if args[0] == 'gitmoji':
                # CI builds the real command before these tests. Only hosted PR metadata is simulated.
                binary = SCRIPT.parents[3] / 'package/target/debug' / ('gitmoji.exe' if os.name == 'nt' else 'gitmoji')
                return command(str(binary) if binary.is_file() else 'gitmoji', *args[1:], data=data)
            return command(*args, data=data)
        mock = patch.object(upgrade, 'run', side_effect=fake_run)
        mock.start()
        self.addCleanup(mock.stop)

    def git(self, *args):
        return subprocess.check_output(['git', *args], stderr=subprocess.DEVNULL).decode().strip()

    def execute(self, **options):
        params = dict(repo='sample/repo', hostname='git.example.com', ref='HEAD', base=None,
                      apply=None, allow_unsigned=False, expect_candidate=None,
                      update_pr_titles=False, apply_pr_titles=None)
        return upgrade.make_candidate(SimpleNamespace(**(params | options)))

    def test_dry_run_then_candidate_preserve_graph_and_body(self):
        self.git('revert', '--no-edit', '-m', '1', self.merge)
        self.git('revert', '--no-edit', 'HEAD')
        original = self.git('rev-parse', 'HEAD')
        before = self.git('count-objects', '-v')
        objects = self.git('show-ref')
        report = self.execute()
        self.assertEqual(self.git('count-objects', '-v'), before)
        self.assertEqual(self.git('show-ref'), objects)
        self.assertEqual(len(report['review_or_changed_commits']), 3)
        candidate = self.execute(apply='candidate', expect_candidate=report['candidate_tip'])
        new = self.git('rev-parse', 'candidate')
        self.assertEqual(new, report['candidate_tip'])
        self.assertEqual(self.git('rev-parse', 'HEAD'), original)
        self.assertEqual(candidate['candidate_tip'], new)
        self.assertEqual(self.git('log', '-1', '--format=%s', 'candidate'), 'Reapply #7 [`🎨✨`] Show preview')
        self.assertEqual(self.git('log', '-1', '--format=%s', 'candidate~1'), 'Revert #7 [`🎨✨`] Show preview')
        msg = self.git('log', '-1', '--format=%B', 'candidate~2')
        self.assertIn('Merged from fork:topic into main (PR #7).', msg)
        self.assertTrue(msg.endswith(self.git('log', '-1', '--format=%B', self.merge)))
        for i in range(3):
            self.assertEqual(self.git('show', '-s', '--format=%T %at %ct', f'candidate~{i}'),
                             self.git('show', '-s', '--format=%T %at %ct', f'HEAD~{i}'))
        self.assertEqual(self.git('rev-parse', 'candidate~2^2'), self.member)
        with self.assertRaises(ValueError):
            self.execute(apply='candidate')
        self.assertEqual(self.git('rev-parse', 'candidate'), new)

    def test_native_and_legacy_landing_subjects_converge(self):
        landed = '[`🎨✨`] Show preview (#7)'
        for existing in ('Merge #7 [`🎨✨`] Show preview', landed):
            with self.subTest(existing=existing):
                self.git('reset', '--hard', self.merge)
                self.git('commit', '-q', '--amend', '-m', existing, '-m', 'Keep these notes.')
                merge = self.git('rev-parse', 'HEAD')
                self.pr['mergeCommit']['oid'] = merge
                self.pr['title'] = '[`🎨✨`] Show preview'
                report = self.execute()
                if existing == landed:
                    self.assertEqual(report['candidate_tip'], merge)
                else:
                    self.assertEqual(report['review_or_changed_commits'][-1]['subject_new'], landed)
                self.git('revert', '--no-edit', '-m', '1', merge)
                self.git('revert', '--no-edit', 'HEAD')
                rows = self.execute(base=merge)['review_or_changed_commits']
                self.assertEqual([r['subject_new'] for r in rows],
                                 ['Revert #7 [`🎨✨`] Show preview', 'Reapply #7 [`🎨✨`] Show preview'])

    def test_rewritten_legacy_title_recovered_with_and_without_old_objects(self):
        self.git('commit', '-q', '--amend', '-m', 'Merge #7 [`🎨✨`] Show preview (#7) (#7)')
        rewritten = self.git('rev-parse', 'HEAD')
        self.git('revert', '--no-edit', '-m', '1', rewritten)
        for hosted_oid in (self.merge, '1' * 40):
            with self.subTest(hosted_oid=hosted_oid):
                self.pr['mergeCommit']['oid'] = hosted_oid
                report = self.execute(update_pr_titles=True)
                landing, undo = report['review_or_changed_commits']
                self.assertEqual(landing['subject_new'], '[`🎨✨`] Show preview (#7)')
                self.assertIn('unique caption', landing['evidence'])
                self.assertEqual(undo['subject_new'], 'Revert #7 [`🎨✨`] Show preview')
                self.assertEqual(report['pr_title_updates'][0]['after'], '[`🎨✨`] Show preview')
        self.assertEqual(self.patches, [])

    def test_fallback_rejects_wrong_title_inventory_ambiguous_or_unmerged_pr(self):
        for subject in ('Merge #7 [`🎨✨`] Other wording', 'Merge #7 [`🧩✨`] Show preview',
                        'Merge #7 [WIP] Show preview', 'Show preview (#7)',
                        'Merge #100 [`🎨✨`] Show preview'):
            with self.subTest(subject=subject):
                self.git('commit', '-q', '--amend', '-m', subject)
                report = self.execute(update_pr_titles=True)
                self.assertEqual(report['candidate_tip'], report['source_tip'])
                self.assertFalse(report['pr_title_updates'])
                self.assertIn('review', report['review_or_changed_commits'][0])
        self.git('commit', '-q', '--amend', '-m', 'Merge #7 [`🎨✨`] Show preview')
        self.pr['mergeCommit']['oid'] = self.member
        self.assertIn('already in', self.execute()['review_or_changed_commits'][-1]['review'])
        self.pr['mergeCommit']['oid'] = '0' * 40
        first = self.git('rev-parse', 'HEAD')
        self.git('switch', '-qc', 'second')
        self.git('commit', '-q', '--allow-empty', '-m', 'Another change')
        self.git('switch', '-q', 'main')
        self.git('merge', '--no-ff', 'second', '-qm', '[`🎨✨`] Show preview (#7)')
        report = self.execute(base=first, update_pr_titles=True)
        self.assertIn('multiple local merges', report['review_or_changed_commits'][0]['review'])
        self.assertFalse(report['pr_title_updates'])

    def test_rewritten_caption_refuses_shared_old_id_and_nonmatching_original_trees(self):
        self.git('commit', '-q', '--amend', '-m', 'Merge #7 [`🎨✨`] Show preview')
        self.prs.append({**self.pr, 'number': 42})
        self.assertIn('multiple merged PRs', self.execute()['review_or_changed_commits'][0]['review'])
        self.prs.pop()
        Path('unrelated').write_text('different merge result')
        self.git('add', 'unrelated')
        self.git('commit', '-q', '--amend', '--no-edit')
        row = self.execute()['review_or_changed_commits'][0]
        self.assertIn('different trees', row['review'])
        self.assertNotIn('subject_new', row)

    def test_hosted_updates_use_landed_wording_and_skip_rewrites_on_retry(self):
        self.git('commit', '-q', '--amend', '-m', '[`🎨✨`] Landed wording (#7)')
        self.pr['mergeCommit']['oid'] = self.git('rev-parse', 'HEAD')
        plan = self.execute(update_pr_titles=True)
        self.assertEqual(plan['candidate_tip'], plan['source_tip'])
        self.assertEqual(len(plan['pr_title_updates']), 1)
        Path('review.json').write_text(json.dumps(plan))
        result = self.execute(apply_pr_titles='review.json')
        self.assertEqual(result['updated_pr_titles'], [7])
        self.assertEqual(self.patches, [(7, '[`🎨✨`] Landed wording')])
        self.assertEqual(self.execute(apply_pr_titles='review.json')['updated_pr_titles'], [])
        self.assertEqual(len(self.patches), 1)
        self.assertEqual(self.git('rev-parse', 'HEAD'), plan['source_tip'])

    def test_hosted_updates_refuse_stale_cross_repo_or_forged_plans(self):
        plan = self.execute(update_pr_titles=True)
        path = Path('plan.json')
        for overrides in ({'title': 'Someone renamed this'}, {'state': 'open'},
                          {'merged_at': None}, {'merge_commit_sha': self.member},
                          {'base': {'repo': {'full_name': 'fork/repo'}}}):
            with self.subTest(overrides=overrides):
                path.write_text(json.dumps(plan))
                self.live_overrides[7] = overrides
                with self.assertRaisesRegex(ValueError, 'PR #7'):
                    self.execute(apply_pr_titles=path)
        self.live_overrides.clear()
        for modified in ({**plan, 'repository': 'git.example.com/fork/repo'},
                         {**plan, 'pr_title_updates': [{**plan['pr_title_updates'][0], 'after': 'Invented'}]},
                         {**plan, 'pr_title_updates': []}):
            path.write_text(json.dumps(modified))
            with self.assertRaises(ValueError):
                self.execute(apply_pr_titles=path)
        self.assertFalse(self.patches)

    def test_one_parent_landing_subject_needs_review_and_undo_names_target_once(self):
        self.git('switch', '-q', 'topic')
        for existing, expected in (('Merge #7 [`🎨✨`] Show preview', '[`🎨✨`] Show preview'),
                                   ('[`🎨✨`] Show preview (#7)', '[`🎨✨`] Show preview'),
                                   ('Show preview (#12) (#7)', 'Show preview (#12)'),
                                   ('Merge #7 into release', 'Merge #7 into release')):
            with self.subTest(existing=existing):
                self.git('commit', '-q', '--amend', '-m', existing)
                target = self.git('rev-parse', 'HEAD')
                self.pr['mergeCommit']['oid'] = target
                self.git('revert', '--no-edit', target)
                rows = self.execute()['review_or_changed_commits']
                self.assertIn('one-parent', rows[0]['review'])
                self.assertEqual(rows[1]['subject_new'], 'Revert #7 ' + expected)
                self.git('reset', '--hard', target)

    def test_untagged_merge_and_undo_carry_the_pr_number_once(self):
        self.git('switch', '-q', 'topic')
        self.git('commit', '-q', '--amend', '-m', 'Add preview')
        self.git('switch', '-q', 'main')
        self.git('reset', '--hard', self.root)
        self.git('merge', '--no-ff', 'topic', '-qm', 'Merge the preview')
        merge = self.git('rev-parse', 'HEAD')
        self.pr['mergeCommit']['oid'] = merge
        self.pr['title'] = 'Show preview (#7)'
        self.git('revert', '--no-edit', '-m', '1', merge)
        rows = self.execute()['review_or_changed_commits']
        self.assertEqual([row['subject_new'] for row in rows], ['Show preview (#7)', 'Revert #7 Show preview'])

    def test_member_revert_can_reference_pr_without_implying_whole_pr(self):
        self.git('revert', '--no-edit', self.member)
        report = self.execute(base=self.merge)
        entries = report['review_or_changed_commits']
        self.assertEqual(len(entries), 1)
        self.assertIn('🎨✨ #9 (preview): add preview', entries[-1]['subject_new'])
        self.assertTrue(entries[-1]['subject_new'].startswith('Revert #7 '))

    def test_ambiguous_missing_and_keyword_only(self):
        self.prs.append({**self.pr, 'number': 8})
        self.git('commit', '--allow-empty', '-qm', 'Rewind #7 to the old build')
        self.git('commit', '--allow-empty', '-qm', 'Merge #7 looks reasonable')
        result = self.execute()
        self.assertEqual(result['source_tip'], result['candidate_tip'])
        for entry in result['review_or_changed_commits']:
            self.assertIn('review', entry)
        self.assertEqual(len(result['review_or_changed_commits']), 3)
        self.prs.clear()
        self.assertIn('merge without', self.execute()['review_or_changed_commits'][0]['review'])

    def test_refuses_unreviewed_candidate_and_keeps_original_revert_targets(self):
        self.git('revert', '--no-edit', '-m', '1', self.merge)
        original_revert = self.git('rev-parse', 'HEAD')
        self.git('revert', '--no-edit', 'HEAD')
        with self.assertRaisesRegex(ValueError, 'expect-candidate'):
            self.execute(apply='guarded', expect_candidate='1' * 40)
        self.assertNotIn('guarded', self.git('show-ref'))
        self.execute(apply='candidate')
        body = self.git('log', '-1', '--format=%B', 'candidate')
        self.assertIn('This reverts commit ' + original_revert + '.', body)
        self.assertIn('Candidate undo target: ' + self.git('rev-parse', 'candidate^'), body)

    def test_merge_revert_needs_verified_mainline(self):
        text = f'This reverts commit {self.merge}, reversing\nchanges made to {self.member}.'
        self.git('commit', '--allow-empty', '-qm', 'Rewind the merge', '-m', text)
        report = self.execute(base=self.merge)
        self.assertIn('review', report['review_or_changed_commits'][0])
        self.assertEqual(report['source_tip'], report['candidate_tip'])

    def test_mixed_undo_and_its_reapply_need_review(self):
        self.git('revert', '--no-edit', '-m', '1', self.merge)
        Path('export').write_bytes(b'\0additional feature')
        self.git('add', 'export')
        self.git('commit', '-q', '--amend', '--no-edit')
        mixed = self.git('rev-parse', 'HEAD')
        self.git('revert', '--no-edit', mixed)
        for entry in self.execute(base=self.merge)['review_or_changed_commits']:
            self.assertIn('exact inverse', entry['review'])
        self.prs.append({**self.pr, 'number': 8, 'title': 'Undo preview and add export',
                         'mergeCommit': {'oid': mixed}})
        entries = self.execute(base=self.merge)['review_or_changed_commits']
        self.assertIn('one-parent', entries[0]['review'])
        self.assertIn('ambiguous', entries[1]['review'])

    def test_undo_finds_introducing_pr_and_preserves_other_changes(self):
        self.git('switch', '-qc', 'rollback')
        Path('elsewhere').write_text('keep me')
        self.git('add', 'elsewhere')
        self.git('commit', '-qm', 'publish a separate file')
        self.git('revert', '--no-edit', '-m', '1', self.merge)
        undo = self.git('rev-parse', 'HEAD')
        self.git('switch', '-q', 'main')
        self.git('merge', '--no-ff', 'rollback', '-qm', 'Merge rollback')
        self.prs.append({**self.pr, 'number': 8, 'title': 'Rollback',
                         'mergeCommit': {'oid': self.git('rev-parse', 'HEAD')}})
        self.members[8] = [undo]
        report = self.execute(base=self.merge, apply='with-provenance')
        entry = next(e for e in report['review_or_changed_commits'] if e['old'] == undo)
        self.assertEqual(entry['subject_new'], 'Revert #7 [`🎨✨`] Show preview')
        self.assertIn('Target: merge of PR #7. Introduced by PR #8.',
                      self.git('log', '-1', '--format=%B', entry['candidate']))

    def test_changed_blob_after_target_goes_to_review(self):
        file = Path('many-lines')
        file.write_text('before\n' + 'between\n' * 12 + 'tail\n')
        self.git('add', 'many-lines')
        self.git('commit', '-qm', 'initial data')
        file.write_text(file.read_text().replace('before', 'after'))
        self.git('commit', '-qam', 'change first line')
        target = self.git('rev-parse', 'HEAD')
        self.members[7].append(target)
        file.write_text(file.read_text().replace('tail', 'new tail'))
        self.git('commit', '-qam', 'change last line')
        base = self.git('rev-parse', 'HEAD')
        self.git('revert', '--no-edit', target)
        self.assertTrue(file.read_text().endswith('new tail\n'))
        entry = self.execute(base=base)['review_or_changed_commits'][0]
        self.assertIn('exact inverse', entry['review'])

    def test_binary_rename_and_mode_are_verified(self):
        self.git('mv', 'feature', 'renamed')
        Path('renamed').write_bytes(b'\x00\xffbinary')
        self.git('add', 'renamed')
        self.git('update-index', '--chmod=+x', 'renamed')
        self.git('commit', '-qm', 'replace the example with a binary')
        self.git('reset', '--hard', 'HEAD')
        target = self.git('rev-parse', 'HEAD')
        self.members[7].append(target)
        self.git('revert', '--no-edit', target)
        entry = self.execute(base=target)['review_or_changed_commits'][0]
        self.assertEqual(entry['subject_new'], 'Revert #7 replace the example with a binary')

    def test_revert_word_in_target_merge_title_has_no_effect_on_identity(self):
        self.git('commit', '-q', '--amend', '-m', 'Revert old route names')
        merge = self.git('rev-parse', 'HEAD')
        self.pr['mergeCommit']['oid'] = merge
        self.git('revert', '--no-edit', '-m', '1', merge)
        self.assertIn('Revert #7', self.execute(base=merge)['review_or_changed_commits'][0]['subject_new'])

    def test_target_legacy_encoding_and_one_parent_landing(self):
        self.git('switch', '-q', 'topic')
        raw = upgrade.git('cat-file', 'commit', self.member)
        header = raw.split(b'\n\n', 1)[0]
        changed = header + b'\nencoding ISO-8859-1\n\nR\xe9parer le r\xe9sum\xe9\n'
        target = upgrade.git('hash-object', '-w', '-t', 'commit', '--stdin', data=changed).decode().strip()
        self.git('reset', '--hard', target)
        self.pr['mergeCommit']['oid'] = target
        self.git('revert', '--no-edit', target)
        report = self.execute(base=target, apply='latin-undo')
        entry = report['review_or_changed_commits'][0]
        self.assertEqual(entry['subject_new'], 'Revert #7 Réparer le résumé')
        body = self.git('log', '-1', '--format=%B', entry['candidate'])
        self.assertIn('Target: landing commit of PR #7.', body)

    def test_invalid_target_subject_is_reported_without_replacement_characters(self):
        self.git('switch', '-q', 'topic')
        header = upgrade.git('cat-file', 'commit', self.member).split(b'\n\n', 1)[0]
        for original in (b'', b'invalid \xff'):
            with self.subTest(original=original):
                target = upgrade.git('hash-object', '-w', '-t', 'commit', '--stdin',
                                     data=header + b'\n\n' + original).decode().strip()
                self.git('reset', '-q', '--hard', target)
                self.members[7] = [target]
                subprocess.run(['git', 'revert', '--no-edit', target], capture_output=True, check=True)
                entry = self.execute(base=target)['review_or_changed_commits'][0]
                self.assertIn('review', entry)
                self.assertNotIn('subject_new', entry)

    def test_reserved_branch_names_keep_worktrees_and_refs_intact(self):
        # --orphan on switch also works on versions without worktree add --orphan.
        self.git('worktree', 'add', '-q', '--detach', 'linked', self.merge)
        self.git('-C', 'linked', 'switch', '--orphan', 'occupied')
        before = self.git('-C', 'linked', 'status', '--porcelain=v1', '-b')
        with patch.object(upgrade, 'PullRequests', side_effect=AssertionError('preflight must run first')):
            with self.assertRaisesRegex(ValueError, 'worktree'):
                self.execute(apply='occupied')
            with self.assertRaisesRegex(ValueError, 'already exists'):
                self.execute(apply='topic')
        self.assertEqual(self.git('-C', 'linked', 'status', '--porcelain=v1', '-b'), before)
        self.git('symbolic-ref', 'refs/heads/alias', 'refs/heads/missing')
        with self.assertRaisesRegex(ValueError, 'symbolic ref'):
            self.execute(apply='alias')
        self.assertEqual(self.git('symbolic-ref', 'refs/heads/alias'), 'refs/heads/missing')
        self.assertEqual(self.git('rev-parse', 'HEAD'), self.merge)

    def test_json_on_ascii_output_survives_branch_creation(self):
        self.pr['title'] = 'Résumé'
        buf = io.BytesIO()
        with io.TextIOWrapper(buf, encoding='ascii') as stream, patch.object(sys, 'stdout', stream), \
                patch.dict(os.environ), \
                patch.object(sys, 'argv', ['upgrade', '--repo', 'sample/repo', '--hostname',
                                           'git.example.com', '--apply', 'ascii-report']):
            upgrade.main()
            stream.flush()
            self.assertIn('Résumé'.encode(), buf.getvalue())
            report = json.loads(buf.getvalue())
        self.assertEqual(self.git('rev-parse', 'ascii-report'), report['candidate_tip'])
        self.assertEqual(report['review_or_changed_commits'][-1]['subject_new'], '[`🎨✨`] Résumé (#7)')

    def test_unsuitable_historical_pr_title_goes_to_review(self):
        self.pr['title'] = 'Two\tcolumns'
        entry = self.execute()['review_or_changed_commits'][0]
        self.assertIn('review', entry)
        self.assertNotIn('candidate', entry)

    def test_signature_and_descendants(self):
        base = upgrade.git('cat-file', 'commit', self.merge)
        signed = base.replace(b'\n\n', b'\ngpgsig placeholder\n signed-data\n\n', 1)
        oid = upgrade.git('hash-object', '-w', '-t', 'commit', '--stdin', data=signed).decode().strip()
        self.pr['mergeCommit']['oid'] = oid
        report = self.execute(ref=oid)
        self.assertEqual(report['invalidated_signatures'], [oid])
        with self.assertRaisesRegex(ValueError, 'signatures'):
            self.execute(ref=oid, apply='signed-candidate')
        self.assertNotIn('signed-candidate', self.git('show-ref'))
        candidate = self.execute(ref=oid, apply='unsigned-candidate', allow_unsigned=True)
        self.assertNotIn(b'gpgsig ', upgrade.git('cat-file', 'commit', candidate['candidate_tip']))
        self.assertIn(b'gpgsig ', upgrade.git('cat-file', 'commit', oid))


if __name__ == '__main__':
    unittest.main()
