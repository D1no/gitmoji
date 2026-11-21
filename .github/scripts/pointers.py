#!/usr/bin/env python3
"""Check that the pointers between the repository's guides lead somewhere.

The guides are kept short by pointing at each other, which only works while every
pointer resolves. This checks, in every tracked Markdown file:

- relative links, `[text](path)`: the file must exist;
- in AGENTS.md, a path in the map: it must exist, unless its row names the issue that
  will bring it (`#12`).
"""

import re
import subprocess
import sys
from pathlib import Path

LINK = re.compile(r"\]\(([^)#\s]+)(?:#[^)]*)?\)")
PATH_IN_ROW = re.compile(r"`([\w./-]+\.md)`")


def main():
    root = Path(subprocess.check_output(["git", "rev-parse", "--show-toplevel"], text=True).strip())
    files = subprocess.check_output(["git", "ls-files", "*.md"], cwd=root, text=True).split()
    broken = []
    for name in files:
        text = (root / name).read_text(encoding="utf-8")
        fenced = False
        for number, line in enumerate(text.splitlines(), 1):
            if line.lstrip().startswith("```"):
                fenced = not fenced
            if fenced:
                continue
            for target in LINK.findall(line):
                if re.match(r"[a-z]+:", target):
                    continue  # a web address
                if not (root / name).parent.joinpath(target).exists():
                    broken.append(f"{name}:{number}: link to {target}, which does not exist")
            if name == "AGENTS.md" and line.startswith("|"):
                for target in PATH_IN_ROW.findall(line):
                    if not (root / target).exists() and not re.search(r"#\d+", line):
                        broken.append(f"{name}:{number}: {target} does not exist and no issue is named")
    for line in broken:
        print(line)
    print(f"{len(files)} files checked, {len(broken)} broken pointers")
    return 1 if broken else 0


if __name__ == "__main__":
    sys.exit(main())
