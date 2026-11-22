"""Exercise prepare-commit-msg through Git and verify persisted merge objects."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


class LocalMerges(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.folder = Path(temp.name) / "work"
        self.folder.mkdir()
        self.git("init", "-q", "-b", "trunk")
        self.git("config", "user.name", "Contributor")
        self.git("config", "user.email", "author@example.invalid")
        self.git("config", "commit.gpgsign", "false")
        self.git("config", "core.hooksPath", ".git/hooks")
        self.git("commit", "--allow-empty", "-qm", "initial")
        self.prepare = self.folder / ".git/hooks/prepare-commit-msg"
        self.prepare.write_text('#!/bin/sh\ngitmoji merge-message "$1" "${2:-}"\n', encoding="utf-8")
        self.prepare.chmod(0o755)

    def run_in_repo(self, *args, check=True):
        result = subprocess.run(args, cwd=self.folder, text=True, stdout=subprocess.PIPE,
                                stderr=subprocess.PIPE, env={
                                    **os.environ, "GIT_EDITOR": "true", "GIT_MERGE_AUTOEDIT": "no",
                                    "GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_GLOBAL": os.devnull,
                                })
        if check and result.returncode:
            self.fail(f"{args}: {result.stderr}{result.stdout}")
        return result

    def git(self, *args, check=True):
        return self.run_in_repo("git", *args, check=check).stdout.strip()

    def commit(self, title, path=None, value="x"):
        if path:
            (self.folder / path).write_text(value, encoding="utf-8")
            self.git("add", path)
        self.git("commit", "--allow-empty", "-qm", title)

    def message(self):
        return self.git("log", "-1", "--format=%B")

    def test_conflict_and_custom_comment_character(self):
        self.git("config", "core.commentChar", ";")
        self.commit("set base", "shared", "base")
        self.git("switch", "-qc", "feature")
        self.commit("🎨✨ #7 (ui): add preview", "shared", "feature")
        self.git("switch", "trunk")
        self.commit("change default", "shared", "trunk")
        result = self.run_in_repo("git", "merge", "feature", check=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertTrue((self.folder / ".git/MERGE_HEAD").exists())
        (self.folder / "shared").write_text("resolved", encoding="utf-8")
        self.git("add", "shared")
        self.git("commit", "--no-edit")
        self.assertTrue(self.message().startswith("[`🎨✨`] feature into trunk: add preview\n\n"))
        self.assertIn("Conflicted paths:\n- shared", self.message())
        self.assertNotIn("; Conflicts:", self.message())

    def test_nested_incoming_pr_excludes_shared_ancestry(self):
        self.git("switch", "-qc", "workspace")
        self.git("switch", "-qc", "piece")
        self.commit("🎨✨ #1 (ui): show activity")
        self.git("switch", "workspace")
        self.git("merge", "--no-ff", "piece", "-m", "Merge pull request #4 from upstream/piece", "-m", "Show activity")
        self.git("switch", "trunk")
        self.git("update-ref", "refs/remotes/origin/workspace", "workspace")
        self.git("merge", "--no-ff", "origin/workspace")
        self.assertEqual(self.message().splitlines()[0], "[`🎨✨`] workspace into trunk: #4 Show activity")
        self.assertIn("Arrived from workspace (1):", self.message())
        self.assertIn("Merge pull request #4 from upstream/piece", self.message())
        self.assertEqual(len(self.git("show", "-s", "--format=%P").split()), 2)

    def test_several_incoming_pr_numbers_lead_their_own_titles(self):
        self.git("switch", "-qc", "workspace")
        for i, title in [(1, "[`🎨✨`] First (#1)"), (2, "[`🎨✨`] #2 Second (#9) (#2)")]:
            self.git("switch", "-qc", f"part-{i}")
            self.commit(f"🎨✨ #{i} (ui): show part")
            self.git("switch", "workspace")
            self.git("merge", "--no-ff", f"part-{i}", "-m", title)
        self.git("switch", "trunk")
        self.git("merge", "--no-ff", "workspace")
        self.assertEqual(self.message().splitlines()[0],
                         "[`🎨✨`] workspace into trunk: #2 Second (#9); #1 First")
        self.assertIn(" [`🎨✨`] #2 Second (#9) (#2)", self.message())

    def test_git_subject_ticket_survives_without_claiming_a_landing(self):
        self.git("switch", "-qc", "feature")
        original = "🎨✨ #7 (ui): Show activity (#77)"
        self.commit(original)
        self.git("switch", "trunk")
        self.git("merge", "--no-ff", "feature")
        self.assertEqual(self.message().splitlines()[0], "[`🎨✨`] feature into trunk: Show activity (ref #77)")
        self.assertIn(original, self.message().split("\n\n")[1])

    def test_single_pr_with_a_distinct_trailing_issue(self):
        self.git("switch", "-qc", "workspace")
        self.git("switch", "-qc", "piece")
        self.commit("🎨✨ #77 (ui): show activity")
        self.git("switch", "workspace")
        self.git("merge", "--no-ff", "piece", "-m", "Merge pull request #4 from upstream/piece",
                 "-m", "[`🎨✨`] #4 Show activity (#77)")
        self.git("switch", "trunk")
        self.git("merge", "--no-ff", "workspace")
        self.assertEqual(self.message().splitlines()[0],
                         "[`🎨✨`] workspace into trunk: #4 Show activity (ref #77)")

    def test_octopus_shared_ancestor_and_bounded_body(self):
        self.git("switch", "-qc", "common")
        self.commit("🧩👔 #2 (server): set policy")
        self.git("switch", "-qc", "first")
        self.commit("🎨✨ #3 (ui): show preview", "first")
        self.git("switch", "-qc", "second", "common")
        self.commit("🧩🛡️ #4 (server): guard update", "second")
        self.git("switch", "trunk")
        self.git("merge", "--no-ff", "first", "second")
        headline, _, body = self.message().partition("\n\n")
        self.assertIn("first, second into trunk: 3 commits", headline)
        for tag in ["🎨", "🧩", "👔", "✨", "🛡️"]:
            self.assertEqual(headline.count(tag), 1)
        self.assertEqual(len([line for line in body.splitlines() if line.startswith("- ")]), 3)
        self.git("switch", "-qc", "long")
        for i in range(24):
            self.commit(f"🧩👔 #2 (server): adjust rule {i}")
        self.git("switch", "trunk")
        self.git("merge", "--no-ff", "long")
        self.assertIn("Arrived from long (24):", self.message())
        self.assertEqual(self.message().count("\n- "), 20)
        self.assertIn("… and 4 more", self.message())

    def test_no_commit_twice_and_legacy_squash(self):
        self.git("switch", "-qc", "squashed")
        self.commit("[`🎨✨`] Show preview (#2)")
        self.git("switch", "trunk")
        self.git("merge", "--no-ff", "--no-commit", "squashed")
        self.run_in_repo("gitmoji", "merge-message", ".git/MERGE_MSG", "merge")
        path = self.folder / ".git/MERGE_MSG"
        original = path.read_bytes()
        self.run_in_repo("gitmoji", "merge-message", ".git/MERGE_MSG", "merge")
        self.assertEqual(path.read_bytes(), original)
        self.git("commit", "--no-edit")
        self.assertEqual(self.message().splitlines()[0], "squashed into trunk: #2 Show preview")
        self.assertIn("[`🎨✨`] Show preview (#2)", self.message().split("\n\n", 1)[1])

    def test_untagged_and_manual_message(self):
        self.git("switch", "-qc", "legacy")
        self.commit("Older feature")
        self.git("switch", "trunk")
        self.git("merge", "--no-ff", "legacy")
        self.assertEqual(self.message().splitlines()[0], "legacy into trunk: Older feature")
        self.git("reset", "--hard", "HEAD^")
        self.git("merge", "--no-ff", "legacy", "-m", "Merge the supported variants", "-m", "Keep this exact body.")
        self.assertEqual(self.message(), "Merge the supported variants\n\nKeep this exact body.")

    def test_pull_fetch_head(self):
        upstream = self.folder.parent / "upstream"
        self.git("clone", "--bare", ".", str(upstream))
        self.git("remote", "add", "origin", str(upstream))
        self.git("switch", "-qc", "peer")
        self.commit("🎨✨ #7 (ui): add preview")
        self.git("push", "origin", "peer:trunk")
        self.git("switch", "trunk")
        self.commit("🧩👔 #2 (server): adjust policy")
        self.git("pull", "--no-rebase", "origin", "trunk")
        self.assertEqual(self.message().splitlines()[0], "[`🎨✨`] fetched trunk into trunk: add preview")

    def test_git_comment_prefix_does_not_remove_the_subject(self):
        self.git("switch", "-qc", "#12-source")
        self.commit("Legacy change")
        self.git("switch", "trunk")
        self.git("merge", "--no-ff", "--edit", "#12-source")
        self.assertEqual(self.message().splitlines()[0], "'#12-source' into trunk: Legacy change")
        self.git("config", "core.commentChar", "[")
        self.git("switch", "-qc", "tagged")
        self.commit("🎨✨ #8 (ui): add preview")
        self.git("switch", "trunk")
        self.git("merge", "--no-ff", "--edit", "tagged")
        self.assertEqual(self.message().splitlines()[0], "Merge branch 'tagged' into trunk")

    def test_explicit_heading_for_a_different_branch_is_preserved(self):
        self.git("switch", "-qc", "source")
        self.commit("some work")
        self.git("switch", "trunk")
        self.git("branch", "other")
        title = "Merge branch 'other' into trunk"
        self.git("merge", "--no-ff", "-m", title, "source")
        self.assertEqual(self.message(), title)

    def test_source_name_matches_fetched_target(self):
        self.git("switch", "-qc", "source")
        self.commit("Changes")
        self.git("switch", "trunk")
        self.git("update-ref", "refs/remotes/origin/trunk", "source")
        self.git("merge", "--no-ff", "origin/trunk")
        self.assertEqual(self.message().splitlines()[0], "origin/trunk into trunk: Changes")

    def test_historical_direction_must_be_recorded(self):
        self.git("switch", "-qc", "topic")
        self.commit("🎨✨ #7 (ui): add preview")
        self.git("switch", "trunk")
        self.prepare.unlink()
        self.git("merge", "--no-ff", "topic")
        before = self.git("rev-parse", "HEAD")
        preview = self.run_in_repo("gitmoji", "merge-message", "--historical", before).stdout
        self.assertTrue(preview.startswith("[`🎨✨`] topic into trunk: add preview\n\nArrived from topic (1):"))
        self.assertEqual(before, self.git("rev-parse", "HEAD"))
        for title in ["Merge branch 'topic'", "Unusual title"]:
            self.git("commit", "--amend", "-m", title)
            self.assertEqual(self.run_in_repo("gitmoji", "merge-message", "--historical", "HEAD").stdout, "")
        self.assertEqual(self.run_in_repo("gitmoji", "merge-message", "--historical", "topic").stdout, "")
