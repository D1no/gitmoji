"""Generate README.md from the staged project without staging other work."""

import os
import subprocess
import sys
import tempfile
from pathlib import Path


def git(*args, **options):
    return subprocess.run(["git", *args], check=True, stdout=subprocess.PIPE, **options).stdout


def render(project, target):
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(target)
    command = ["cargo", "run", "--quiet", "--locked", "--example", "readme", "--", "--stdout"]
    return subprocess.run(command, cwd=project / "package", env=env, check=True,
                          stdout=subprocess.PIPE).stdout


def main():
    root = Path(os.fsdecode(git("rev-parse", "--show-toplevel").rstrip(b"\n")))
    os.chdir(root)
    sources = ["package", "skills", "INSTALL.md", ".cargo", ".gitattributes",
               ".github/workflows", ".github/instructions"]
    if not git("diff", "--cached", "--name-only", "-z", "--", "README.md", *sources):
        return

    readme = root / "README.md"
    staged_tree = git("write-tree")
    tracked = git("ls-files", "--stage", "--", "README.md")
    previous = tracked.split()[1] if tracked else None
    target = Path(os.environ.get("CARGO_TARGET_DIR") or root / "package/target").resolve()

    # For ordinary commits the checkout is already the staged source. If it
    # contains local work, isolate the renderer and every input it can read.
    def has_local_inputs():
        return (git("diff-files", "--name-only", "-z", "--", *sources)
                or git("ls-files", "--others", "--exclude-standard", "-z", "--", *sources))

    result = None
    if not has_local_inputs():
        result = render(root, target)
    if result is None or has_local_inputs():
        with tempfile.TemporaryDirectory(prefix="gitmoji-readme-") as scratch:
            git("checkout-index", "--all", f"--prefix={scratch}{os.sep}")
            result = render(Path(scratch), target)

    oid = git("hash-object", "-w", "--stdin", input=result).strip()
    if git("write-tree") != staged_tree:
        sys.exit("The staged files changed while generating README.md. Retry the commit.")
    if oid == previous:
        return
    # A path-limited commit keeps a separate index.lock for the index Git will
    # restore afterwards. Adding a file to the hook's temporary index alone
    # would leave the old README staged as a reversal after the commit.
    if "GIT_INDEX_FILE" in os.environ:
        ordinary_env = os.environ.copy()
        ordinary_env.pop("GIT_INDEX_FILE")
        primary = Path(os.fsdecode(git("rev-parse", "--path-format=absolute", "--git-path",
                                       "index", env=ordinary_env).rstrip(b"\n")))
        lock = primary.with_name(primary.name + ".lock")
        effective = Path(os.environ["GIT_INDEX_FILE"]).resolve()
        if lock.exists() and effective not in (primary, lock):
            sys.exit("Git needs to stage a generated README alongside your sources. "
                     "Stage the intended changes with git add, then run git commit without paths.")

    # A partially staged README or a local deletion belongs to the contributor.
    # Check just before writing; preserve their working copy while updating Git.
    clean_copy = (readme.is_file() and not readme.is_symlink() and previous is not None
                  and git("hash-object", "--path=README.md", "README.md").strip() == previous)
    new_copy = previous is None and not os.path.lexists(readme)
    git("update-index", "--add", "--cacheinfo", f"100644,{oid.decode('ascii')},README.md")
    if clean_copy or new_copy:
        git("checkout-index", "--force", "--", "README.md")
    print("Staged the generated README.md.")


if __name__ == "__main__":
    main()
