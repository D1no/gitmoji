"""Native `gitmoji pr-title` tests over local TLS. Run with python3 -m unittest discover -s package/tests/python."""

from http.server import BaseHTTPRequestHandler
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

from hosted_tls import LocalTlsFixture, read_request_body, serve_git_http


class TitleTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.binary = shutil.which("gitmoji")
        if not cls.binary:
            raise unittest.SkipTest("put the built gitmoji on PATH")
        cls.state = {}

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def do_GET(self):
                if self.path.startswith("/example/project.git/"):
                    serve_git_http(self, cls.state["git_root"], self.path)
                    return
                if self.path == "/api/v3/repos/example/project/pulls/12":
                    handler = cls.state.get("on_get_pull")
                    if handler:
                        code, data = handler()
                    else:
                        code, data = 200, cls.state["pull_queue"].pop(0) if cls.state["pull_queue"] else cls.state["pull"]
                    self._json(code, data)
                    return
                if self.path.startswith("/api/v3/repos/example/project/git/ref/heads/"):
                    ref_data = cls.state["ref_queue"].pop(0) if cls.state["ref_queue"] else cls.state["ref"]
                    self._json(200, ref_data)
                    return
                self.send_response(404)
                self.end_headers()

            def do_POST(self):
                if self.path.startswith("/example/project.git/"):
                    serve_git_http(self, cls.state["git_root"], self.path)
                    return
                self.send_response(404)
                self.end_headers()

            def do_PATCH(self):
                payload = json.loads(read_request_body(self))
                cls.state["patches"].append((self.path, payload))
                code = cls.state.get("patch_status", 200)
                if code == 422:
                    self._json(422, {"message": "Validation Failed"})
                else:
                    self._json(code, {"number": 12, "title": payload["title"]})

            def _json(self, code, data):
                body = json.dumps(data).encode()
                self.send_response(code)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

        cls.tls = LocalTlsFixture(Handler)

    @classmethod
    def tearDownClass(cls):
        cls.tls.close()

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="gitmoji-title-repo-")
        self.addCleanup(self.tmp.cleanup)
        root = Path(self.tmp.name)
        self.git_root = root / "srv"
        self.git_root.mkdir()
        self.origin = self.git_root / "example" / "project.git"
        self.origin.mkdir(parents=True)
        subprocess.run(["git", "init", "--bare", "--quiet", "--initial-branch=trunk", str(self.origin)], check=True)

        work = root / "work"
        subprocess.run(["git", "clone", "--quiet", str(self.origin), str(work)], check=True)
        self.work = work
        self.git(work, "config", "user.name", "User")
        self.git(work, "config", "user.email", "user@example.invalid")
        self.git(work, "commit", "--quiet", "--allow-empty", "-m", "base")
        self.base = self.git(work, "rev-parse", "HEAD")
        self.git(work, "push", "--quiet", "origin", "trunk")

        self.git(work, "checkout", "--quiet", "-b", "fork-head")
        (work / "gitmoji.toml").write_text("invalid {{{{")
        self.git(work, "add", "gitmoji.toml")
        self.git(work, "commit", "--quiet", "-m", "🔖 release 1.2.3")
        self.head = self.git(work, "rev-parse", "HEAD")
        self.git(work, "push", "--quiet", "origin", f"{self.head}:refs/pull/12/head")

        self.shallow = root / "shallow-consumer"
        subprocess.run(["git", "clone", "--quiet", "--depth=1", self.origin.as_uri(), str(self.shallow)], check=True)
        self.assertTrue((self.shallow / ".git/shallow").exists())

        self.state["git_root"] = self.git_root
        self.state["patches"] = []
        self.state["patch_status"] = 200
        self.state["on_get_pull"] = None
        self.state["pull_queue"] = []
        self.state["ref_queue"] = []
        self.state["pull"] = {
            "number": 12,
            "state": "open",
            "title": "Title",
            "head": {"sha": self.head},
            "base": {"ref": "trunk", "sha": self.base},
        }
        self.state["ref"] = {
            "ref": "refs/heads/trunk",
            "object": {"sha": self.base, "type": "commit"},
        }

    def git(self, cwd, *args):
        return subprocess.check_output(["git", "-C", str(cwd), *args], text=True).strip()

    def run_cli(self, env_overrides=None):
        server = f"https://localhost:{self.tls.port}"
        env = {
            **os.environ,
            "GITHUB_SERVER_URL": server,
            "GITHUB_API_URL": f"{server}/api/v3",
            "GITHUB_REPOSITORY": "example/project",
            "GITMOJI_TITLE_TOKEN": "fixture-token",
            "SSL_CERT_FILE": str(self.tls.ca_crt),
            "NO_PROXY": "localhost,127.0.0.1",
            "no_proxy": "localhost,127.0.0.1",
        }
        if env_overrides:
            env.update(env_overrides)
        return subprocess.run(
            [self.binary, "pr-title", "--pr", "12"],
            cwd=self.shallow, env=env, text=True, capture_output=True, timeout=15,
        )

    def test_noop_and_closed(self):
        self.state["pull"]["title"] = "[`🔖`] Title"
        res = self.run_cli()
        self.assertEqual(res.returncode, 0, res.stderr)
        self.assertIn("matches", res.stdout)
        self.assertEqual(self.state["patches"], [])

        self.state["pull"]["state"] = "closed"
        res2 = self.run_cli()
        self.assertEqual(res2.returncode, 0, res2.stderr)
        self.assertIn("closed", res2.stdout)
        self.assertEqual(self.state["patches"], [])

    def test_retarget_force_push_and_human_title_edit_use_latest(self):
        self.git(self.work, "checkout", "--quiet", "trunk")
        self.git(self.work, "checkout", "--quiet", "-b", "release")
        self.git(self.work, "commit", "--quiet", "--allow-empty", "-m", "release base")
        rel_base = self.git(self.work, "rev-parse", "HEAD")
        self.git(self.work, "push", "--quiet", "origin", "release")
        self.git(self.work, "commit", "--quiet", "--allow-empty", "-m", "🎨✨ #12 (ui): updated")
        rel_head = self.git(self.work, "rev-parse", "HEAD")
        self.git(self.work, "push", "--quiet", "origin", f"+{rel_head}:refs/pull/12/head")

        old_pull = dict(self.state["pull"])
        updated_pull = {
            "number": 12,
            "state": "open",
            "title": "-new title `verbatim`",
            "head": {"sha": rel_head},
            "base": {"ref": "release", "sha": rel_base},
        }
        old_ref = {"ref": "refs/heads/trunk", "object": {"sha": self.base, "type": "commit"}}
        rel_ref = {"ref": "refs/heads/release", "object": {"sha": rel_base, "type": "commit"}}
        self.state["pull_queue"] = [old_pull, updated_pull, updated_pull, updated_pull]
        self.state["ref_queue"] = [old_ref, rel_ref, rel_ref, rel_ref]

        res = self.run_cli()
        self.assertEqual(res.returncode, 0, res.stderr)
        self.assertEqual(self.state["patches"], [
            ("/api/v3/repos/example/project/pulls/12", {"title": "[`🎨✨`] -new title `verbatim`"})
        ])

    def test_a_changed_pr_gets_fresh_history_even_on_a_failed_fetch(self):
        # Missing refs/pull/12/head causes fetch failure; next read sees closed -> exits 0 cleanly.
        self.git(self.origin, "update-ref", "-d", "refs/pull/12/head")
        open_pull = dict(self.state["pull"])
        closed_pull = dict(self.state["pull"], state="closed")
        self.state["pull_queue"] = [open_pull, closed_pull, closed_pull]
        res = self.run_cli()
        self.assertEqual(res.returncode, 0, res.stderr)
        self.assertIn("closed", res.stdout)
        self.assertEqual(self.state["patches"], [])

        # Still open after failed fetch -> fails without patching.
        self.state["pull_queue"] = [open_pull, open_pull]
        res2 = self.run_cli()
        self.assertNotEqual(res2.returncode, 0)
        self.assertRegex(res2.stderr, "could not fetch")
        self.assertEqual(self.state["patches"], [])

    def test_preserves_full_title_on_host_validation_failure(self):
        long_human = "x" * 256
        self.state["pull"]["title"] = long_human
        self.state["patch_status"] = 422
        res = self.run_cli()
        self.assertNotEqual(res.returncode, 0)
        self.assertIn("422", res.stderr)
        self.assertEqual(
            self.state["patches"],
            [("/api/v3/repos/example/project/pulls/12", {"title": "[`🔖`] " + long_human})],
        )

    def test_reads_live_base_ref_when_pr_records_an_older_commit(self):
        self.state["pull"]["base"] = {"ref": "trunk", "sha": "0" * 40}
        res = self.run_cli()
        self.assertEqual(res.returncode, 0, res.stderr)
        self.assertEqual(
            self.state["patches"],
            [("/api/v3/repos/example/project/pulls/12", {"title": "[`🔖`] Title"})],
        )

    def test_enterprise_host_mismatch_refuses_before_requests(self):
        res = self.run_cli({
            "GITHUB_SERVER_URL": "https://tenant.ghe.com",
            "GITHUB_API_URL": "https://api.github.com",
        })
        self.assertNotEqual(res.returncode, 0)
        self.assertRegex(res.stderr, "base repository")

    def test_fork_ref_inventory_updates_title_from_shallow_consumer_and_handles_missing_ref(self):
        human = "-a `literal` $(touch injected)"
        self.state["pull"]["title"] = human
        res = self.run_cli()
        self.assertEqual(res.returncode, 0, res.stderr)
        self.assertEqual(
            self.state["patches"],
            [("/api/v3/repos/example/project/pulls/12", {"title": "[`🔖`] " + human})],
        )

        # Missing refs/pull/12/head must fail without patching.
        self.state["patches"].clear()
        self.git(self.origin, "update-ref", "-d", "refs/pull/12/head")
        res_missing = self.run_cli()
        self.assertNotEqual(res_missing.returncode, 0)
        self.assertRegex(res_missing.stderr, "could not fetch")
        self.assertEqual(self.state["patches"], [])


if __name__ == "__main__":
    unittest.main()
