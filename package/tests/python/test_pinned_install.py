"""Exercise the CI bootstrap against annotated releases in a local upstream.

`install.sh --pinned` asks a gitmoji for the project's adopted release (`gitmoji pin`),
then fetches that release's tag and runs its installer. The upstream here answers for the
public address, its installer is a stand-in that prints which release it belongs to, and
release files come from a local folder, so nothing leaves the machine.
"""
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

INSTALLER = Path(__file__).resolve().parents[3] / "install.sh"
PIN = '[vocabulary]\nversion = "1.2.3"\n'
ASSETS = ("gitmoji-aarch64-apple-darwin", "gitmoji-x86_64-apple-darwin",
          "gitmoji-aarch64-unknown-linux-musl", "gitmoji-x86_64-unknown-linux-musl",
          "gitmoji-x86_64-pc-windows-msvc.exe")
# What the bootstrap and the installer it calls run, besides the shell's own builtins.
TOOLS = ("sh", "git", "mktemp", "rm", "dirname", "cat", "mkdir", "sed", "head", "uname",
         "curl", "grep", "cut", "chmod", "cp", "mv")
POSIX = os.name == "posix"

# A stand-in command for the paths that do not depend on how settings are read.
FAKE = """#!/bin/sh
case "$1" in
  --version) echo "gitmoji 9.9.9 (gitmoji-triangle)" ;;
  pin)
    [ "${2:-}" = --help ] && exit 0
    pwd >> "$FAKE_LOG"
    if [ -n "${FAKE_PIN_FAIL:-}" ]; then echo "$FAKE_PIN_FAIL" >&2; exit 7; fi
    printf '%s\\n' "$FAKE_PIN"
    ;;
  *) exit 2 ;;
esac
"""
WITHOUT_PIN = """#!/bin/sh
case "$1" in
  --version) echo "gitmoji 0.16.0 (gitmoji-triangle)" ;;
  *) echo "error: unrecognized subcommand" >&2; exit 2 ;;
esac
"""
ANOTHER_TOOL = "#!/bin/sh\necho 'gitmoji-cli 9.0.0'\n"


def real_gitmoji():
    """The command under test when it is on PATH and already has `pin`."""
    found = shutil.which("gitmoji")
    if found and subprocess.run([found, "pin", "--help"], capture_output=True).returncode == 0:
        return found
    return None


REAL = real_gitmoji()


class Fixture(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.root = root = Path(tmp.name)
        self.upstream, self.project = root / "upstream", root / "project"
        self.scratch, self.home = root / "scratch", root / "home"
        self.caller_bin, self.assets = root / "caller-bin", root / "assets"
        for folder in (self.scratch, self.home, self.caller_bin, self.assets):
            folder.mkdir()
        # Nothing of the machine's git settings, and the public address answered locally.
        self.env = dict(os.environ, GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull,
                        GIT_CONFIG_COUNT="1",
                        GIT_CONFIG_KEY_0=f"url.{self.upstream.as_uri()}.insteadOf",
                        GIT_CONFIG_VALUE_0="https://github.com/D1no/gitmoji.git",
                        HOME=str(self.home), TMPDIR=str(self.scratch),
                        GITMOJI_BIN=str(self.caller_bin),
                        GITMOJI_RELEASES=self.assets.as_uri(),
                        FAKE_LOG=str(root / "pin-ran-in"))
        self.env.pop("GITMOJI_OVERWRITE", None)
        for repo in (self.upstream, self.project):
            subprocess.run(["git", "init", "-q", str(repo)], env=self.env, check=True)
        install = self.upstream / "install.sh"
        install.write_text("#!/bin/sh\ngit -C \"$(dirname \"$0\")\" log -1 --format=%s\n")
        install.chmod(0o755)
        git = ["git", "-C", str(self.upstream), "-c", "user.name=Tester", "-c",
               "user.email=test@example.com", "-c", "commit.gpgsign=false"]
        subprocess.run(git + ["add", "install.sh"], env=self.env, check=True)
        for release in ("1.2.3", "2.0.0"):
            subprocess.run(git + ["commit", "-q", "--allow-empty", "-m", f"installer of {release}"],
                           env=self.env, check=True)
            subprocess.run(git + ["tag", "-am", "release", f"v{release}"], env=self.env, check=True)
        # A python that records being called, ahead of everything else on PATH.
        self.python_called = root / "python-was-called"
        self.shims = root / "shims"
        self.shims.mkdir()
        for name in ("python3", "python", "py"):
            self.script(self.shims / name,
                        f"#!/bin/sh\necho called >> '{self.python_called.as_posix()}'\nexit 97\n")
        self.use_path(self.shims, *self.path_without_gitmoji())

    def script(self, path, text):
        path.write_text(text)
        path.chmod(0o755)
        return path

    def path_without_gitmoji(self):
        return [entry for entry in os.environ["PATH"].split(os.pathsep) if entry
                and not any((Path(entry) / name).exists() for name in ("gitmoji", "gitmoji.exe"))]

    def use_path(self, *entries):
        self.env["PATH"] = os.pathsep.join(str(entry) for entry in entries)

    def gitmoji_on_path(self, source):
        """Puts one gitmoji first on PATH: a script's text, or the path of a real binary."""
        folder = self.root / "tool-on-path"
        folder.mkdir(exist_ok=True)
        if isinstance(source, Path):
            tool = folder / source.name
            shutil.copy2(source, tool)
        else:
            tool = self.script(folder / "gitmoji", source)
        self.use_path(folder, self.env["PATH"])
        return tool

    def release_files(self, source, damage=False):
        """The files of a release as the installer downloads them, for every platform."""
        data = source.read_bytes() if isinstance(source, Path) else source.encode()
        sums = []
        for asset in ASSETS:
            (self.assets / asset).write_bytes(data + (b"\n# changed after release" if damage else b""))
            sums.append(f"{hashlib.sha256(data).hexdigest()}  {asset}\n")
        (self.assets / "SHA256SUMS").write_text("".join(sums))

    def only_named_tools(self, *more):
        """A PATH of links to the named tools and nothing else: no gitmoji, no python."""
        only = self.root / "only"
        only.mkdir()
        checker = "sha256sum" if shutil.which("sha256sum") else "shasum"
        for tool in TOOLS + (checker,):
            found = shutil.which(tool)
            self.assertIsNotNone(found, tool)
            (only / tool).symlink_to(found)
        self.use_path(only, *more)
        for absent in ("python3", "python", "gitmoji", "cargo"):
            self.assertIsNone(shutil.which(absent, path=self.env["PATH"]), absent)

    def settings(self, text, name="gitmoji.toml"):
        target = self.project / name
        target.parent.mkdir(exist_ok=True)
        target.write_bytes(text if isinstance(text, bytes) else text.encode())

    def pinned(self, cwd=None, **more):
        return subprocess.run(["sh", str(INSTALLER), "--pinned"], cwd=cwd or self.project,
                              env=dict(self.env, **more), capture_output=True, text=True)

    def assert_ran_installer_of(self, release, result):
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), f"installer of {release}")
        self.assertNotIn("is not a commit", result.stderr)
        self.assert_machine_untouched()

    def assert_nothing_fetched(self, result):
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "")
        self.assert_machine_untouched()

    def assert_machine_untouched(self):
        # The stand-in installer of a release installs nothing, so whatever appears in
        # these places would have come from the command fetched only to read the pin.
        self.assertEqual(list(self.caller_bin.iterdir()), [], "the caller's GITMOJI_BIN")
        self.assertEqual(list(self.home.iterdir()), [], "the home folder")
        self.assertEqual(list(self.scratch.iterdir()), [], "the temporary folder")
        self.assertFalse(self.python_called.exists(), "the bootstrap reached for python")


@unittest.skipUnless(POSIX, "the stand-in commands are shell scripts")
class PinnedInstallPlumbing(Fixture):
    def test_a_gitmoji_on_path_that_has_pin_is_asked_in_the_project(self):
        tool = self.gitmoji_on_path(FAKE)
        before = tool.read_bytes()
        (self.project / "deep").mkdir()
        result = self.pinned(cwd=self.project / "deep", FAKE_PIN="2.0.0")
        self.assert_ran_installer_of("2.0.0", result)
        asked_in = Path(self.env["FAKE_LOG"]).read_text().strip()
        self.assertEqual(Path(asked_in).resolve(), (self.project / "deep").resolve())
        self.assertEqual(tool.read_bytes(), before)
        self.assertEqual(list(self.assets.iterdir()), [], "nothing needed downloading")

    def test_what_pin_says_on_failure_is_passed_on(self):
        self.gitmoji_on_path(FAKE)
        result = self.pinned(FAKE_PIN_FAIL="No Gitmoji settings found in the project")
        self.assert_nothing_fetched(result)
        self.assertEqual(result.returncode, 7)
        self.assertEqual(result.stderr.strip(), "No Gitmoji settings found in the project")

    def test_an_answer_that_is_not_a_release_number_never_becomes_a_tag(self):
        self.gitmoji_on_path(FAKE)
        for answer in ("trunk", "1.2", "1.2.3.4", "v1.2.3", "1..3", ".2.3", "1.2.",
                       "1.2.3 --upload-pack=touch", "1.2.3\n2.0.0", " "):
            with self.subTest(answer):
                result = self.pinned(FAKE_PIN=answer)
                self.assert_nothing_fetched(result)
                self.assertIn("something other than a release number", result.stderr)

    def test_an_older_gitmoji_on_path_is_left_alone_and_the_reader_is_fetched(self):
        tool = self.gitmoji_on_path(WITHOUT_PIN)
        self.release_files(FAKE)
        self.assert_ran_installer_of("1.2.3", self.pinned(FAKE_PIN="1.2.3"))
        self.assertEqual(tool.read_text(), WITHOUT_PIN)

    def test_another_tool_called_gitmoji_is_left_alone_without_a_question(self):
        tool = self.gitmoji_on_path(ANOTHER_TOOL)
        self.release_files(FAKE)
        self.assert_ran_installer_of("1.2.3", self.pinned(FAKE_PIN="1.2.3"))
        self.assertEqual(tool.read_text(), ANOTHER_TOOL)

    def test_a_fresh_machine_without_gitmoji_or_python(self):
        self.only_named_tools()
        self.release_files(FAKE)
        self.assert_ran_installer_of("2.0.0", self.pinned(FAKE_PIN="2.0.0"))

    def test_release_files_that_fail_their_checksum_are_not_run(self):
        self.only_named_tools()
        self.release_files(FAKE, damage=True)
        result = self.pinned(FAKE_PIN="1.2.3")
        self.assert_nothing_fetched(result)
        self.assertIn("did not match the release's checksum", result.stderr)
        self.assertFalse(Path(self.env["FAKE_LOG"]).exists(), "the damaged file was never run")

    def test_no_release_files_and_no_way_to_build_stops_with_the_reason(self):
        self.only_named_tools()
        result = self.pinned(FAKE_PIN="1.2.3")
        self.assert_nothing_fetched(result)
        self.assertIn("Could not get a gitmoji", result.stderr)

    def test_a_fetched_command_without_pin_stops(self):
        self.only_named_tools()
        self.release_files(WITHOUT_PIN)
        result = self.pinned()
        self.assert_nothing_fetched(result)
        self.assertIn("cannot name a project's adopted release", result.stderr)


@unittest.skipUnless(REAL, "needs the built gitmoji with `pin` on PATH")
class PinnedInstallWithTheCommand(Fixture):
    """The script and the command agree, because the command does the reading."""

    VALID = {
        "the form init writes": PIN,
        "an inline table": 'vocabulary = { version = "1.2.3" }\n',
        "a dotted key": 'vocabulary.version = "1.2.3"\n',
        "a quoted key": '[vocabulary]\n"version" = "1.2.3"\n',
        "a literal string": "[vocabulary]\nversion = '2.0.0'\n",
        "a string over several lines elsewhere":
            '[scope]\nallowed = [\n  "app",\n]\n[vocabulary]\nversion = "2.0.0"\n',
        "Windows line ends": '[vocabulary]\r\nversion = "1.2.3"\r\n',
    }
    INVALID = {
        "broken TOML": '[vocabulary\nversion = "1.2.3"\n',
        "the key twice": '[vocabulary]\nversion = "1.2.3"\nversion = "2.0.0"\n',
        "not a release": '[vocabulary]\nversion = "trunk"\n',
        "a draft name": '[vocabulary]\nversion = "2.0.0-draft.22"\n',
        "no pin": '[rules]\nlength = "warn"\n',
    }

    def pin(self, cwd=None):
        return subprocess.run([REAL, "pin"], cwd=cwd or self.project, env=self.env,
                              capture_output=True, text=True)

    def test_the_release_fetched_is_the_one_pin_names(self):
        self.gitmoji_on_path(Path(REAL))
        for label, text in self.VALID.items():
            with self.subTest(label):
                self.settings(text)
                named = self.pin()
                self.assertEqual(named.returncode, 0, named.stderr)
                self.assert_ran_installer_of(named.stdout.strip(), self.pinned())

    def test_settings_pin_refuses_are_refused_the_same_way(self):
        self.gitmoji_on_path(Path(REAL))
        for label, text in self.INVALID.items():
            with self.subTest(label):
                self.settings(text)
                named = self.pin()
                self.assertNotEqual(named.returncode, 0)
                result = self.pinned()
                self.assert_nothing_fetched(result)
                self.assertEqual(result.returncode, named.returncode)
                self.assertEqual(result.stderr, named.stderr)

    def test_precedence_is_the_commands(self):
        self.gitmoji_on_path(Path(REAL))
        self.settings('[vocabulary]\nversion = "1.2.3"\n', "triangle.toml")
        self.assert_ran_installer_of(self.pin().stdout.strip(), self.pinned())
        self.settings('[vocabulary]\nversion = "2.0.0"\n', ".config/gitmoji.toml")
        self.assert_ran_installer_of(self.pin().stdout.strip(),
                                     self.pinned(cwd=self.project / ".config"))
        self.settings('[vocabulary]\nversion = "trunk"\n')
        self.assert_nothing_fetched(self.pinned())

    def test_a_project_without_settings(self):
        self.gitmoji_on_path(Path(REAL))
        named = self.pin()
        result = self.pinned()
        self.assert_nothing_fetched(result)
        self.assertEqual(result.stderr, named.stderr)

    def test_a_fresh_machine_fetches_the_command_and_reads_with_it(self):
        if POSIX:
            self.only_named_tools()
        self.release_files(Path(REAL))
        self.settings('vocabulary = { version = "2.0.0" }\n')
        self.assert_ran_installer_of("2.0.0", self.pinned())


if __name__ == "__main__":
    unittest.main()
