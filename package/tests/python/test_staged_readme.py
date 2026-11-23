"""Exercise the repository hook with real Git indexes and a tiny offline Rust renderer."""

from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import unittest


HOOK = Path(__file__).resolve().parents[3] / ".github/scripts/stage-readme.py"


class StagedReadme(unittest.TestCase):
    def setUp(self):
        scratch = tempfile.TemporaryDirectory()
        self.addCleanup(scratch.cleanup)
        self.root = Path(scratch.name) / "project"
        self.root.mkdir()
        self.git("init", "-qb", "trunk")
        self.git("config", "user.name", "Example")
        self.git("config", "user.email", "example@example.test")
        self.git("config", "commit.gpgsign", "false")
        hooks = self.root / ".git/hooks"
        self.git("config", "core.hooksPath", str(hooks))
        hook = hooks / "pre-commit"
        hook.write_text("#!/bin/sh\nexec " + shlex.join([sys.executable, str(HOOK)]) + "\n")
        hook.chmod(0o755)
        self.write(".gitignore", "target/")
        self.write("INSTALL.md", "initial")
        self.write("package/Cargo.toml", '[package]\nname="fixture"\nversion="0.1.0"\nedition="2021"')
        self.write("package/src/lib.rs", "")
        self.write("package/examples/readme.rs", 'fn main() { print!("{}", include_str!("../../INSTALL.md")); }')
        subprocess.run(["cargo", "generate-lockfile", "--offline"], cwd=self.root / "package", check=True)
        self.git("add", ".")
        self.git("commit", "-qm", "initial")
        self.assertEqual(self.git("show", "HEAD:README.md"), "initial")

    def write(self, name, text):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text + "\n", encoding="utf-8")

    def git(self, *args):
        return subprocess.run(["git", *args], cwd=self.root, text=True, capture_output=True,
                              check=True).stdout.strip()

    def test_preserves_unstaged_sources_and_readme_in_linked_worktree(self):
        worktree = self.root.parent / "linked"
        self.git("worktree", "add", "-qb", "linked", str(worktree))
        self.root = worktree
        self.write("INSTALL.md", "staged")
        self.git("add", "INSTALL.md")
        self.write("INSTALL.md", "local source edit")
        self.write("README.md", "local README edit")
        self.git("commit", "-qm", "stage only intended content")
        self.assertEqual(self.git("show", "HEAD:README.md"), "staged")
        self.assertEqual(self.git("show", "HEAD:INSTALL.md"), "staged")
        self.assertEqual((worktree / "INSTALL.md").read_text().strip(), "local source edit")
        self.assertEqual((worktree / "README.md").read_text().strip(), "local README edit")
        self.assertEqual(self.git("show", "trunk:README.md"), "initial")

    def test_path_limited_commit_cannot_leave_staged_reversal(self):
        self.write("INSTALL.md", "changed")
        head = self.git("rev-parse", "HEAD")
        attempt = subprocess.run(["git", "commit", "-qm", "limited", "--", "INSTALL.md"],
                                 cwd=self.root, text=True, capture_output=True)
        self.assertNotEqual(attempt.returncode, 0)
        self.assertIn("git commit without paths", attempt.stderr)
        self.assertEqual(self.git("rev-parse", "HEAD"), head)
        self.assertEqual(self.git("diff", "--cached", "--name-only"), "")
        self.git("commit", "-qam", "all intended changes")
        self.assertEqual(self.git("show", "HEAD:README.md"), "changed")
        self.assertEqual(self.git("status", "--porcelain"), "")
        self.write("package/src/lib.rs", "// no change to generated output")
        self.git("commit", "-qm", "limited, current homepage", "--", "package/src/lib.rs")
        self.assertEqual(self.git("status", "--porcelain"), "")
