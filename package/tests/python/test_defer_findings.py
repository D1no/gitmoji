"""Run defer-findings against an Enterprise-style HTTPS endpoint and actual commits."""

from http.server import BaseHTTPRequestHandler
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

from hosted_tls import LocalTlsFixture, read_request_body


class DeferFindingsTests(unittest.TestCase):
    def setUp(self):
        self.binary = shutil.which("gitmoji")
        if not self.binary:
            self.skipTest("put the built gitmoji on PATH")
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.repo = Path(temporary.name)
        self.git("init", "-q")
        self.git("config", "user.name", "Reviewer")
        self.git("config", "user.email", "reviewer@example.invalid")
        self.git("commit", "-qm", "initial", "--allow-empty")
        self.base = self.git("rev-parse", "HEAD")
        self.git("commit", "-qm", "🧩✨ #76 (feed): publish finished work", "--allow-empty")
        self.settled = self.git("rev-parse", "HEAD")
        self.git("commit", "-qm", "🎨🎭🚧 #19 (feed): show a sample feed", "--allow-empty")
        self.head = self.git("rev-parse", "HEAD")
        self.issue = {"number": 19, "state": "open", "labels": [{"name": "provisional"}]}
        self.threads = [
            self.thread("production", 501, "The save button loses data"),
            self.thread("presentation", 502, "Tune the label contrast"),
        ]
        self.issue_comments = []
        self.replies = []
        self.writes = []
        self.close_after_reply = False
        self.fail_resolution = False
        case = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def respond(self, value, status=200):
                data = json.dumps(value).encode()
                self.send_response(status)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)

            def do_GET(self):
                prefix = "/api/v3/repos/project/app"
                responses = {
                    f"{prefix}/git/ref/heads/trunk": {
                        "ref": "refs/heads/trunk", "object": {"type": "commit", "sha": case.base},
                    },
                    f"{prefix}/issues/19": case.issue,
                    f"{prefix}/issues/19/comments?per_page=100&page=1": case.issue_comments,
                    f"{prefix}/pulls/12/comments?per_page=100&page=1": case.replies,
                    f"{prefix}/pulls/12": {
                        "number": 12, "state": "open", "title": "🚧 A misleading title",
                        "head": {"sha": case.head}, "base": {"ref": "trunk", "sha": case.base},
                    },
                }
                if self.path in responses:
                    return self.respond(responses[self.path])
                self.respond({"message": "unrecognized read"}, 404)

            def do_POST(self):
                payload = json.loads(read_request_body(self))
                if self.path == "/api/graphql":
                    query, variables = payload["query"], payload["variables"]
                    if "reviewThreads(" in query:
                        cursor = variables.get("cursor")
                        second = cursor == "page-two"
                        return self.respond({"data": {"viewer": {"login": "reviewer"},
                            "repository": {"pullRequest": {
                                "state": "OPEN", "baseRefOid": case.base, "headRefOid": case.head,
                                "reviewThreads": {
                                    "pageInfo": {"hasNextPage": not second,
                                                 "endCursor": None if second else "page-two"},
                                    "nodes": [case.threads[1 if second else 0]],
                                },
                            }}}})
                    if "resolveReviewThread(" in query:
                        thread_id = variables["id"]
                        case.writes.append(("resolve", thread_id))
                        if case.fail_resolution:
                            return self.respond({"errors": [{"message": "resolution refused"}]})
                        chosen = next(t for t in case.threads if t["id"] == thread_id)
                        chosen["isResolved"] = True
                        return self.respond({"data": {"resolveReviewThread": {"thread": {
                            "id": thread_id, "isResolved": True,
                        }}}})
                comment = {"user": {"login": "reviewer"}, "body": payload.get("body", "")}
                if self.path == "/api/v3/repos/project/app/issues/19/comments":
                    case.writes.append(("record", 19))
                    case.issue_comments.append(comment)
                    return self.respond(comment, 201)
                if self.path == "/api/v3/repos/project/app/pulls/12/comments/502/replies":
                    case.writes.append(("reply", 502))
                    case.replies.append({**comment, "in_reply_to_id": 502})
                    if case.close_after_reply:
                        case.issue["state"] = "closed"
                    return self.respond(comment, 201)
                case.writes.append(("unexpected", self.path))
                self.respond({"message": "unrecognized write"}, 404)

        self.tls = LocalTlsFixture(Handler)
        self.addCleanup(self.tls.close)

    def git(self, *args):
        return subprocess.check_output(["git", *args], cwd=self.repo, text=True).strip()

    @staticmethod
    def thread(identity, comment_id, body):
        return {"id": identity, "isResolved": False, "viewerCanResolve": True,
                "path": "ui/feed.rs", "line": None, "originalLine": 17, "comments": {
                    "nodes": [{"databaseId": comment_id, "body": body}],
                }}

    def invoke(self, *args):
        return subprocess.run(
            [self.binary, "defer-findings", "--hostname", f"localhost:{self.tls.port}",
             "--repo", "project/app", "--pr", "12", *args],
            cwd=self.repo, env={**os.environ, "GH_ENTERPRISE_TOKEN": "fixture",
                                "SSL_CERT_FILE": str(self.tls.ca_crt),
                                "NO_PROXY": "localhost", "no_proxy": "localhost"},
            capture_output=True, text=True, timeout=15,
        )

    def test_preview_and_transfer_only_the_selected_finding_across_pages(self):
        preview = self.invoke()
        self.assertEqual(preview.returncode, 0, preview.stderr)
        self.assertIn("Follow-up #19", preview.stdout)
        self.assertIn("production", preview.stdout)
        self.assertIn("presentation", preview.stdout)
        self.assertEqual(self.writes, [])
        result = self.invoke("--thread", "presentation", "--apply")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.writes, [("record", 19), ("reply", 502), ("resolve", "presentation")])
        record = self.issue_comments[0]["body"]
        for expected in ["- [ ]", "ui/feed.rs:17", "Tune the label contrast", "/pull/12#discussion_r502"]:
            self.assertIn(expected, record)
        self.assertNotIn("loses data", record)
        self.assertIn("#19", self.replies[0]["body"])
        self.assertFalse(self.threads[0]["isResolved"])

    def test_closed_follow_up_keeps_the_review_thread_unresolved(self):
        self.close_after_reply = True
        result = self.invoke("--apply", "--thread", "presentation")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("must be open", result.stderr)
        self.assertEqual(self.writes, [("record", 19), ("reply", 502)])
        self.assertFalse(self.threads[1]["isResolved"])

    def test_resolution_failure_retries_without_duplicate_issue_entry_or_reply(self):
        self.fail_resolution = True
        args = ("--apply", "--thread", "presentation")
        first = self.invoke(*args)
        self.assertNotEqual(first.returncode, 0)
        self.assertEqual(self.writes, [("record", 19), ("reply", 502), ("resolve", "presentation")])
        self.assertFalse(self.threads[1]["isResolved"])
        self.fail_resolution = False
        self.writes.clear()
        second = self.invoke(*args)
        self.assertEqual(second.returncode, 0, second.stderr)
        self.assertEqual(self.writes, [("resolve", "presentation")])
        self.assertEqual(len(self.issue_comments), 1)
        self.assertEqual(len(self.replies), 1)
        self.writes.clear()
        third = self.invoke(*args)
        self.assertEqual(third.returncode, 0, third.stderr)
        self.assertEqual(self.writes, [])

    def test_ineligible_pull_issue_and_thread_leave_both_records_untouched(self):
        args = ["--apply", "--thread", "presentation"]
        provisional = self.head
        self.head = self.settled
        self.assertNotEqual(self.invoke(*args).returncode, 0)
        self.head = provisional
        self.issue["labels"] = []
        self.assertNotEqual(self.invoke(*args).returncode, 0)
        self.issue["labels"] = [{"name": "provisional"}]
        self.assertNotEqual(self.invoke(*args, "--thread", "another-pr").returncode, 0)
        self.threads[0]["viewerCanResolve"] = False
        self.assertNotEqual(self.invoke(*args, "--thread", "production").returncode, 0)
        self.assertEqual(self.writes, [])
