"""Exercise the shipped provisional workflow command on a local HTTPS host."""

from http.server import BaseHTTPRequestHandler
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

from hosted_tls import LocalTlsFixture, read_request_body, serve_git_http


class PolicyHost(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def respond(self, value, status=200):
        body = json.dumps(value).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def dispatch(self):
        state = self.server.state
        if self.path.startswith("/octo/repo.git/"):
            return serve_git_http(self, state["root"], self.path)
        prefix = "/api/v3/repos/octo/repo"
        if not self.path.startswith(prefix) or self.headers.get("Authorization") != "Bearer fixture-token":
            return self.respond({}, 403)
        route = self.path[len(prefix):]
        if self.command == "GET":
            if route == "/pulls/12":
                return self.respond(state["pr"], state["pull_status"])
            if route == "/git/ref/heads/trunk":
                return self.respond({"ref": "refs/heads/trunk", "object": {"type": "commit", "sha": state["base"]}})
            if route == "/issues/42":
                return self.respond(state["issue"])
            if route == "/issues/42/comments?per_page=100&page=1":
                return self.respond(state["comments"])
        else:
            value = json.loads(read_request_body(self))
            state["writes"].append((self.command, route, value))
            if self.command == "PATCH" and route == "/pulls/12":
                state["pr"].update(value)
                return self.respond(state["pr"])
            if self.command == "POST" and route == "/issues/42/comments":
                state["comments"].append(value)
                return self.respond(value, 201)
            if self.command == "POST" and route.startswith("/statuses/"):
                return self.respond(value, 201)
        self.respond({}, 404)

    do_GET = do_POST = do_PATCH = dispatch


class ProvisionalTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.binary = shutil.which("gitmoji")
        if not cls.binary:
            raise unittest.SkipTest("put the built gitmoji on PATH")
        cls.tls = LocalTlsFixture(PolicyHost)
        cls.addClassCleanup(cls.tls.close)

    def setUp(self):
        scratch = tempfile.TemporaryDirectory(prefix="gitmoji-policy-")
        self.addCleanup(scratch.cleanup)
        root = Path(scratch.name)
        self.origin = root / "octo" / "repo.git"
        self.origin.mkdir(parents=True)
        self.git("init", "--bare", "--initial-branch=trunk")
        self.tree = self.git("mktree")
        base = self.git("commit-tree", self.tree, "-m", "base")
        self.git("update-ref", "refs/heads/trunk", base)
        self.state = self.tls.server.state = {
            "root": root, "base": base, "writes": [], "comments": [], "pull_status": 200,
            "pr": {"number": 12, "state": "open", "title": "Demo", "body": "Author notes",
                   "base": {"ref": "trunk", "sha": base}, "head": {}},
            "issue": {"number": 42, "state": "open", "labels": [{"name": "board risk"}]},
        }
        self.set_head(base, "🎨💄🚧 #42 (app): preview the design")

    def git(self, *args):
        return subprocess.run(["git", "-C", str(self.origin), "-c", "user.name=Fixture",
                               "-c", "user.email=fixture@example.invalid", *args],
                              input="", text=True, capture_output=True, check=True).stdout.strip()

    def set_head(self, parent, subject):
        self.head = self.git("commit-tree", self.tree, "-p", parent, "-m", subject)
        self.git("update-ref", "refs/pull/12/head", self.head)
        self.state["pr"]["head"] = {"sha": self.head, "repo": {"full_name": "fork/repo"}}

    def run_cli(self, *args, success=True):
        env = {**os.environ, "GH_ENTERPRISE_TOKEN": "fixture-token", "SSL_CERT_FILE": str(self.tls.ca_crt),
               "NO_PROXY": "localhost,127.0.0.1", "no_proxy": "localhost,127.0.0.1"}
        result = subprocess.run([self.binary, "provisional-pr", "--repo", "octo/repo", "--pr", "12",
                                 "--hostname", f"localhost:{self.tls.port}", "--label", "board risk",
                                 "--expect-head", self.head, "--json", *args],
                                env=env, text=True, capture_output=True, timeout=15)
        self.assertEqual(result.returncode, 0 if success else 1, result.stdout + result.stderr)
        return json.loads(result.stdout)

    def test_records_status_on_head_and_updates_author_body_and_backlink_once(self):
        report = self.run_cli("--check")
        self.assertEqual((report["policy"], report["follow_up_issue"]), ("provisional", 42))
        self.assertEqual(self.state["writes"], [])
        self.run_cli("--publish-status", "--backlink")
        self.run_cli("--publish-status", "--backlink")
        body = self.state["pr"]["body"]
        self.assertTrue(body.startswith("Author notes\n\n"))
        self.assertEqual(body.count("Follow-up issue: #42"), 1)
        self.assertEqual(len(self.state["comments"]), 1)
        self.assertEqual(sum(method == "PATCH" for method, _, _ in self.state["writes"]), 1)
        statuses = [(route, value["state"]) for _, route, value in self.state["writes"] if route.startswith("/statuses/")]
        self.assertEqual(statuses, [(f"/statuses/{self.head}", stage) for stage in ["pending", "success"] * 2])

        self.set_head(self.state["base"], "🎨💄 #42 (app): settle the design")
        self.assertEqual(self.run_cli("--publish-status")["policy"], "ordinary")
        self.assertEqual(self.state["pr"]["body"], "Author notes")

    def test_failed_reads_and_mixed_missing_tickets_cannot_leave_a_success_status(self):
        self.state["pull_status"] = 503
        self.run_cli("--publish-status", success=False)
        self.state["pull_status"] = 200
        self.set_head(self.head, "🎨💄🚧 — (app): preview another design")
        self.run_cli("--publish-status", success=False)
        self.assertEqual([v["state"] for _, _, v in self.state["writes"]], ["pending", "failure"] * 2)
        self.assertEqual(self.state["writes"][-1][1], f"/statuses/{self.head}")
        self.assertEqual(self.state["pr"]["body"], "Author notes")
