"""Native `gitmoji github-merge` tests over local TLS. Run with python3 -m unittest discover -s package/tests/python."""

import copy
from http.server import BaseHTTPRequestHandler
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest

from hosted_tls import LocalTlsFixture


TITLE = "-a `shell` $(command) update"
CAPTION_TITLE = "[`🧱🛂`] " + TITLE
SUBJECT = CAPTION_TITLE + " (#42)"


def make_pr(base_oid, head_oid):
    return dict(
        id="PR_node42",
        number=42,
        state="OPEN",
        isDraft=False,
        title=TITLE,
        baseRefName="release/1",
        baseRef={"target": {"oid": base_oid}},
        baseRepository={"nameWithOwner": "team/project"},
        headRefName="preview",
        headRefOid=head_oid,
        headRepository={"nameWithOwner": "dev/fork"},
        headRepositoryOwner={"login": "dev"},
        viewerMergeHeadlineText="Merge pull request #42 from dev/preview",
        viewerMergeBodyText="PR details",
        isMergeQueueEnabled=False,
        stack=None,
        stackEntry=None,
    )


class GitHubMergeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.binary = shutil.which("gitmoji")
        if not cls.binary:
            raise unittest.SkipTest("put the built gitmoji on PATH")
        cls.state = {}

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def do_POST(self):
                length = int(self.headers.get("Content-Length", "0") or "0")
                payload = json.loads(self.rfile.read(length))
                assert self.path == "/api/graphql", self.path
                assert payload["variables"] == {
                    "owner": cls.state["expected_owner"],
                    "name": cls.state["expected_name"],
                    "number": 42,
                }
                assert "baseRef { target { oid } }" in payload["query"]
                pr = copy.deepcopy(cls.state["pr"])
                if cls.state["requests"] and cls.state["on_recheck"]:
                    pr.update(cls.state["on_recheck"])
                cls.state["requests"].append(("POST", payload))
                self._json(200, {"data": {"repository": {"pullRequest": pr}}})

            def do_PUT(self):
                length = int(self.headers.get("Content-Length", "0") or "0")
                payload = json.loads(self.rfile.read(length))
                assert self.path == "/api/v3/repos/team/project/pulls/42/merge", self.path
                cls.state["requests"].append(("PUT", payload))
                self._json(200, cls.state["merged"])

            def _json(self, code, data):
                body = json.dumps(data).encode()
                self.send_response(code)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

        cls.tls = LocalTlsFixture(Handler)
        cls.tls2 = LocalTlsFixture(Handler, ca_crt=cls.tls.ca_crt, ca_key=cls.tls.ca_key)

    @classmethod
    def tearDownClass(cls):
        cls.tls.close()
        cls.tls2.close()

    def setUp(self):
        self.repo_dir = tempfile.TemporaryDirectory(prefix="gitmoji-merge-repo-")
        self.addCleanup(self.repo_dir.cleanup)
        self.repo = Path(self.repo_dir.name)
        self.git("init", "-q")
        self.git("config", "user.name", "Test")
        self.git("config", "user.email", "test@example.invalid")
        self.git("commit", "-q", "--allow-empty", "-m", "Older root")
        self.base0 = self.git("rev-parse", "HEAD")
        self.git("commit", "-q", "--allow-empty", "-m", "Base commit")
        self.base = self.git("rev-parse", "HEAD")
        self.git("commit", "-q", "--allow-empty", "-m", "🧱🛂 #42 (core): guard shell input")
        self.head = self.git("rev-parse", "HEAD")
        self.git("commit", "-q", "--allow-empty", "-m", "🧩✨ #42 (core): second commit")
        self.head2 = self.git("rev-parse", "HEAD")

        self.pr = make_pr(self.base, self.head)
        self.state["pr"] = self.pr
        self.state["on_recheck"] = {}
        self.state["requests"] = []
        self.state["merged"] = {"merged": True, "sha": "c" * 40}
        self.state["expected_owner"] = "team"
        self.state["expected_name"] = "project"

    def git(self, *args):
        return subprocess.check_output(
            ["git", "-C", str(self.repo), *args], text=True
        ).strip()

    def run_cli(self, extra=(), hostname=None, repo="team/project", text=True):
        host = hostname or f"localhost:{self.tls.port}"
        env = {
            **os.environ,
            "GH_ENTERPRISE_TOKEN": "test-token",
            "SSL_CERT_FILE": str(self.tls.ca_crt),
            "NO_PROXY": "localhost,127.0.0.1",
            "no_proxy": "localhost,127.0.0.1",
        }
        return subprocess.run(
            [self.binary, "github-merge", "--repo", repo, "--pr", "42", "--hostname", host, *extra],
            cwd=self.repo, env=env, text=text, capture_output=True, timeout=15,
        )

    def reviewed(self, extra=()):
        res = self.run_cli(extra)
        self.assertEqual(res.returncode, 0, res.stderr)
        digest = re.search(r"--expect-preview ([0-9a-f]{64})$", res.stderr.strip()).group(1)
        self.state["requests"].clear()
        return res.stdout.rstrip("\n"), res.stderr, ["--merge", "--expect-preview", digest]

    def test_preview_preserves_fork_route_default_text_and_one_copy_of_description(self):
        res = self.run_cli()
        self.assertEqual(res.returncode, 0, res.stderr)
        self.assertEqual(
            res.stdout.rstrip("\n"),
            SUBJECT + "\n\nSource: dev/fork:preview\nBase: team/project:release/1"
            "\n\nMerge pull request #42 from dev/preview\n\nPR details",
        )
        self.assertEqual([m for m, _ in self.state["requests"]], ["POST"])
        self.assertIn("Recognized tags in 1 of 1", res.stderr)
        self.assertIn(f"Base {self.base} / Head {self.head}", res.stderr)

    def test_explicit_merge_rechecks_metadata_and_uses_pinned_head_in_json(self):
        preview, _, apply = self.reviewed()
        res = self.run_cli(apply)
        self.assertEqual(res.returncode, 0, res.stderr)
        self.assertIn(f"https://localhost:{self.tls.port}/team/project/pull/42", res.stdout)
        self.assertEqual([m for m, _ in self.state["requests"]], ["POST", "POST", "PUT"])
        self.assertEqual(
            self.state["requests"][-1][1],
            {
                "sha": self.head,
                "merge_method": "merge",
                "commit_title": SUBJECT,
                "commit_message": preview.split("\n\n", 1)[1],
            },
        )

    def test_original_message_keeps_conflict_notes_whitespace_and_crlf(self):
        original = b"Merge topic\r\n\r\nConflicts:\r\n\tfile  \r\n"
        path = self.repo / "message"
        path.write_bytes(original)
        res = self.run_cli(["--original-message", str(path)], text=False)
        self.assertEqual(res.returncode, 0, res.stderr.decode(errors="replace"))
        self.assertTrue(res.stdout.endswith(original))
        self.assertNotIn(b"PR details", res.stdout)

    def test_empty_host_default_body_preserves_its_description_opt_out(self):
        self.pr.update(viewerMergeHeadlineText=TITLE, viewerMergeBodyText="")
        res = self.run_cli()
        self.assertEqual(res.returncode, 0, res.stderr)
        self.assertNotIn("PR details", res.stdout)
        self.assertTrue(res.stdout.rstrip("\n").endswith("\n\n" + TITLE))

    def test_stack_preview_names_review_target_and_stack_trunk(self):
        self.pr.update(stack={"number": 7, "size": 3, "baseRefName": "trunk"}, stackEntry={"position": 2})
        res = self.run_cli()
        self.assertEqual(res.returncode, 0, res.stderr)
        self.assertIn("Base: team/project:release/1", res.stdout)
        self.assertIn("Stack #7, layer 2 of 3, targeting team/project:trunk", res.stdout)

    def test_preview_deleted_fork_uses_available_metadata(self):
        self.pr.update(headRepository=None, headRepositoryOwner=None)
        res = self.run_cli()
        self.assertEqual(res.returncode, 0, res.stderr)
        self.assertIn("source repository unavailable; refs/pull/42/head", res.stdout)

    def test_ineligible_pulls_never_trigger_a_put(self):
        for change in (
            {"state": "CLOSED"},
            {"state": "MERGED"},
            {"isDraft": True},
            {"isMergeQueueEnabled": True},
            {"stack": {"number": 1}},
            {"headRepository": None},
        ):
            with self.subTest(change=change):
                self.pr = dict(make_pr(self.base, self.head), **change)
                self.state["pr"] = self.pr
                self.state["requests"].clear()
                res = self.run_cli(["--merge", "--expect-preview", "0" * 64])
                self.assertNotEqual(res.returncode, 0)
                self.assertNotIn("PUT", [m for m, _ in self.state["requests"]])

    def test_changed_base_aborts_before_put(self):
        _, _, apply = self.reviewed()
        self.state["on_recheck"] = {"baseRef": {"target": {"oid": "f" * 40}}}
        res = self.run_cli(apply)
        self.assertNotEqual(res.returncode, 0)
        self.assertRegex(res.stderr, "PR changed")
        self.assertEqual([m for m, _ in self.state["requests"]], ["POST", "POST"])

    def test_changed_inputs_across_preview_and_apply_refuse_before_put(self):
        _, _, apply = self.reviewed()
        changes = [
            {"headRefOid": self.head2},
            {"baseRef": {"target": {"oid": self.base0}}},
            {"baseRefName": "trunk"},
            {"headRefName": "new-topic"},
            {"title": "Edited title"},
            {"viewerMergeBodyText": "Edited body"},
            {"id": "different-PR"},
        ]
        for change in changes:
            with self.subTest(change=change):
                self.state["pr"] = dict(make_pr(self.base, self.head), **change)
                self.state["requests"].clear()
                res = self.run_cli(apply)
                self.assertNotEqual(res.returncode, 0)
                self.assertRegex(res.stderr, "differs from the reviewed preview")
                self.assertEqual([m for m, _ in self.state["requests"]], ["POST"])

    def test_changed_original_file_aborts_before_put(self):
        path = self.repo / "message"
        path.write_bytes(b"Reviewed original\n")
        extra = ["--original-message", str(path)]
        _, _, apply = self.reviewed(extra)
        path.write_bytes(b"Edited original!\n")
        res = self.run_cli(extra + apply)
        self.assertNotEqual(res.returncode, 0)
        self.assertRegex(res.stderr, "reviewed preview")
        self.assertNotIn("PUT", [m for m, _ in self.state["requests"]])

    def test_same_named_refs_on_another_host_or_repository_require_their_own_preview(self):
        _, _, apply = self.reviewed()
        # Different host
        self.state["requests"].clear()
        res_host = self.run_cli(apply, hostname=f"localhost:{self.tls2.port}")
        self.assertNotEqual(res_host.returncode, 0)
        self.assertRegex(res_host.stderr, "reviewed preview")
        self.assertNotIn("PUT", [m for m, _ in self.state["requests"]])
        # Different repository
        self.state["requests"].clear()
        self.state["expected_name"] = "other"
        self.state["pr"] = dict(make_pr(self.base, self.head), baseRepository={"nameWithOwner": "team/other"})
        res_repo = self.run_cli(apply, repo="team/other")
        self.assertNotEqual(res_repo.returncode, 0)
        self.assertRegex(res_repo.stderr, "reviewed preview")
        self.assertNotIn("PUT", [m for m, _ in self.state["requests"]])

    def test_merge_requires_review_and_rejects_improper_arguments_before_host_reads(self):
        for extra in (["--merge"], ["--merge", "--expect-preview", "short"], ["--expect-preview", "a" * 64]):
            with self.subTest(extra=extra):
                res = self.run_cli(extra)
                self.assertNotEqual(res.returncode, 0)
        self.assertEqual(self.state["requests"], [])

    def test_base_ref_must_resolve_before_calculating_coverage(self):
        self.pr["baseRef"] = None
        res = self.run_cli()
        self.assertNotEqual(res.returncode, 0)
        self.assertRegex(res.stderr, "invalid PR commit ID")

    def test_host_rejection_propagates(self):
        _, _, apply = self.reviewed()
        self.state["merged"] = {"merged": False, "message": "blocked by branch protection"}
        res = self.run_cli(apply)
        self.assertNotEqual(res.returncode, 0)
        self.assertRegex(res.stderr, "did not merge")

    def test_confirmed_merge_with_no_returned_sha_links_to_its_pr(self):
        _, _, apply = self.reviewed()
        self.state["merged"] = {"merged": True}
        res = self.run_cli(apply)
        self.assertEqual(res.returncode, 0, res.stderr)
        self.assertEqual(res.stdout.strip(), f"Merged https://localhost:{self.tls.port}/team/project/pull/42")

    def test_missing_commits_report_the_fetch_refs(self):
        self.pr["headRefOid"] = "d" * 40
        res = self.run_cli()
        self.assertNotEqual(res.returncode, 0)
        self.assertIn("refs/pull/42/head", res.stderr)
        self.assertNotIn("PUT", [m for m, _ in self.state["requests"]])

    def test_invalid_title_never_reaches_merge(self):
        self.pr["title"] = "title\nanother line"
        res = self.run_cli()
        self.assertNotEqual(res.returncode, 0)
        self.assertRegex(res.stderr, "single-line")

    def test_malformed_coordinates_never_contact_host(self):
        for flag, value in [("--repo", "acme/../repos"), ("--hostname", "invalid/?q=1"), ("--pr", "0")]:
            with self.subTest(flag=flag):
                if flag == "--repo":
                    res = self.run_cli(repo=value)
                elif flag == "--hostname":
                    res = self.run_cli(hostname=value)
                else:
                    env = {**os.environ, "GH_ENTERPRISE_TOKEN": "test-token", "SSL_CERT_FILE": str(self.tls.ca_crt)}
                    res = subprocess.run(
                        [self.binary, "github-merge", "--repo", "team/project", "--pr", value, "--hostname", f"localhost:{self.tls.port}"],
                        cwd=self.repo, env=env, text=True, capture_output=True,
                    )
                self.assertNotEqual(res.returncode, 0)
        self.assertEqual(self.state["requests"], [])

    def test_real_mixed_pr_uses_merge_title_and_reports_coverage_separately(self):
        self.git("checkout", "-q", self.base)
        self.git("commit", "-q", "--allow-empty", "-m", 'Revert "Remove old preview"')
        self.git("commit", "-q", "--allow-empty", "-m", "🎨✨ #1 (ui): add an export button")
        mixed_head = self.git("rev-parse", "HEAD")
        self.pr.update(headRefOid=mixed_head, title="Revert preview and add export")
        res = self.run_cli()
        self.assertEqual(res.returncode, 0, res.stderr)
        self.assertEqual(res.stdout.splitlines()[0], "[`🎨✨`] Revert preview and add export (#42)")
        self.assertIn("Recognized tags in 1 of 2", res.stderr)
        self.assertNotIn("--expect-preview", res.stdout)


if __name__ == "__main__":
    unittest.main()
