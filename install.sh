#!/bin/sh
# Install the `gitmoji` command and set a git repository up for Gitmoji Triangle.
#
#   ./install.sh [--overwrite] [path to the repository] [github|backlog]
#   ./install.sh --pinned  (from a project checkout; installs only its adopted CLI)
#
# Needs git and curl: the command is one prebuilt file, downloaded from the GitHub release
# that matches this clone and checked against the release's checksums.
# Without a path it only installs the command.
# If another tool called gitmoji is already installed (gitmoji-cli, for example), this
# asks before replacing it. --overwrite, or GITMOJI_OVERWRITE=1, answers yes in advance;
# GITMOJI_OVERWRITE set to anything else answers no.
# Set GITMOJI_BIN to choose the folder the command is copied to.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
if [ "${1:-}" = --pinned ]; then
  if [ "$#" != 1 ]; then
    echo "Usage: install.sh --pinned (run from the project checkout)" >&2
    exit 2
  fi
  top=$(git rev-parse --show-toplevel)
  # Match the CLI's settings precedence. Parse TOML before interpolating a tag.
  version=$(python3 -I - "$top" <<'PYTHON'
from pathlib import Path
import re
import sys
try:
    import tomllib
except ImportError:
    sys.exit("install.sh --pinned requires Python 3.11 or later")
root = Path(sys.argv[1])
settings = next((root / name for name in
                 ("gitmoji.toml", ".config/gitmoji.toml", "triangle.toml")
                 if (root / name).is_file()), None)
if settings is None:
    sys.exit("No Gitmoji settings found in the project")
try:
    with settings.open("rb") as source:
        version = tomllib.load(source)["vocabulary"]["version"]
except (OSError, ValueError, KeyError, TypeError) as error:
    sys.exit(f"Cannot read [vocabulary].version from {settings}: {error}")
if not isinstance(version, str) or not re.fullmatch(r"(?:0|[1-9][0-9]*)(?:\.(?:0|[1-9][0-9]*)){2}", version):
    sys.exit(f"Expected a release number such as 1.2.3 in {settings}")
print(version)
PYTHON
)
  pinned=$(mktemp -d "${TMPDIR:-/tmp}/gitmoji-pinned.XXXXXX")
  trap 'rm -rf "$pinned"' EXIT
  trap 'exit 1' INT TERM
  git -C "$pinned" init -q
  git -C "$pinned" fetch -q --depth=1 --no-tags https://github.com/D1no/gitmoji.git "refs/tags/v$version"
  git -C "$pinned" -c advice.detachedHead=false checkout -q --detach 'FETCH_HEAD^{commit}'
  "$pinned/install.sh"
  exit
fi

# Only 1 means yes. Any other value means no, so that a setting such as 0 or false can
# never replace a working command.
overwrite=${GITMOJI_OVERWRITE:-}
target=
tickets=
for word in "$@"; do
  case "$word" in
    --overwrite) overwrite=1 ;;
    -h | --help)
      sed -n '1d; /^#/!q; s/^# \{0,1\}//p' "$0"
      exit 0
      ;;
    *)
      if [ -z "$target" ]; then
        target=$word
      elif [ -z "$tickets" ]; then
        tickets=$word
      fi
      ;;
  esac
done
tickets=${tickets:-github}

case "$tickets" in
  github | backlog) ;;
  *)
    echo "the ticket format is github or backlog, not: $tickets" >&2
    exit 2
    ;;
esac
if [ -n "$target" ] && ! git -C "$target" rev-parse --git-dir >/dev/null 2>&1; then
  echo "usage: ./install.sh [--overwrite] [path to a git repository] [github|backlog]" >&2
  echo "$target is not inside a git repository" >&2
  exit 2
fi

# 1. Which prebuilt command fits this machine.
exe=gitmoji
case "$(uname -s)" in
  Darwin) os=apple-darwin ;;
  Linux) os=unknown-linux-musl ;;
  MINGW* | MSYS* | CYGWIN*)
    os=pc-windows-msvc
    exe=gitmoji.exe
    ;;
  *) os=unknown ;;
esac
case "$(uname -m)" in
  arm64 | aarch64) arch=aarch64 ;;
  x86_64 | amd64) arch=x86_64 ;;
  *) arch=unknown ;;
esac
# The release that matches this clone: the version in its Cargo.toml.
version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$here/package/Cargo.toml" | head -n 1)
repository=${GITMOJI_REPOSITORY:-D1no/gitmoji}
asset="gitmoji-$arch-$os"
[ "$exe" = gitmoji.exe ] && asset="$asset.exe"
# A folder of this run's own, so that nothing from an earlier or a parallel run is used.
download=$(mktemp -d "${TMPDIR:-/tmp}/gitmoji-install.XXXXXX")
trap 'rm -rf "$download"' EXIT INT TERM
binary=
if [ "$arch" != unknown ] && [ "$os" != unknown ] && command -v curl >/dev/null 2>&1; then
  # GITMOJI_RELEASES points at another copy of the release files, such as a mirror.
  base=${GITMOJI_RELEASES:-https://github.com/$repository/releases/download/v$version}
  # A private repository normally declines this anonymous probe. Report failure
  # after the authenticated download has also been tried.
  if curl -fsL "$base/$asset" -o "$download/$asset" && curl -fsL "$base/SHA256SUMS" -o "$download/SHA256SUMS"; then
    expected=$(grep " $asset\$" "$download/SHA256SUMS" | cut -d' ' -f1)
    if command -v sha256sum >/dev/null 2>&1; then
      actual=$(sha256sum "$download/$asset" | cut -d' ' -f1)
    else
      actual=$(shasum -a 256 "$download/$asset" | cut -d' ' -f1)
    fi
    if [ -n "$expected" ] && [ "$expected" = "$actual" ]; then
      binary="$download/$asset"
      chmod +x "$binary"
    else
      echo "The download of $asset did not match the release's checksum; it was not used." >&2
    fi
  elif command -v gh >/dev/null 2>&1 && gh release download "v$version" --repo "$repository" \
    --pattern "$asset" --pattern SHA256SUMS --dir "$download" --clobber >/dev/null 2>&1; then
    # A repository that is not public gives its release files only to a signed-in gh.
    if (cd "$download" && grep " $asset\$" SHA256SUMS | { sha256sum -c - 2>/dev/null || shasum -a 256 -c - ; } >/dev/null 2>&1); then
      binary="$download/$asset"
      chmod +x "$binary"
    else
      echo "The download of $asset did not match the release's checksum; it was not used." >&2
    fi
  else
    echo "Could not download gitmoji $version for this machine from $base."
  fi
fi
if [ -z "$binary" ]; then
  echo "Could not obtain a verified gitmoji $version binary for $(uname -s) $(uname -m)."
  if command -v cargo >/dev/null 2>&1; then
    echo "Building it with cargo."
    (cd "$here/package" && cargo build --release --locked --quiet)
    binary="$here/package/target/release/$exe"
  else
    echo "Install Rust (https://rustup.rs) and run this again; it will build the command." >&2
    exit 1
  fi
fi

# 2. Is there a gitmoji already, and is it this one?
on_path() {
  case ":$PATH:" in
    *":$1:"*) return 0 ;;
  esac
  return 1
}
ours() {
  case "$("$1" --version 2>/dev/null </dev/null || true)" in
    *gitmoji-triangle*) return 0 ;;
  esac
  return 1
}
folder=${GITMOJI_BIN:-}
current=$(command -v gitmoji 2>/dev/null || true)
if [ -n "$current" ] && [ -f "$current" ]; then
  if ours "$current"; then
    # An earlier copy of this command: bring it up to date where it is.
    if [ -z "$folder" ] && [ -w "$current" ]; then
      folder=$(dirname "$current")
    fi
  else
    echo "There is already a gitmoji command on this machine, and it is another tool:"
    echo "  $current  ($("$current" --version 2>/dev/null </dev/null | head -n 1 || true))"
    echo "It is probably gitmoji-cli. Git hooks run the first gitmoji on PATH, so only one"
    echo "of the two can be used. Replacing it breaks scripts and hooks that call gitmoji-cli."
    if [ -z "$overwrite" ]; then
      if [ -t 0 ]; then
        printf 'Replace it with Gitmoji Triangle? [y/N] '
        read -r answer
        case "$answer" in
          y | Y | yes | Yes | YES) overwrite=1 ;;
        esac
      else
        echo "Nothing was changed. To replace it, run this again with --overwrite." >&2
        exit 3
      fi
    fi
    if [ "$overwrite" != 1 ]; then
      echo "Left as it is. Nothing was installed." >&2
      exit 3
    fi
    if [ -z "$folder" ]; then
      if [ -w "$(dirname "$current")" ]; then
        folder=$(dirname "$current")
      else
        echo "$(dirname "$current") cannot be written to, so the other gitmoji stays there."
        echo "Remove it yourself (npm uninstall -g gitmoji-cli, or brew uninstall gitmoji)."
      fi
    fi
    echo "To keep it replaced, also uninstall the other tool: npm uninstall -g gitmoji-cli, or brew uninstall gitmoji."
  fi
fi

# 3. Where to put it: the first folder on PATH you can write to.
if [ -z "$folder" ]; then
  for candidate in "$HOME/.local/bin" "$HOME/bin" "$HOME/.cargo/bin" /opt/homebrew/bin /usr/local/bin; do
    if on_path "$candidate" && [ -d "$candidate" ] && [ -w "$candidate" ]; then
      folder=$candidate
      break
    fi
  done
fi
if [ -z "$folder" ]; then
  folder="$HOME/.local/bin"
fi
mkdir -p "$folder"
# Copy beside the old file and move over it, so a running command is never half written.
cp "$binary" "$folder/$exe.new"
chmod +x "$folder/$exe.new"
mv -f "$folder/$exe.new" "$folder/$exe"
rm -rf "$download"
echo "Installed $("$folder/$exe" --version) as $folder/$exe"

# 4. Set the repository up. The command does this itself, and says so when it is not on PATH.
if [ -z "$target" ]; then
  if ! on_path "$folder"; then
    echo "$folder is not on your PATH. Add it, so that git hooks find gitmoji."
  fi
  echo "To set a repository up: gitmoji init /path/to/the/repository"
  exit 0
fi
echo
exec "$folder/$exe" init "$target" --tickets "$tickets"
