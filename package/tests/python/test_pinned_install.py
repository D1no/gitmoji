"""Install from simulated GitHub releases with a standard shell and no language runtimes.

The downloader uses the real curl or wget against a loopback HTTP server. Distinct release
directories catch fetching the installer's own version instead of the project's pin.
"""
from functools import partial
import hashlib
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import tempfile
from threading import Thread
import unittest

ROOT = Path(__file__).resolve().parents[3]
INSTALLER = ROOT / "install.sh"
BOOTSTRAP = "2.0.0"
PINNED = "1.2.3"
PLATFORMS = (
    ("Linux", "x86_64", "gitmoji-x86_64-unknown-linux-musl"),
    ("Linux", "aarch64", "gitmoji-aarch64-unknown-linux-musl"),
    ("Darwin", "arm64", "gitmoji-aarch64-apple-darwin"),
    ("Darwin", "x86_64", "gitmoji-x86_64-apple-darwin"),
    ("MINGW64_NT", "x86_64", "gitmoji-x86_64-pc-windows-msvc.exe"),
)
COMMON = ("sh", "cat", "mktemp", "rm", "mkdir", "awk", "chmod", "cmp", "cp", "mv",
          "sed", "head", "dirname")
OTHER_TOOL = "#!/bin/sh\necho gitmoji-cli\n"


def executable(path, content):
    path.write_text(content)
    path.chmod(0o755)
    return path


def native_reader():
    candidates = [shutil.which("gitmoji"), ROOT / "package/target/debug/gitmoji"]
    for path in candidates:
        if path and Path(path).is_file() and not subprocess.run(
                [path, "pin", "--help"], capture_output=True).returncode:
            return str(Path(path).resolve())
    return None


NATIVE = native_reader()


class Releases(SimpleHTTPRequestHandler):
    def log_message(self, *args):
        pass


@unittest.skipUnless(os.name == "posix", "the standalone installer is a shell script")
class PinnedInstall(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        release_dir = tempfile.TemporaryDirectory()
        cls.addClassCleanup(release_dir.cleanup)
        cls.host = Path(release_dir.name)
        server = ThreadingHTTPServer(("127.0.0.1", 0), partial(Releases, directory=cls.host))
        cls.url = f"http://127.0.0.1:{server.server_port}"
        thread = Thread(target=server.serve_forever, daemon=True)
        thread.start()
        cls.addClassCleanup(thread.join)
        cls.addClassCleanup(server.server_close)
        cls.addClassCleanup(server.shutdown)

    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        self.tools = self.root / "tools"
        self.scratch = self.root / "tmp"
        self.home = self.root / "home"
        self.project = self.root / "project with spaces"
        self.bin = self.root / "installed commands"
        self.log = self.root / "downloads"
        self.releases = self.host / self.root.name
        self.addCleanup(shutil.rmtree, self.releases)
        for directory in (self.tools, self.scratch, self.home, self.project, self.releases):
            directory.mkdir()
        (self.project / ".git").mkdir()
        self.installer = self.root / "install.sh"
        self.installer.write_text(INSTALLER.read_text().replace("installer_release=''",
                                                                 f"installer_release='{BOOTSTRAP}'"))
        self.env = dict(HOME=str(self.home), PATH=str(self.tools), TMPDIR=str(self.scratch),
                        GITMOJI_BIN=str(self.bin), PIN_FOR_TEST=PINNED)
        for tool in COMMON:
            (self.tools / tool).symlink_to(shutil.which(tool))
        checker = "sha256sum" if shutil.which("sha256sum") else "shasum"
        (self.tools / checker).symlink_to(shutil.which(checker))
        self.downloader("curl")
        self.platform(*PLATFORMS[0][:2])
        for version in (BOOTSTRAP, PINNED):
            self.release(version)

    def downloader(self, name):
        for downloader in ("curl", "wget"):
            (self.tools / downloader).unlink(missing_ok=True)
        path = shutil.which(name)
        if not path:
            self.skipTest(f"{name} is not installed on this test host")
        # Both adapters are invoked with '-flags -o/-O dest URL'. Accept only the public
        # release URL and rewrite its origin for this test. Keep the actual HTTP options.
        origin = "https://github.com/D1no/gitmoji/releases/download/"
        executable(self.tools / name, f"""#!/bin/sh
case "$4" in
  {origin}*) relative=${{4#{origin}}} ;;
  *) echo "unexpected URL: $4" >&2; exit 92 ;;
esac
printf '%s\\n' "$relative" >> {shlex.quote(str(self.log))}
exec {shlex.quote(path)} "$1" "$2" "$3" "{self.url}/{self.root.name}/$relative"
""")

    def platform(self, os_name, arch):
        executable(self.tools / "uname",
                   f'#!/bin/sh\ncase "$1" in -s) echo {os_name} ;; -m) echo {arch} ;; esac\n')

    def release(self, version, reader=None):
        folder = self.releases / f"v{version}"
        folder.mkdir(exist_ok=True)
        if reader:
            pin = f"exec {shlex.quote(reader)} pin"
        else:
            pin = 'printf "%s\\n" "$PIN_FOR_TEST"'
        tool = f"""#!/bin/sh
case "$1" in
  --version) echo 'gitmoji {version} (gitmoji-triangle)' ;;
  pin) [ "${{2:-}}" = --help ] && exit 0; {pin} ;;
  *) exit 2 ;;
esac
"""
        sums = []
        for _, _, asset in PLATFORMS:
            content = (tool + f"# {asset}\n").encode()
            (folder / asset).write_bytes(content)
            sums.append(f"{hashlib.sha256(content).hexdigest()}  {asset}\n")
        (folder / "SHA256SUMS").write_text("".join(sums))

    def run_install(self, *args, script=None, stdin_script=False, cwd=None, env=None):
        if stdin_script:
            cmd, content = ["sh", "-s", "--", *args], self.installer.read_text()
        else:
            cmd, content = ["sh", str(script or self.installer), *args], ""
        result = subprocess.run(cmd, input=content, env=self.env | (env or {}),
                                cwd=cwd or self.project, capture_output=True, text=True)
        self.assertEqual(list(self.scratch.iterdir()), [], "temporary download was left behind")
        return result

    def test_standalone_installs_the_projects_release_on_each_platform(self):
        for os_name, arch, asset in PLATFORMS:
            with self.subTest(os=os_name, arch=arch):
                self.platform(os_name, arch)
                result = self.run_install("--pinned", stdin_script=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                name = "gitmoji.exe" if asset.endswith(".exe") else "gitmoji"
                self.assertEqual((self.bin / name).read_bytes(),
                                 (self.releases / f"v{PINNED}" / asset).read_bytes())
                self.assertIn("not on your PATH", result.stdout)

    def test_wget_alone_and_shasum_can_bootstrap(self):
        self.downloader("wget")
        if not shutil.which("shasum"):
            self.skipTest("no shasum on this test host")
        (self.tools / "sha256sum").unlink(missing_ok=True)
        if not (self.tools / "shasum").exists():
            (self.tools / "shasum").symlink_to(shutil.which("shasum"))
        result = self.run_install("--pinned")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(PINNED, result.stdout)

    def test_idempotent_installs_preserve_the_verified_binary(self):
        # An explicit destination works even when the caller hasn't added it to PATH yet.
        first = self.run_install("--pinned", env={"PIN_FOR_TEST": BOOTSTRAP})
        self.assertEqual(first.returncode, 0, first.stderr)
        tool = self.bin / "gitmoji"
        before = tool.stat()
        self.assertEqual(len(self.log.read_text().splitlines()), 2, "reuse the verified reader")
        second = self.run_install("--pinned", env={"PIN_FOR_TEST": BOOTSTRAP})
        self.assertEqual(second.returncode, 0, second.stderr)
        self.assertIn("Already installed", second.stdout)
        self.assertEqual(self.log.read_text().splitlines()[2:], [f"v{BOOTSTRAP}/SHA256SUMS"])
        self.assertEqual((tool.stat().st_ino, tool.stat().st_mtime_ns),
                         (before.st_ino, before.st_mtime_ns))

    def test_collision_on_path_is_preserved_unless_explicitly_replaced(self):
        other = executable(self.tools / "gitmoji", OTHER_TOOL)
        del self.env["GITMOJI_BIN"]
        for overwrite in ("", "0", "false"):
            with self.subTest(overwrite=overwrite):
                result = self.run_install("--pinned", env={"GITMOJI_OVERWRITE": overwrite})
                self.assertEqual(result.returncode, 3, result.stderr)
                self.assertEqual(other.read_text(), OTHER_TOOL)
        result = self.run_install("--pinned", env={"GITMOJI_OVERWRITE": "1"})
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(f"gitmoji {PINNED} (gitmoji-triangle)", other.read_text())

    def test_failure_verifying_either_release_preserves_the_existing_install(self):
        self.bin.mkdir()
        old = executable(self.bin / "gitmoji", OTHER_TOOL)
        for version in (BOOTSTRAP, PINNED):
            with self.subTest(version=version):
                asset = self.releases / f"v{version}" / PLATFORMS[0][2]
                asset.write_text("#!/bin/sh\nexit 93\n")
                result = self.run_install("--pinned", "--overwrite")
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("did not match the release's checksum", result.stderr)
                self.assertEqual(old.read_text(), OTHER_TOOL)
                self.release(version)

    def test_missing_and_duplicate_checksums_are_refused(self):
        sums = self.releases / f"v{PINNED}" / "SHA256SUMS"
        manifest = sums.read_text()
        for broken in ("", manifest + manifest):
            sums.write_text(broken)
            result = self.run_install("--pinned")
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("one checksum", result.stderr)
            self.assertFalse(self.bin.exists())

    def test_invalid_pin_stops_before_another_download(self):
        for value in ("trunk", "1.2.3.4", "../0.9", "1..3", "1.2.3\n2.0.0"):
            self.log.unlink(missing_ok=True)
            result = self.run_install("--pinned", env={"PIN_FOR_TEST": value})
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("other than a release number", result.stderr)
            self.assertFalse(self.bin.exists())
            self.assertTrue(all(p.startswith(f"v{BOOTSTRAP}/")
                                for p in self.log.read_text().splitlines()))

    @unittest.skipUnless(NATIVE, "needs the built Gitmoji pin reader")
    def test_native_pin_handles_toml_and_settings_precedence_with_no_git(self):
        for version in (BOOTSTRAP, PINNED):
            self.release(version, reader=NATIVE)
        (self.project / ".git").rmdir()
        (self.project / ".git").write_text("gitdir: example\n")  # worktree marker
        nested = self.project / "src"
        nested.mkdir()
        legacy = self.project / "triangle.toml"
        legacy.write_text(f"[vocabulary]\r\nversion = '{BOOTSTRAP}'\r\n")
        config = self.project / ".config"
        config.mkdir()
        hidden = config / "gitmoji.toml"
        hidden.write_text(f'vocabulary.version = "{PINNED}"\n')
        root = self.project / "gitmoji.toml"
        for path, content, expected in (
            (root, f'vocabulary = {{ version = "{BOOTSTRAP}" }}\n', BOOTSTRAP),
            (root, None, PINNED),
            (hidden, None, BOOTSTRAP),
        ):
            if content is None:
                path.unlink()
            else:
                path.write_text(content)
            result = self.run_install("--pinned", cwd=nested)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn(expected, result.stdout)
        for broken in ("[vocabulary\n", "version = 42\n", 'vocabulary.version = "trunk"\n'):
            legacy.write_text(broken)
            result = self.run_install("--pinned", cwd=nested)
            self.assertNotEqual(result.returncode, 0)
            self.assertRegex(result.stderr, r"version|release number")

    def test_a_checkout_can_still_install_its_own_version(self):
        crate = (ROOT / "package/Cargo.toml").read_text().split('version = "', 1)[1].split('"', 1)[0]
        self.release(crate)
        result = self.run_install(script=INSTALLER)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(f"gitmoji {crate}", result.stdout)

    def test_foreign_tool_in_an_explicit_destination_is_protected(self):
        self.bin.mkdir()
        old = executable(self.bin / "gitmoji", OTHER_TOOL)
        result = self.run_install("--pinned")
        self.assertEqual(result.returncode, 3, result.stderr)
        self.assertEqual(old.read_text(), OTHER_TOOL)
        replaced = self.run_install("--pinned", "--overwrite")
        self.assertEqual(replaced.returncode, 0, replaced.stderr)


if __name__ == "__main__":
    unittest.main()
