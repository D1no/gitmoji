"""Follow each hosted permalink through the shipped CLI and a disposable GitHub HTTPS fixture."""

from http.server import BaseHTTPRequestHandler
import json
import os
import shutil
import subprocess
import unittest

from hosted_tls import LocalTlsFixture, read_request_body


class PostHost(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def respond(self, value, status=200):
        body = json.dumps(value).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        self.server.requests.append((self.command, self.path))
        if self.headers.get("Authorization") != "Bearer trace-fixture":
            return self.respond({}, 403)
        if self.path != "/api/v3/repos/octo/demo/" + self.server.endpoint:
            return self.respond({}, 404)
        self.respond(self.server.post)

    def do_POST(self):
        self.server.requests.append((self.command, self.path))
        payload = json.loads(read_request_body(self))
        if (self.path != "/api/graphql" or not payload["query"].startswith("query ")
                or payload["variables"] != {"id": "node-42"}
                or self.headers.get("Authorization") != "Bearer trace-fixture"):
            return self.respond({}, 400)
        self.respond(self.server.edits)


class CommentTrace(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.binary = shutil.which("gitmoji")
        if cls.binary is None:
            raise RuntimeError("put the built gitmoji on PATH")
        cls.tls = LocalTlsFixture(PostHost)
        cls.addClassCleanup(cls.tls.close)
        cls.host = f"localhost:{cls.tls.port}"
        cls.repo = f"https://{cls.host}/octo/demo"
        cls.signed = subprocess.run([cls.binary, "comment", "sign", "--harness", "fixture-runner"],
                                    input="A signed note.", text=True, capture_output=True,
                                    timeout=15, check=True).stdout

    def setUp(self):
        self.server = self.tls.server
        self.server.requests = []
        self.server.edits = {"errors": [{"message": "field unavailable"}]}

    def trace(self, url, success=True):
        result = subprocess.run(
            [self.binary, "comment", "trace", "--hostname", self.host, "--json", url],
            env={**os.environ, "GH_ENTERPRISE_TOKEN": "trace-fixture",
                 "SSL_CERT_FILE": str(self.tls.ca_crt), "no_proxy": "localhost,127.0.0.1",
                 "NO_PROXY": "localhost,127.0.0.1"},
            text=True, capture_output=True, timeout=15,
        )
        if not success:
            self.assertNotEqual(result.returncode, 0)
            return result.stderr
        self.assertEqual(result.returncode, 0, result.stderr)
        return json.loads(result.stdout)

    def test_raw_lookup_and_history_fallback_for_each_post_kind(self):
        routes = [
            ("issues/2", "issues/2", "Issue"),
            ("pull/2", "pulls/2", "PullRequest"),
            ("pull/2#issue-5000000042", "issues/2", "PullRequest"),
            ("issues/2#issue-5000000042", "issues/2", "Issue"),
            ("issues/2#issuecomment-5000000001", "issues/comments/5000000001", "IssueComment"),
            ("pull/2#issuecomment-4", "issues/comments/4", "IssueComment"),
            ("pull/2#discussion_r5", "pulls/comments/5", "PullRequestReviewComment"),
            ("pull/2/files#r5", "pulls/comments/5", "PullRequestReviewComment"),
            ("pull/2/changes#r5", "pulls/comments/5", "PullRequestReviewComment"),
            ("pull/2#pullrequestreview-6", "pulls/2/reviews/6", "PullRequestReview"),
        ]
        for path, endpoint, kind in routes:
            with self.subTest(path):
                self.server.endpoint = endpoint
                canonical = "pull/2#discussion_r5" if "/files" in path or "/changes" in path else path
                canonical = canonical.split("#issue-")[0]
                self.server.post = {"html_url": self.repo + "/" + canonical, "id": 5000000042, "body": self.signed,
                                    "body_html": "<p>Already rendered</p>", "node_id": "node-42",
                                    "updated_at": "2026-01-09T00:00:00Z"}
                report = self.trace(self.repo + "/" + path)
                self.assertEqual((report["kind"], report["status"]), (kind, "verified"))
                self.assertEqual(report["url"], self.server.post["html_url"])
                self.assertEqual(report["edit_history"], {"available": False})
                self.assertEqual(report["updated_at"], self.server.post["updated_at"])

    def test_correlates_visible_edit_metadata_with_the_same_body_revision(self):
        url = self.repo + "/pull/2#issuecomment-4"
        self.server.endpoint = "issues/comments/4"
        self.server.post = {"html_url": url, "body": self.signed, "node_id": "node-42"}
        revision = {"editedAt": "2026-01-01T00:00:00Z", "editor": {"login": "reviewer"},
                    "deletedAt": "2026-01-02T00:00:00Z"}
        node = {"id": "node-42", "__typename": "IssueComment",
                "body": self.signed.replace("A signed note.", "A changed note."),
                "lastEditedAt": "2026-01-01T00:00:00Z", "editor": {"login": "reviewer"},
                "updatedAt": "2026-01-09T00:00:00Z", "userContentEdits": {
                    "totalCount": 22, "nodes": [revision], "pageInfo": {"hasNextPage": True}}}
        self.server.edits = {"data": {"node": node}}
        report = self.trace(url)
        self.assertEqual(report["status"], "edited")
        self.assertEqual(report["edit_history"], {"available": True,
            "last_edited_at": node["lastEditedAt"], "editor_account": "reviewer",
            "visible_entry_count": 22, "has_more": True, "revisions": [
                {"edited_at": revision["editedAt"], "editor_account": "reviewer", "deleted_at": revision["deletedAt"]}]})
        self.assertEqual(report["updated_at"], node["updatedAt"])

    def test_rejects_unrelated_host_anchor_parent_or_comment_id(self):
        for url in ["https://github.com/octo/demo/issues/2", self.repo + "/issues/2#unknown",
                    self.repo + "/pull/2#discussion_r0", self.repo + "/issues/2?redirect=elsewhere"]:
            with self.subTest(url):
                self.trace(url, success=False)
        self.assertEqual(self.server.requests, [])
        self.server.endpoint = "pulls/comments/7"
        for wrong in ["octo/demo/pull/3#discussion_r7", "octo/demo/pull/2#discussion_r8",
                      "octo/elsewhere/pull/2#discussion_r7"]:
            self.server.requests = []
            self.server.post = {"html_url": f"https://{self.host}/{wrong}", "node_id": "node-42", "body": ""}
            error = self.trace(self.repo + "/pull/2#discussion_r7", success=False)
            self.assertIn("different post or parent", error)
            self.assertEqual(self.server.requests, [("GET", "/api/v3/repos/octo/demo/pulls/comments/7")])

    def test_null_review_body_and_unrelated_graph_node(self):
        url = self.repo + "/pull/2#pullrequestreview-7"
        self.server.endpoint = "pulls/2/reviews/7"
        self.server.post = {"html_url": url, "node_id": "node-42", "body": None,
                            "submitted_at": "2026-01-04T00:00:00Z"}
        for node_id, kind in [("other", "PullRequestReview"), ("node-42", "IssueComment")]:
            self.server.edits = {"data": {"node": {"id": node_id, "__typename": kind, "body": self.signed}}}
            report = self.trace(url)
            self.assertEqual(report["status"], "unsigned")
            self.assertEqual(report["submitted_at"], self.server.post["submitted_at"])
            self.assertEqual(report["edit_history"], {"available": False})

    def test_description_anchor_and_unavailable_history(self):
        url = self.repo + "/pull/2"
        self.server.endpoint = "issues/2"
        self.server.post = {"id": 42, "html_url": url, "body": self.signed, "node_id": "node-42"}
        node = {"id": "node-42", "__typename": "PullRequest", "body": self.signed,
                "lastEditedAt": "2026-01-03T00:00:00Z", "editor": {"login": "octo"}}
        for connection in [None, {"totalCount": None}]:
            node["userContentEdits"] = connection
            self.server.edits = {"data": {"node": node}}
            report = self.trace(url + "#issue-42")
            self.assertEqual(report["status"], "verified")
            self.assertEqual(report["edit_history"], {"available": False,
                "last_edited_at": node["lastEditedAt"], "editor_account": "octo"})
        self.server.requests = []
        error = self.trace(url + "#issue-43", success=False)
        self.assertIn("different description ID", error)
        self.assertEqual(self.server.requests, [("GET", "/api/v3/repos/octo/demo/issues/2")])
