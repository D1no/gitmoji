"""GitHub merge adapter tests. Run with python3 -m unittest discover -s package/tests/python."""

import contextlib
import copy
import importlib.util
import io
import json
import os
import re
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch


script = Path(__file__).resolve().parents[3] / "skills/gitmoji/scripts/github-merge.py"
spec = importlib.util.spec_from_file_location("github_merge", script)
adapter = importlib.util.module_from_spec(spec)
spec.loader.exec_module(adapter)

TITLE = "-a `shell` $(command) update"
BASE = "a" * 40
HEAD = "b" * 40
ARGS = ["--repo", "team/project", "--pr", "42", "--hostname", "company.ghe.com"]
SUBJECT = "Merge #42 [`🧱🛂`] " + TITLE


def pull():
    return dict(
        id="PR_node42", number=42, state="OPEN", isDraft=False, title=TITLE,
        baseRefName="release/1", baseRef={"target": {"oid": BASE}}, baseRepository={"nameWithOwner": "team/project"},
        headRefName="preview", headRefOid=HEAD, headRepository={"nameWithOwner": "dev/fork"},
        headRepositoryOwner={"login": "dev"}, viewerMergeHeadlineText="Merge pull request #42 from dev/preview",
        viewerMergeBodyText="PR details", isMergeQueueEnabled=False, stack=None, stackEntry=None,
    )


class GitHubMergeTests(unittest.TestCase):
    def setUp(self):
        self.stderr = io.StringIO()
        output = contextlib.redirect_stderr(self.stderr)
        output.__enter__()
        self.addCleanup(output.__exit__, None, None, None)
        self.pr = pull()
        self.on_recheck = {}
        self.requests = []
        self.gitmoji_calls = []
        self.caption_problem = None
        self.inventory = dict(subject=SUBJECT, recognized=1, total=6)
        self.merged = {"merged": True, "sha": "c" * 40}
        mock = patch.object(adapter.subprocess, "run", side_effect=self.fake_run)
        mock.start()
        self.addCleanup(mock.stop)

    def fake_run(self, args, **kwargs):
        if args[0] == "gitmoji":
            self.gitmoji_calls.append(args)
            if self.caption_problem:
                return subprocess.CompletedProcess(args, 1, "", self.caption_problem)
            return subprocess.CompletedProcess(args, 0, json.dumps(self.inventory), "")

        self.assertEqual(args[:4], ["gh", "api", "--hostname", "company.ghe.com"])
        self.assertNotIn("shell", kwargs)
        payload = json.loads(kwargs["input"])
        if args[5] == "POST":
            self.assertEqual(args[6], "graphql")
            self.assertEqual(payload["variables"], {"owner": "team", "name": "project", "number": 42})
            self.assertIn("baseRef { target { oid } }", payload["query"])
            pr = copy.deepcopy(self.pr)
            if self.requests:
                pr.update(self.on_recheck)
            value = {"data": {"repository": {"pullRequest": pr}}}
        else:
            self.assertEqual(args[5:7], ["PUT", "repos/team/project/pulls/42/merge"])
            value = self.merged
        self.requests.append((args[5], payload))
        return subprocess.CompletedProcess(args, 0, json.dumps(value), "")

    def reviewed(self, extra=()):
        self.stderr.truncate(0)
        self.stderr.seek(0)
        text = adapter.main(ARGS + list(extra))
        digest = re.search(r"--expect-preview ([0-9a-f]{64})$", self.stderr.getvalue()).group(1)
        self.requests.clear()
        return text, ["--merge", "--expect-preview", digest]

    def test_preview_preserves_fork_route_default_text_and_one_copy_of_description(self):
        result = adapter.main(ARGS)
        self.assertEqual(result, SUBJECT + "\n\nSource: dev/fork:preview\nBase: team/project:release/1"
                         "\n\nMerge pull request #42 from dev/preview\n\nPR details")
        self.assertEqual([method for method, _ in self.requests], ["POST"])
        self.assertEqual(self.gitmoji_calls, [
            ["gitmoji", "merge-caption", "--json", "--pr", "42", "--title=" + TITLE, BASE, HEAD]
        ])
        self.assertIn("Recognized tags in 1 of 6", self.stderr.getvalue())
        self.assertIn(f"Base {BASE} / Head {HEAD}", self.stderr.getvalue())

    def test_explicit_merge_rechecks_metadata_and_uses_pinned_head_in_json(self):
        preview, apply = self.reviewed()
        self.assertIn("https://company.ghe.com/team/project/pull/42", adapter.main(ARGS + apply))
        self.assertEqual([method for method, _ in self.requests], ["POST", "POST", "PUT"])
        self.assertEqual(self.requests[-1][1], {
            "sha": HEAD, "merge_method": "merge", "commit_title": SUBJECT,
            "commit_message": preview.split("\n\n", 1)[1],
        })

    def test_original_message_keeps_conflict_notes_whitespace_and_crlf(self):
        original = b"Merge topic\r\n\r\nConflicts:\r\n\tfile  \r\n"
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "message"
            path.write_bytes(original)
            result = adapter.main(ARGS + ["--original-message", str(path)])
        self.assertTrue(result.encode().endswith(original))
        self.assertNotIn("PR details", result)

    def test_empty_host_default_body_preserves_its_description_opt_out(self):
        self.pr.update(viewerMergeHeadlineText=TITLE, viewerMergeBodyText="")
        result = adapter.main(ARGS)
        self.assertNotIn("PR details", result)
        self.assertTrue(result.endswith("\n\n" + TITLE))

    def test_stack_preview_names_review_target_and_stack_trunk(self):
        self.pr.update(stack={"number": 7, "size": 3, "baseRefName": "trunk"}, stackEntry={"position": 2})
        result = adapter.main(ARGS)
        self.assertIn("Base: team/project:release/1", result)
        self.assertIn("Stack #7, layer 2 of 3, targeting team/project:trunk", result)

    def test_preview_deleted_fork_uses_available_metadata(self):
        self.pr.update(headRepository=None, headRepositoryOwner=None)
        self.assertIn("source repository unavailable; refs/pull/42/head", adapter.main(ARGS))

    def test_ineligible_pulls_never_trigger_a_put(self):
        for change in ({"state": "CLOSED"}, {"state": "MERGED"}, {"isDraft": True},
                       {"isMergeQueueEnabled": True}, {"stack": {"number": 1}}, {"headRepository": None}):
            with self.subTest(change=change):
                self.pr = dict(pull(), **change)
                with self.assertRaises(ValueError):
                    adapter.main(ARGS + ["--merge", "--expect-preview", "0" * 64])
                self.assertNotIn("PUT", [method for method, _ in self.requests])

    def test_changed_base_aborts_before_put(self):
        _, apply = self.reviewed()
        self.on_recheck["baseRef"] = {"target": {"oid": "f" * 40}}
        with self.assertRaisesRegex(ValueError, "PR changed"):
            adapter.main(ARGS + apply)
        self.assertEqual([method for method, _ in self.requests], ["POST", "POST"])

    def test_changed_inputs_across_preview_and_apply_refuse_before_put(self):
        _, apply = self.reviewed()
        changes = [
            {"headRefOid": "e" * 40}, {"baseRef": {"target": {"oid": "f" * 40}}},
            {"baseRefName": "trunk"}, {"headRefName": "new-topic"}, {"title": "Edited title"},
            {"viewerMergeBodyText": "Edited body"}, {"id": "different-PR"},
        ]
        for change in changes:
            with self.subTest(change=change):
                self.pr = dict(pull(), **change)
                self.requests.clear()
                with self.assertRaisesRegex(ValueError, "differs from the reviewed preview"):
                    adapter.main(ARGS + apply)
                self.assertEqual([method for method, _ in self.requests], ["POST"])

        self.pr = pull()
        self.inventory["recognized"] = 2
        with self.assertRaisesRegex(ValueError, "reviewed preview"):
            adapter.main(ARGS + apply)
        self.assertNotIn("PUT", [method for method, _ in self.requests])

    def test_changed_original_file_aborts_before_put(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "message"
            path.write_bytes(b"Reviewed original\n")
            extra = ["--original-message", str(path)]
            _, apply = self.reviewed(extra)
            path.write_bytes(b"Edited original!\n")
            with self.assertRaisesRegex(ValueError, "reviewed preview"):
                adapter.main(ARGS + extra + apply)
            self.assertNotIn("PUT", [method for method, _ in self.requests])

    def test_same_named_refs_on_another_host_or_repository_require_their_own_preview(self):
        _, apply = self.reviewed()
        with patch.object(adapter, "read_pr", return_value=self.pr), patch.object(adapter, "api") as submit:
            for flag, changed in (("--hostname", "other.ghe.com"), ("--repo", "team/other")):
                with self.subTest(flag=flag), self.assertRaisesRegex(ValueError, "reviewed preview"):
                    adapter.main(ARGS + [flag, changed] + apply)
            submit.assert_not_called()

    def test_merge_requires_review_and_rejects_improper_arguments_before_host_reads(self):
        for extra in (["--merge"], ["--merge", "--expect-preview", "short"], ["--expect-preview", "a" * 64]):
            with self.subTest(extra=extra), self.assertRaises(SystemExit):
                adapter.main(ARGS + extra)
        self.assertEqual(self.requests, [])

    def test_base_ref_must_resolve_before_calculating_coverage(self):
        self.pr["baseRef"] = None
        with self.assertRaisesRegex(ValueError, "both pull request tips"):
            adapter.main(ARGS)
        self.assertEqual(self.gitmoji_calls, [])

    def test_host_rejection_propagates(self):
        _, apply = self.reviewed()
        self.merged = {"merged": False, "message": "blocked by branch protection"}
        with self.assertRaisesRegex(ValueError, "branch protection"):
            adapter.main(ARGS + apply)

    def test_confirmed_merge_with_no_returned_sha_links_to_its_pr(self):
        _, apply = self.reviewed()
        self.merged = {"merged": True}
        self.assertEqual(adapter.main(ARGS + apply), "Merged https://company.ghe.com/team/project/pull/42")

    def test_missing_commits_report_the_fetch_refs(self):
        self.caption_problem = "cannot resolve commit"
        with self.assertRaisesRegex(ValueError, "refs/heads/release/1 and refs/pull/42/head"):
            adapter.main(ARGS)
        self.assertNotIn("PUT", [method for method, _ in self.requests])

    def test_invalid_title_never_reaches_command(self):
        self.pr["title"] = "title\nanother line"
        with self.assertRaisesRegex(ValueError, "single-line"):
            adapter.main(ARGS)
        self.assertEqual(self.gitmoji_calls, [])

    def test_malformed_coordinates_never_contact_host(self):
        for flag, value in [("--repo", "acme/../repos"), ("--hostname", "invalid/?q=1"), ("--pr", "0")]:
            with self.subTest(flag=flag), contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
                adapter.main(ARGS + [flag, value])
        self.assertEqual(self.requests, [])


class InstalledCaptionTests(unittest.TestCase):
    def test_real_mixed_pr_uses_merge_title_and_reports_coverage_separately(self):
        with tempfile.TemporaryDirectory() as clone:
            previous = Path.cwd()
            os.chdir(clone)
            try:
                def git(*args):
                    return subprocess.check_output(["git", *args], text=True).strip()

                git("init", "-q")
                git("config", "user.name", "Test")
                git("config", "user.email", "test@example.invalid")
                git("commit", "-q", "--allow-empty", "-m", "Initial commit")
                pr = pull()
                pr["baseRef"]["target"]["oid"] = git("rev-parse", "HEAD")
                git("commit", "-q", "--allow-empty", "-m", 'Revert "Remove old preview"')
                git("commit", "-q", "--allow-empty", "-m", "🎨✨ #1 (ui): add an export button")
                pr.update(headRefOid=git("rev-parse", "HEAD"), title="Revert preview and add export")
                with patch.object(adapter, "api", return_value={"data": {"repository": {"pullRequest": pr}}}), \
                        contextlib.redirect_stderr(io.StringIO()) as diagnostics:
                    preview = adapter.main(ARGS)
                self.assertEqual(preview.splitlines()[0], "Merge #42 [`🎨✨`] Revert preview and add export")
                self.assertIn("Recognized tags in 1 of 2", diagnostics.getvalue())
                self.assertNotIn("--expect-preview", preview)
            finally:
                os.chdir(previous)


if __name__ == "__main__":
    unittest.main()
