"""Exercise title synchronization with isolated Git history and simulated GitHub state."""
import importlib.util
import os
from contextlib import chdir
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import Mock, patch
from urllib.error import HTTPError

source = Path(__file__).resolve().parents[3] / "skills/gitmoji/scripts/github-pr-title.py"
spec = importlib.util.spec_from_file_location("github_pr_title", source)
title_sync = importlib.util.module_from_spec(spec)
spec.loader.exec_module(title_sync)

PULL = title_sync.Pull("Title", "trunk", "a" * 40, "b" * 40)
CLOUD = {
    "GITHUB_SERVER_URL": "https://github.com",
    "GITHUB_API_URL": "https://api.github.com",
    "GITHUB_REPOSITORY": "example/project",
    "GITMOJI_TITLE_TOKEN": "fixture-token",
}


class TitleTests(unittest.TestCase):
    def test_noop_and_closed(self):
        host = Mock()
        host.read.side_effect = [PULL, PULL, None]
        with patch.object(title_sync, "proposal", return_value="Title") as proposal:
            self.assertIn("matches", title_sync.synchronize(host, "/bin/gitmoji"))
            self.assertIn("closed", title_sync.synchronize(host, "/bin/gitmoji"))
            proposal.assert_called_once()
        host.update.assert_not_called()

    def test_retarget_force_push_and_human_title_edit_use_latest(self):
        host = Mock()
        updated = title_sync.Pull("-new title `verbatim`", "release", "c" * 40, "d" * 40)
        host.read.side_effect = [PULL, updated, updated, updated]
        with patch.object(title_sync, "proposal", side_effect=["stale", "updated"]) as proposal:
            title_sync.synchronize(host, "/bin/gitmoji")
            self.assertEqual(proposal.call_args_list[-1].args[1], updated)
        host.update.assert_called_once_with("updated")

    def test_a_changed_pr_gets_fresh_history_even_on_a_failed_fetch(self):
        host = Mock()
        host.read.side_effect = [PULL, None, None]
        with patch.object(title_sync, "proposal", side_effect=ValueError("missing")):
            self.assertIn("closed", title_sync.synchronize(host, "/bin/gitmoji"))
        host.update.assert_not_called()
        host.read.side_effect = [PULL, PULL]
        with patch.object(title_sync, "proposal", side_effect=ValueError("missing")):
            with self.assertRaisesRegex(ValueError, "missing"):
                title_sync.synchronize(host, "/bin/gitmoji")
        host.update.assert_not_called()

    def test_preserves_full_title_on_host_validation_failure(self):
        from io import BytesIO
        import json
        long_title = "[`🔖`] " + "x" * 256
        with patch.dict(os.environ, CLOUD), patch.object(title_sync, "build_opener") as opener:
            response = opener.return_value.open
            response.side_effect = HTTPError("https://api.github.com", 422, "", {}, BytesIO())
            host = title_sync.GitHub(12)
            with self.assertRaisesRegex(ValueError, "Shorten the human wording"):
                host.update(long_title)
            req = response.call_args.args[0]
            self.assertEqual(json.loads(req.data), {"title": long_title})
            self.assertEqual(req.method, "PATCH")
            response.assert_called_once()

    def test_reads_live_base_ref_when_pr_records_an_older_commit(self):
        with patch.dict(os.environ, CLOUD):
            host = title_sync.GitHub(12)
            pull = {"number": 12, "state": "open", "title": "Title", "head": {"sha": PULL.head},
                    "base": {"ref": "release/next", "sha": "0" * 40}}
            reference = {"ref": "refs/heads/release/next", "object": {"sha": PULL.base, "type": "commit"}}
            with patch.object(host, "request", side_effect=[pull, reference]) as api:
                result = host.read()
            self.assertEqual(result.base, PULL.base)
            self.assertEqual(api.call_args.kwargs["endpoint"], host.repository_api + "/git/ref/heads/release/next")

    def test_enterprise_host_endpoints_and_host_mismatch(self):
        for origin, api in [
            ("https://github.com", "https://api.github.com"),
            ("https://tenant.ghe.com", "https://api.tenant.ghe.com"),
            ("https://code.example.org:8443", "https://code.example.org:8443/api/v3"),
        ]:
            with self.subTest(api=api), patch.dict(os.environ, {**CLOUD, "GITHUB_SERVER_URL": origin, "GITHUB_API_URL": api}):
                host = title_sync.GitHub(12)
                self.assertEqual(host.remote, origin + "/example/project.git")
                self.assertEqual(host.endpoint, api + "/repos/example/project/pulls/12")
        with patch.dict(os.environ, {**CLOUD, "GITHUB_SERVER_URL": "https://tenant.ghe.com"}):
            with self.assertRaisesRegex(ValueError, "base repository"):
                title_sync.GitHub(12)

    def test_fork_ref_inventory_does_not_read_pr_files_or_caller_shallow_history(self):
        binary = shutil.which("gitmoji")
        self.assertIsNotNone(binary, "put the built gitmoji on PATH")
        with tempfile.TemporaryDirectory() as temporary, patch.dict(os.environ, CLOUD):
            path = Path(temporary)
            repo = path / "origin"
            repo.mkdir()

            def git(*args):
                return subprocess.check_output(["git", "-c", "user.name=User", "-c",
                    "user.email=user@example.invalid", *args], cwd=repo, text=True).strip()

            git("init", "--quiet", "--initial-branch=trunk")
            git("commit", "--quiet", "--allow-empty", "-m", "base")
            base = git("rev-parse", "HEAD")
            git("checkout", "--quiet", "-b", "fork-head")
            (repo / "gitmoji.toml").write_text("invalid {{{{")
            git("add", "gitmoji.toml")
            git("commit", "--quiet", "-m", "🔖 release 1.2.3")
            head = git("rev-parse", "HEAD")
            git("update-ref", "refs/pull/12/head", head)
            git("checkout", "--quiet", "trunk")
            git("branch", "-D", "fork-head")
            # Every computation uses a fresh bare preview. A missing or advanced
            # refs/pull/N/head must leave the old title alone.
            host = title_sync.GitHub(12)
            host.remote = repo.as_uri()
            human = "-a `literal` $(touch injected)"
            active = title_sync.Pull(human, "trunk", base, head)
            shallow = path / "shallow-consumer"
            git("clone", "--quiet", "--depth=1", repo.as_uri(), str(shallow))
            self.assertTrue((shallow / ".git/shallow").exists())
            with tempfile.TemporaryDirectory() as preview, chdir(shallow):
                self.assertEqual(title_sync.proposal(host, active, binary, preview), "[`🔖`] " + human)
                self.assertFalse((Path(preview) / "gitmoji.toml").exists())
                self.assertFalse((Path(preview) / "injected").exists())
            newer = title_sync.Pull(human, "trunk", base, "c" * 40)
            with tempfile.TemporaryDirectory() as preview, self.assertRaisesRegex(ValueError, "differs"):
                title_sync.proposal(host, newer, binary, preview)
            git("merge", "--ff-only", "--quiet", head)
            with tempfile.TemporaryDirectory() as preview, self.assertRaisesRegex(ValueError, "differs"):
                title_sync.proposal(host, active, binary, preview)
            advanced = title_sync.Pull("[`🔖`] " + human, "trunk", head, head)
            with tempfile.TemporaryDirectory() as preview:
                self.assertEqual(title_sync.proposal(host, advanced, binary, preview), human)
            git("update-ref", "-d", "refs/pull/12/head")
            with tempfile.TemporaryDirectory() as preview, self.assertRaisesRegex(ValueError, "Could not fetch"):
                title_sync.proposal(host, active, binary, preview)


if __name__ == "__main__":
    unittest.main()
