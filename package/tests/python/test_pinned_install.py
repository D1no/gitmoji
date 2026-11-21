"""Exercise the CI bootstrap against an annotated release in a local upstream."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

INSTALLER = Path(__file__).resolve().parents[3] / "install.sh"

class PinnedInstallTests(unittest.TestCase):
    def test_settings_precedence_and_invalid_pin(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            upstream, project = root / "upstream", root / "project"
            env = dict(os.environ, GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull,
                       GIT_CONFIG_COUNT="1", GIT_CONFIG_KEY_0=f"url.{upstream.as_uri()}.insteadOf",
                       GIT_CONFIG_VALUE_0="https://github.com/D1no/gitmoji.git")
            for repo in (upstream, project):
                subprocess.run(["git", "init", "-q", str(repo)], env=env, check=True)
            install = upstream / "install.sh"
            install.write_text("#!/bin/sh\ngit -C \"$(dirname \"$0\")\" log -1 --format=%s\n")
            install.chmod(0o755)
            git = ["git", "-C", str(upstream), "-c", "user.name=Tester", "-c",
                   "user.email=test@example.com", "-c", "commit.gpgsign=false"]
            for args in (["add", "install.sh"], ["commit", "-qm", "release installer"],
                         ["tag", "-am", "release", "v1.2.3"]):
                subprocess.run(git + args, env=env, check=True)
            command = ["sh", str(INSTALLER), "--pinned"]
            (project / ".config").mkdir()
            settings = '[vocabulary]\nversion = "1.2.3"\n'
            (project / ".config/gitmoji.toml").write_text(settings)
            result = subprocess.run(command, cwd=project / ".config", env=env, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout.strip(), "release installer")
            self.assertNotIn("is not a commit", result.stderr)
            (project / "gitmoji.toml").write_text('[vocabulary]\nversion = "trunk"\n')
            result = subprocess.run(command, cwd=project, env=env, capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("Expected a release number", result.stderr)
