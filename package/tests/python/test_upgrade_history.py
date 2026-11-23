"""Exercise native history upgrades against Git objects and a local GitHub Enterprise fixture."""

from http.server import BaseHTTPRequestHandler
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from hosted_tls import LocalTlsFixture


BINARY = (Path(__file__).resolve().parents[2] / 'target/debug' /
          ('gitmoji.exe' if os.name == 'nt' else 'gitmoji'))
REPOSITORY = 'sample/repo'


def git_bytes(*args, data=None):
    return subprocess.check_output(['git', *args], input=data)


def commit_message(oid):
    return git_bytes('cat-file', 'commit', oid).split(b'\n\n', 1)[1]


class HistoryHost(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def request_json(self):
        return json.loads(self.rfile.read(int(self.headers['Content-Length'])))

    def send_json(self, value):
        data = json.dumps(value).encode()
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Content-Length', str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_POST(self):
        if self.path != '/api/graphql':
            return self.send_error(404)
        request = self.request_json()
        query, variables = request['query'], request['variables']
        if (variables.get('owner'), variables.get('name')) != ('sample', 'repo'):
            return self.send_error(404)
        fixture = self.server.fixture
        fixture.requests.append(('POST', self.path))

        def connection(nodes):
            # Use two pages for multi-PR fixtures to exercise real cursor transport.
            offset = int(variables.get('endCursor', 0))
            following = offset + 1
            return {'nodes': nodes[offset:following], 'pageInfo': {
                'hasNextPage': following < len(nodes), 'endCursor': str(following)}}

        if 'associatedPullRequests' in query:
            associations = [{'number': pr['number'], 'repository': {'nameWithOwner': REPOSITORY}}
                            for pr in fixture.prs]
            repo = {'object': {'associatedPullRequests': connection(associations)}}
        elif 'commits(' in query:
            commits = [{'commit': {'oid': oid}} for oid in fixture.members.get(variables['number'], [])]
            repo = {'pullRequest': {'commits': connection(commits)}}
        elif 'pullRequests(' in query:
            repo = {'nameWithOwner': REPOSITORY, 'pullRequests': connection(fixture.prs)}
        else:
            return self.send_error(400, 'Unexpected history query')
        self.send_json({'data': {'repository': repo}})

    def pull(self):
        match = re.fullmatch(r'/api/v3/repos/sample/repo/pulls/([0-9]+)', self.path)
        if match:
            self.server.fixture.requests.append((self.command, self.path))
            return next((p for p in self.server.fixture.prs if p['number'] == int(match[1])), None)
        return None

    def do_GET(self):
        pr = self.pull()
        if pr is None:
            return self.send_error(404)
        number = pr['number']
        record = {'number': number, 'title': pr['title'], 'state': 'closed', 'merged_at': '2024-10-01',
                  'merge_commit_sha': pr['mergeCommit']['oid'], 'base': {'repo': {'full_name': REPOSITORY}}}
        record.update(self.server.fixture.live_overrides.get(number, {}))
        self.send_json(record)

    def do_PATCH(self):
        pr = self.pull()
        if pr is None:
            return self.send_error(404)
        title = self.request_json()['title']
        self.server.fixture.patches.append((pr['number'], title))
        pr['title'] = title
        self.send_json({'number': pr['number'], 'title': title})


class UpgradeHistory(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        fixture = LocalTlsFixture(HistoryHost)
        cls.addClassCleanup(fixture.close)
        cls.host, cls.ca = fixture.server, fixture.ca_crt

    def setUp(self):
        self.enterContext(patch.dict(os.environ, {'GIT_CONFIG_NOSYSTEM': '1', 'GIT_CONFIG_GLOBAL': os.devnull}))
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
        self.requests = []
        self.host.fixture = self

    def git(self, *args):
        return subprocess.check_output(['git', *args], stderr=subprocess.DEVNULL).decode().strip()

    def execute(self, **options):
        hostname = f'localhost:{self.host.server_port}'
        command = [BINARY, 'history', '--upgrade', '--repo', REPOSITORY, '--hostname', hostname]
        for key, value in options.items():
            command.append('--' + key.replace('_', '-'))
            if value is not True:
                command.append(str(value))
        env = {**os.environ, 'GH_ENTERPRISE_TOKEN': 'local-fixture',
               'SSL_CERT_FILE': str(self.ca), 'NO_PROXY': 'localhost', 'no_proxy': 'localhost'}
        result = subprocess.run(command, env=env, capture_output=True)
        if result.returncode:
            raise ValueError(result.stderr.decode())
        return json.loads(result.stdout)

    def test_verified_landing_preserves_review_commit_and_its_inner_inventory(self):
        self.git('switch', '-qc', 'review-topic')
        message = '👌 <`🧩🛡️🚧`> #9 (preview): reject duplicate entries'
        self.git('commit', '--allow-empty', '-qm', message, '-m', 'Review found duplicate saves.')
        review = self.git('rev-parse', 'HEAD')
        self.git('switch', '-q', 'main')
        self.git('merge', '--no-ff', 'review-topic', '-qm', 'Merge pull request #8 from fork/review-topic')
        landing = self.git('rev-parse', 'HEAD')
        self.prs.append({**self.pr, 'number': 8, 'title': 'Protect preview saves',
                         'headRefName': 'review-topic', 'mergeCommit': {'oid': landing}})
        self.members[8] = [review]
        report = self.execute()
        self.assertFalse(any('review' in entry for entry in report['review_or_changed_commits']))
        self.execute(apply='candidate')
        self.assertEqual(self.git('show', '-s', '--format=%s', 'candidate'),
                         '🚧 [`🧩🛡️`] Protect preview saves (#8)')
        self.assertEqual(self.git('show', '-s', '--format=%s', 'candidate^2'), message)
        # The prior landing was recaptioned too, so the new parent changes the
        # review commit's ID. Its complete authored message is still byte-for-byte equal.
        self.assertEqual(commit_message('candidate^2'), commit_message(review))

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

    def test_local_merges_in_a_landed_integration_keep_direction_graph_and_pr_identity(self):
        self.git('commit', '--amend', '-qm', '[`🎨✨`] Show preview (#7)')
        self.merge = self.git('rev-parse', 'HEAD')
        self.pr.update(title='[`🎨✨`] Show preview', mergeCommit={'oid': self.merge})
        self.git('switch', '-qc', 'worker', self.root)
        self.git('commit', '--allow-empty', '-qm', '🧩🐛 #9 (worker): recover a cancelled run')
        self.git('update-ref', 'refs/remotes/origin/worker', 'HEAD')
        self.git('update-ref', 'refs/remotes/origin/main', self.merge)
        self.git('switch', '-qc', 'integration', self.root)
        self.git('commit', '--allow-empty', '-qm', '🧩📝 #9 (worker): explain job recovery')
        self.git('merge', '--no-ff', '--no-edit', 'origin/main')
        sync = self.git('rev-parse', 'HEAD')
        self.git('merge', '--no-ff', '--no-edit', 'origin/worker')
        original = self.git('show', '-s', '--format=%s', 'HEAD')
        self.git('commit', '--amend', '-qm', original, '-m', 'Maintainer notes.\n\nReviewed-by: Colleague')
        worker_merge = self.git('rev-parse', 'HEAD')
        self.git('switch', '-q', 'main')
        self.git('merge', '--no-ff', 'integration', '-qm', 'Merge pull request #8 from fork/integration')
        landing = self.git('rev-parse', 'HEAD')
        self.prs.append({**self.pr, 'number': 8, 'title': 'Recover jobs',
                         'mergeCommit': {'oid': landing}, 'headRefName': 'integration'})
        self.git('branch', '-D', 'worker', 'integration')
        self.git('update-ref', '-d', 'refs/remotes/origin/worker')
        self.git('update-ref', '-d', 'refs/remotes/origin/main')

        preview = self.execute(update_pr_titles=True)
        result = self.execute(apply='repaired', expect_candidate=preview['candidate_tip'])
        edits = {row['old']: row for row in result['review_or_changed_commits']}
        self.assertEqual(set(edits), {sync, worker_merge, landing})
        self.assertEqual(edits[sync]['subject_new'], '[`🎨✨`] main into integration: #7 Show preview')
        self.assertIn('worker into integration: recover a cancelled run', edits[worker_merge]['subject_new'])
        outer = edits[landing]['subject_new']
        self.assertTrue(outer.endswith('Recover jobs (#8)'))
        self.assertIn('🐛', outer)
        self.assertIn('📝', outer)
        self.assertNotIn('✨', outer, 'that work was already reachable on the landing branch')
        self.assertEqual(preview['pr_title_summary'], {'to_change': 1, 'already_correct': 1, 'unmatched': 0})
        self.assertEqual([row['number'] for row in preview['pr_title_updates']], [8])
        for oid, row in edits.items():
            after = row['candidate']
            self.assertEqual(self.git('show', '-s', '--format=%T %an %ae %at %cn %ce %ct', oid),
                             self.git('show', '-s', '--format=%T %an %ae %at %cn %ce %ct', after))
            parents = self.git('show', '-s', '--format=%P', oid).split()
            self.assertEqual(self.git('show', '-s', '--format=%P', after).split(),
                             [edits.get(p, {}).get('candidate', p) for p in parents])
            message = self.git('show', '-s', '--format=%B', after)
            self.assertTrue(message.endswith(self.git('show', '-s', '--format=%B', oid)))
            self.assertNotIn('Conflicts resolved', message)
        self.assertIn('Arrived from worker (1):', self.git('show', '-s', '--format=%B',
                                                           edits[worker_merge]['candidate']))
        # The PR still verifies on a second run when its hosted object is unavailable.
        self.pr['mergeCommit'] = {'oid': '1' * len(self.merge)}
        replay = self.execute(ref='repaired', update_pr_titles=True)
        self.assertEqual(replay['source_tip'], replay['candidate_tip'])
        self.assertFalse(replay['review_or_changed_commits'])
        self.assertEqual(replay['pr_title_summary'], preview['pr_title_summary'])

    def test_older_local_caption_ending_in_an_arriving_pr_number_is_not_a_hosted_landing(self):
        title = '[`🎨✨`] Show preview (#7)'
        self.git('commit', '--amend', '-qm', title)
        landing = self.git('rev-parse', 'HEAD')
        short = self.git('rev-parse', '--short', landing)
        self.git('switch', '-qc', 'integration', self.root)
        self.git('commit', '--allow-empty', '-qm', 'prepare integration')
        self.git('merge', '--no-ff', 'main', '-qm', '[`🎨✨`] main into integration: Show preview (#7)',
                 '-m', f'Arrived from main (1):\n- {short} {title}')

        # With the hosted landing object absent, the historical matcher can only
        # verify case #7 if it excludes the local caption from its claim index.
        self.pr['mergeCommit'] = {'oid': '1' * len(landing)}
        report = self.execute(update_pr_titles=True)
        self.assertEqual(report['source_tip'], report['candidate_tip'])
        self.assertFalse(report['review_or_changed_commits'])
        self.assertEqual(report['pr_title_summary'], {'to_change': 1, 'already_correct': 0, 'unmatched': 0})
        self.assertEqual([edit['source_commit'] for edit in report['pr_title_updates']], [landing])

    def test_arrived_list_links_to_the_recaptioned_commit_in_the_candidate(self):
        self.git('switch', '-qc', 'integration', self.root)
        self.git('commit', '--allow-empty', '-qm', 'prepare integration')
        self.git('merge', '--no-ff', '--no-edit', 'main')
        preview = self.execute()
        self.execute(apply='candidate', expect_candidate=preview['candidate_tip'])
        incoming = self.git('show', '-s', '--format=%H %s', 'candidate^2')
        self.assertTrue(incoming.endswith('[`🎨✨`] Show preview (#7)'))
        self.assertIn('- ' + incoming, self.git('show', '-s', '--format=%B', 'candidate').splitlines())

    def test_live_caption_refreshes_only_its_incoming_row_when_earlier_pr_is_rewritten(self):
        self.git('switch', '-qc', 'integration', self.root)
        self.git('commit', '--allow-empty', '-qm', 'prepare integration')
        self.git('merge', '--no-ff', '--no-commit', 'main')
        path = Path(self.git('rev-parse', '--git-path', 'MERGE_MSG'))
        subprocess.run([BINARY, 'merge-message', str(path), 'merge'], check=True)
        original = path.read_bytes().rstrip(b'\n')
        old_sha = self.git('rev-parse', '--short', self.merge).encode()
        old_row = b'- ' + old_sha + b' Merge pull request #7 from fork/topic'
        self.assertIn(old_row, original)
        # The same text in author notes, outside the managed paragraph, is evidence
        # about the original history and should survive verbatim.
        notes = (b'\n\nMy record of the pre-rewrite state:\n' + old_row +
                 b'\n\nAgent: Example\nMethod: paired review\n')
        header, _, inventory = original.partition(b'\n\n')
        custom_header = header.split(b': ', 1)[0] + b': bring the reviewed preview into the release'
        path.write_bytes(custom_header + b'\n\n' + inventory + b'\n\nConflicted paths:\n- preview.txt' + notes)
        self.git('commit', '--cleanup=verbatim', '-q', '-F', str(path))
        before = commit_message(self.git('rev-parse', 'HEAD'))
        preview = self.execute()
        self.execute(apply='candidate', expect_candidate=preview['candidate_tip'])
        after = commit_message(self.git('rev-parse', 'candidate'))
        new_row = b'- ' + self.git('show', '-s', '--format=%H %s', 'candidate^2').encode()
        self.assertEqual(after, before.replace(old_row, new_row, 1))
        self.assertTrue(after.endswith(notes))
        self.assertTrue(preview['review_or_changed_commits'][-1]['inventory_updated'])
        again = self.execute(ref='candidate')
        self.assertEqual(again['candidate_tip'], again['source_tip'])
        self.assertEqual(again['review_or_changed_commits'], [])

    def test_offline_cli_repairs_unpublished_history_without_gh_or_a_remote(self):
        self.assertEqual(self.git('remote'), '')
        self.git('revert', '--no-edit', '-m', '1', self.merge)
        undo = self.git('rev-parse', 'HEAD')
        self.git('switch', '-qc', 'layout')
        self.git('commit', '--allow-empty', '-qm', '🎨✨ #9 (preview): add layout')
        self.git('switch', '-qc', 'integration', undo)
        self.git('commit', '--allow-empty', '-qm', 'prepare integration')
        self.git('merge', '--no-ff', '--no-edit', 'layout')
        local = self.git('rev-parse', 'HEAD')
        self.git('switch', '-qc', 'assembled', undo)
        self.git('commit', '--allow-empty', '-qm', 'start assembly')
        self.git('merge', '--no-ff', '--no-commit', 'integration')
        path = self.git('rev-parse', '--git-path', 'MERGE_MSG')
        subprocess.run([BINARY, 'merge-message', path, 'merge'], check=True)
        self.git('commit', '--cleanup=verbatim', '-q', '-F', path)
        assembled = self.git('rev-parse', 'HEAD')
        self.git('switch', '-q', 'main')
        self.git('merge', '--no-ff', 'assembled', '-qm', 'Merge pull request #8 from fork/assembled')
        self.git('branch', '-D', 'layout', 'integration', 'assembled')
        tip = self.git('rev-parse', 'HEAD')

        # Direct invocation exercises the installed CLI contract, bypassing the
        # hosted metadata stub used by the other tests. The PATH has no gh.
        tool_dir = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, tool_dir)
        git_path = Path(shutil.which('git'))
        binary_name = 'gitmoji.exe' if os.name == 'nt' else 'gitmoji'
        gitmoji = BINARY
        if os.name == 'nt':
            # Git for Windows finds its runtime relative to its installation.
            tool_path = os.pathsep.join([str(git_path.parent), str(gitmoji.parent)])
        else:
            (tool_dir / git_path.name).symlink_to(git_path)
            (tool_dir / binary_name).symlink_to(gitmoji)
            tool_path = str(tool_dir)
        env = {**os.environ, 'PATH': tool_path, 'GH_HOST': 'unreachable.invalid',
               'GIT_EXEC_PATH': self.git('--exec-path')}
        for tool in ('gh', 'python3', 'python'):
            self.assertIsNone(shutil.which(tool, path=env['PATH']))

        def offline(*args):
            return json.loads(subprocess.check_output(
                [BINARY, 'history', '--upgrade', '--local-merges-only', *args], env=env))

        preview = offline()
        self.assertIsNone(preview['repository'])
        self.assertEqual(preview['mode'], 'local-merges-only')
        self.assertNotIn('pr_title_summary', preview)
        self.assertNotIn('pr_title_updates', preview)
        self.assertEqual(preview['source_tip'], tip)
        applied = offline('--apply', 'offline-candidate', '--expect-candidate', preview['candidate_tip'])
        self.assertEqual(applied['candidate_tip'], preview['candidate_tip'])
        self.assertEqual(self.git('rev-parse', 'HEAD'), tip)
        self.assertEqual(self.git('rev-parse', 'offline-candidate^{tree}'), self.git('rev-parse', tip + '^{tree}'))
        edits = {row['old']: row for row in applied['review_or_changed_commits']}
        self.assertNotIn(self.merge, edits)
        self.assertNotIn(undo, edits)
        self.assertEqual(edits[local]['subject_new'], '[`🎨✨`] layout into integration: add layout')
        self.assertTrue(edits[assembled]['inventory_updated'])
        self.assertIn('- ' + edits[local]['candidate'] + ' ' + edits[local]['subject_new'],
                      self.git('show', '-s', '--format=%B', edits[assembled]['candidate']).splitlines())
        self.assertEqual(commit_message(tip), commit_message(applied['candidate_tip']))
        again = offline('--ref', 'offline-candidate')
        self.assertEqual(again['candidate_tip'], again['source_tip'])
        self.assertFalse(again['review_or_changed_commits'])

    def test_offline_cli_rejects_hosted_options(self):
        for hosted in (['--repo', 'sample/repo'], ['--hostname', 'example.org'],
                       ['--update-pr-titles'], ['--apply-pr-titles', 'unused.json']):
            with self.subTest(hosted=hosted):
                result = subprocess.run([BINARY, 'history', '--upgrade', '--local-merges-only', *hosted],
                                        capture_output=True)
                self.assertEqual(result.returncode, 2)
                self.assertIn(hosted[0].encode(), result.stderr)

    def test_old_nine_slot_nested_landing_can_expand_after_a_history_rewrite(self):
        self.git('switch', '-qc', 'integration', self.root)
        self.git('commit', '--allow-empty', '-qm', 'start integration')
        for pair in ('🧱🔧', '🎨✨', '👷📝', '🎨🛂', '👷♻️', '🧱☑️', '🧱🔊'):
            self.git('commit', '--allow-empty', '-qm', f'{pair} #9 (topic): individual change')
        self.git('merge', '--no-ff', '--no-edit', 'topic')
        self.git('switch', '-q', 'main')
        self.git('reset', '--hard', self.root)
        old_title = '[`🧱🎨👷` / `🔧✨📝🛂♻️`…] Show preview'
        self.git('merge', '--no-ff', 'integration', '-qm', old_title + ' (#7)')
        # As after a previous rewrite in another clone, GitHub's recorded merge
        # object is absent. The complete prior nine-slot rendering still matches.
        self.pr['mergeCommit'] = {'oid': '1' * len(self.merge)}
        plan = self.execute(update_pr_titles=True)
        landing = plan['review_or_changed_commits'][-1]
        self.assertIn('unique caption matches', landing['evidence'])
        self.assertIn('🔊', landing['subject_new'])
        self.assertNotIn('…', landing['subject_new'])
        self.assertEqual(plan['pr_title_summary'], {'to_change': 1, 'already_correct': 0, 'unmatched': 0})

    def test_octopus_branch_merge_is_repaired_without_a_hosted_title(self):
        self.git('switch', '-qc', 'integration', self.root)
        self.git('commit', '--allow-empty', '-qm', 'start integration')
        for branch, pair in [('batch', '🧩✨'), ('preview', '🎨🐛')]:
            self.git('switch', '-qc', branch, self.root)
            Path(branch).write_text(branch)
            self.git('add', branch)
            self.git('commit', '-qm', f'{pair} #9 (topic): show {branch}')
            self.git('update-ref', 'refs/remotes/origin/' + branch, 'HEAD')
        self.git('switch', '-q', 'integration')
        self.git('merge', '--no-ff', '--no-edit', 'origin/batch', 'origin/preview')
        original = self.git('rev-parse', 'HEAD')
        plan = self.execute(apply='octopus', update_pr_titles=True)
        self.assertNotEqual(plan['candidate_tip'], original)
        self.assertEqual(self.git('show', '-s', '--format=%P', original).split(),
                         self.git('show', '-s', '--format=%P', 'octopus').split())
        message = self.git('show', '-s', '--format=%B', 'octopus')
        self.assertIn('batch, preview into integration: 2 commits', message)
        self.assertIn('Arrived from batch, preview (2):', message)
        self.assertEqual(plan['pr_title_summary'], {'to_change': 0, 'already_correct': 0, 'unmatched': 0})
        self.assertEqual(plan['pr_title_updates'], [])

    def test_missing_direction_and_custom_merge_need_review(self):
        self.prs.clear()
        for subject in ("Merge branch 'topic'", 'Merge the assessed work', "Merge branch 'topic' into main manually"):
            with self.subTest(subject=subject):
                self.git('commit', '--amend', '-qm', subject)
                plan = self.execute(update_pr_titles=True)
                self.assertEqual(plan['source_tip'], plan['candidate_tip'])
                self.assertIn('review', plan['review_or_changed_commits'][0])
                self.assertEqual(plan['pr_title_summary']['unmatched'], 1)

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

    def rewrite_fixture(self, change_tree=(), change_author=()):
        """Rewrite the entire reachable DAG in order, preserving the original objects."""
        remapped = {}
        blob = git_bytes('hash-object', '-w', '--stdin', data=b'shared content').strip()
        for old in self.git('rev-list', '--reverse', '--topo-order', self.merge).splitlines():
            header, message = git_bytes('cat-file', 'commit', old).split(b'\n\n', 1)
            fields = header.split(b'\n')
            rewritten = []
            for field in fields:
                if field.startswith(b'parent '):
                    field = b'parent ' + remapped[field[7:].decode()].encode()
                elif field.startswith(b'tree ') and old in change_tree:
                    entries = git_bytes('ls-tree', '-z', field[5:].decode())
                    tree = git_bytes('mktree', '-z', data=entries + b'100644 blob ' + blob + b'\tshared\0')
                    field = b'tree ' + tree.strip()
                elif field.startswith(b'author ') and old in change_author:
                    field = field.replace(b'Contributor <', b'Another Contributor <')
                rewritten.append(field)
            if old == self.merge:
                message = 'Merge #7 [`🎨✨`] Show preview\n'.encode()
            raw = b'\n'.join(rewritten) + b'\n\n' + message + b'\nRewritten description.\n'
            remapped[old] = git_bytes('hash-object', '-w', '-t', 'commit', '--stdin', data=raw).decode().strip()
            self.assertNotEqual(old, remapped[old])
        self.assertEqual(self.git('rev-list', '--parents', '-1', remapped[self.merge]).split()[1:],
                         [remapped[self.root], remapped[self.member]])
        return remapped[self.merge]

    def test_entire_dag_rewrite_accepts_matching_trees_or_unchanged_integration_deltas(self):
        for altered, proof in (((), 'ordered parent trees'),
                               ((self.root, self.member, self.merge), 'both ordered parents')):
            with self.subTest(altered=altered):
                tip = self.rewrite_fixture(change_tree=altered)
                plan = self.execute(ref=tip, update_pr_titles=True)
                self.assertEqual(plan['pr_title_summary'], {'to_change': 1, 'already_correct': 0, 'unmatched': 0})
                [row] = plan['pr_title_updates']
                self.assertIn(proof, row['evidence'])
                self.assertEqual(row['after'], '[`🎨✨`] Show preview')
                self.pr['title'] = row['after']
                plan = self.execute(ref=tip, update_pr_titles=True)
                self.assertEqual(plan['pr_title_summary'], {'to_change': 0, 'already_correct': 1, 'unmatched': 0})
                self.pr['title'] = 'Show preview'

    def test_rewritten_parent_drift_and_different_attribution_stay_unmatched(self):
        for trees, authors, expected in (
                ((self.root,), (), 'changes from the first parent differ'),
                ((self.member,), (), 'changes from the second parent differ'),
                ((self.root, self.member, self.merge), (self.member,), 'author or author date differs at the second parent')):
            with self.subTest(expected=expected):
                tip = self.rewrite_fixture(trees, authors)
                plan = self.execute(ref=tip, update_pr_titles=True)
                self.assertEqual(plan['source_tip'], plan['candidate_tip'])
                self.assertIn(expected, plan['review_or_changed_commits'][0]['review'])
                self.assertEqual(plan['pr_title_summary'], {'to_change': 0, 'already_correct': 0, 'unmatched': 1})
                self.assertFalse(plan['pr_title_updates'])

    def test_empty_parent_deltas_cannot_confirm_different_snapshots(self):
        self.git('switch', '-q', 'topic')
        self.git('read-tree', '--reset', '-u', self.root)
        self.git('commit', '-q', '--amend', '--allow-empty', '--no-edit')
        self.member = self.git('rev-parse', 'HEAD')
        self.git('switch', '-q', 'main')
        self.git('reset', '--hard', self.root)
        self.git('merge', '--no-ff', 'topic', '-qm', 'Empty landing')
        self.merge = self.git('rev-parse', 'HEAD')
        self.pr['mergeCommit']['oid'] = self.merge
        tip = self.rewrite_fixture(change_tree=(self.root, self.member, self.merge))
        plan = self.execute(ref=tip, update_pr_titles=True)
        self.assertIn('both parent changes are empty', plan['review_or_changed_commits'][0]['review'])
        self.assertEqual(plan['pr_title_summary'], {'to_change': 0, 'already_correct': 0, 'unmatched': 1})

    def test_title_summary_counts_only_selected_landings_and_unmatched_merges(self):
        self.git('switch', '-qc', 'sync')
        self.git('commit', '--allow-empty', '-qm', 'branch state')
        self.git('switch', '-q', 'main')
        self.git('merge', '--no-ff', '-qm', 'Sync branch', 'sync')
        self.git('commit', '--allow-empty', '-qm', 'Rewind unknown change')
        self.pr['title'] = '[`🎨✨`] Show preview'
        full = self.execute(update_pr_titles=True)
        self.assertEqual(full['pr_title_summary'], {'to_change': 0, 'already_correct': 1, 'unmatched': 1})
        limited = self.execute(base=self.merge, update_pr_titles=True)
        self.assertEqual(limited['pr_title_summary'], {'to_change': 0, 'already_correct': 0, 'unmatched': 1})

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
        raw = git_bytes('cat-file', 'commit', self.member)
        header = raw.split(b'\n\n', 1)[0]
        changed = header + b'\nencoding ISO-8859-1\n\nR\xe9parer le r\xe9sum\xe9\n'
        target = git_bytes('hash-object', '-w', '-t', 'commit', '--stdin', data=changed).decode().strip()
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
        header = git_bytes('cat-file', 'commit', self.member).split(b'\n\n', 1)[0]
        for original in (b'', b'invalid \xff'):
            with self.subTest(original=original):
                target = git_bytes('hash-object', '-w', '-t', 'commit', '--stdin',
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
        with self.assertRaisesRegex(ValueError, 'worktree'):
            self.execute(apply='occupied')
        with self.assertRaisesRegex(ValueError, 'already exists'):
            self.execute(apply='topic')
        self.assertFalse(self.requests, 'reserved ref names must fail before querying GitHub')
        self.assertEqual(self.git('-C', 'linked', 'status', '--porcelain=v1', '-b'), before)
        self.git('symbolic-ref', 'refs/heads/alias', 'refs/heads/missing')
        with self.assertRaisesRegex(ValueError, 'symbolic ref'):
            self.execute(apply='alias')
        self.assertEqual(self.git('symbolic-ref', 'refs/heads/alias'), 'refs/heads/missing')
        self.assertEqual(self.git('rev-parse', 'HEAD'), self.merge)

    def test_json_on_ascii_output_survives_branch_creation(self):
        self.pr['title'] = 'Résumé'
        with patch.dict(os.environ, {'LANG': 'C', 'LC_ALL': 'C'}):
            report = self.execute(apply='ascii-report')
        self.assertEqual(self.git('rev-parse', 'ascii-report'), report['candidate_tip'])
        self.assertEqual(report['review_or_changed_commits'][-1]['subject_new'], '[`🎨✨`] Résumé (#7)')

    def test_unsuitable_historical_pr_title_goes_to_review(self):
        self.pr['title'] = 'Two\tcolumns'
        entry = self.execute()['review_or_changed_commits'][0]
        self.assertIn('review', entry)
        self.assertNotIn('candidate', entry)

    def test_signature_and_descendants(self):
        base = git_bytes('cat-file', 'commit', self.merge)
        signed = base.replace(b'\n\n', b'\ngpgsig placeholder\n signed-data\n\n', 1)
        oid = git_bytes('hash-object', '-w', '-t', 'commit', '--stdin', data=signed).decode().strip()
        self.pr['mergeCommit']['oid'] = oid
        report = self.execute(ref=oid)
        self.assertEqual(report['invalidated_signatures'], [oid])
        with self.assertRaisesRegex(ValueError, 'signatures'):
            self.execute(ref=oid, apply='signed-candidate')
        self.assertNotIn('signed-candidate', self.git('show-ref'))
        candidate = self.execute(ref=oid, apply='unsigned-candidate', allow_unsigned=True)
        self.assertNotIn(b'gpgsig ', git_bytes('cat-file', 'commit', candidate['candidate_tip']))
        self.assertIn(b'gpgsig ', git_bytes('cat-file', 'commit', oid))


if __name__ == '__main__':
    unittest.main()
