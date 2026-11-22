#!/bin/sh
# Install the Gitmoji Triangle CLI. A release download is a standalone installer.
set -eu

# Filled in when this script is packaged as a release asset.
installer_release=''
usage() {
  cat <<'USAGE'
Usage: install.sh [--overwrite] [path to the repository] [github|backlog]
       install.sh --pinned [--overwrite]
Run --pinned from a project to install the CLI release in its Gitmoji settings.
With a repository path, install the CLI and run gitmoji init. With no arguments,
install the release of this script. Set GITMOJI_BIN to choose the destination.
USAGE
}

pinned=0
overwrite=${GITMOJI_OVERWRITE:-}
target=
tickets=
for word in "$@"; do
  case "$word" in
    --pinned) pinned=1 ;;
    --overwrite) overwrite=1 ;;
    -h | --help) usage; exit 0 ;;
    -*) usage >&2; exit 2 ;;
    *)
      if [ -z "$target" ]; then target=$word
      elif [ -z "$tickets" ]; then tickets=$word
      else usage >&2; exit 2
      fi ;;
  esac
done
tickets=${tickets:-github}
case "$tickets" in
  github | backlog) ;;
  *) echo "the ticket format is github or backlog, not: $tickets" >&2; exit 2 ;;
esac
if [ -n "$target" ]; then
  if [ "$pinned" = 1 ]; then usage >&2; exit 2; fi
  if ! git -C "$target" rev-parse --git-dir >/dev/null 2>&1; then
    echo "$target is not inside a git repository" >&2
    exit 2
  fi
fi

checkout=
if [ -z "$installer_release" ]; then
  # The checkout can also build from source if no prebuilt binary is available.
  checkout=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
  installer_release=$(awk -F'"' '/^version = "/ {print $2; found=1; exit} END {if (!found) exit 1}' "$checkout/package/Cargo.toml")
fi

exe=gitmoji
case "$(uname -s)" in
  Darwin) os=apple-darwin ;;
  Linux) os=unknown-linux-musl ;;
  MINGW* | MSYS* | CYGWIN*) os=pc-windows-msvc; exe=gitmoji.exe ;;
  *) os=unknown ;;
esac
case "$(uname -m)" in
  arm64 | aarch64) arch=aarch64 ;;
  x86_64 | amd64) arch=x86_64 ;;
  *) arch=unknown ;;
esac
asset="gitmoji-$arch-$os"
[ "$exe" = gitmoji.exe ] && asset="$asset.exe"

work=$(mktemp -d "${TMPDIR:-/tmp}/gitmoji-install.XXXXXX")
staged=
cleanup() {
  rm -rf "$work"
  if [ -n "$staged" ]; then rm -f "$staged"; fi
}
trap cleanup EXIT
trap 'exit 1' INT HUP TERM

download() {
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL -o "$2" "$1"
  elif command -v wget >/dev/null 2>&1; then
    wget -q -O "$2" "$1"
  else
    echo "Install curl or wget to download Gitmoji." >&2
    return 1
  fi
}

# A private GitHub repository may require a signed-in gh client. Mirrors always
# supply their own files, so a missing file there cannot select a different source.
release_file() {
  if download "$base/$1" "$dest/$1"; then return 0; fi
  if [ -z "${GITMOJI_RELEASES:-}" ] && command -v gh >/dev/null 2>&1 &&
      gh release download "v$version" --repo "$repository" --pattern "$1" \
        --dir "$dest" --clobber >/dev/null 2>&1; then return 0; fi
  echo "Could not download $1 from $base." >&2
  return 1
}

checksum() {
  if command -v sha256sum >/dev/null 2>&1; then
    sum=$(sha256sum < "$1") || return 1
  elif command -v shasum >/dev/null 2>&1; then
    sum=$(shasum -a 256 < "$1") || return 1
  else
    echo "Install sha256sum or shasum to verify the release." >&2
    return 1
  fi
  printf '%s\n' "${sum%% *}"
}

# Set binary to a verified executable, reusing the reader when it is also the
# target release. A repeat install only downloads the small checksum manifest.
verified_release=
fetch_binary() {
  version=$1
  if [ "$version" = "$verified_release" ]; then return 0; fi
  if [ "$arch" = unknown ] || [ "$os" = unknown ]; then
    echo "No Gitmoji release binary for $(uname -s) $(uname -m)." >&2
    return 1
  fi
  dest="$work/$version"
  repository=${GITMOJI_REPOSITORY:-D1no/gitmoji}
  base=${GITMOJI_RELEASES:-https://github.com/$repository/releases/download/v$version}
  mkdir -p "$dest" || return 1
  release_file SHA256SUMS || return 1
  expected=$(awk -v asset="$asset" '
    $2 == asset && NF == 2 { sum = $1; n++ }
    END { if (n == 1) print sum }
  ' "$dest/SHA256SUMS") || return 1
  if [ "${#expected}" != 64 ]; then
    echo "SHA256SUMS must contain one checksum for $asset." >&2
    return 1
  fi

  if [ -f "$candidate_binary" ] && [ -x "$candidate_binary" ] &&
      [ "$(checksum "$candidate_binary")" = "$expected" ]; then
    binary=$candidate_binary
  else
    release_file "$asset" || return 1
    binary="$dest/$asset"
    actual=$(checksum "$binary") || return 1
    if [ "$expected" != "$actual" ]; then
      echo "The download of $asset did not match the release's checksum." >&2
      return 1
    fi
    chmod +x "$binary" || return 1
  fi
  if [ "$("$binary" --version)" != "gitmoji $version (gitmoji-triangle)" ]; then
    echo "The verified file did not identify itself as Gitmoji Triangle $version." >&2
    return 1
  fi
  verified_release=$version
}

ours() {
  case "$("$1" --version 2>/dev/null </dev/null || true)" in
    *'(gitmoji-triangle)') return 0 ;;
  esac
  return 1
}

current=$(command -v gitmoji 2>/dev/null || true)
candidate_binary=$current
if [ -n "${GITMOJI_BIN:-}" ]; then candidate_binary="$GITMOJI_BIN/$exe"; fi
release=$installer_release
if [ "$pinned" = 1 ]; then
  reader=
  for available in "$candidate_binary" "$current"; do
    if [ -f "$available" ] && [ -x "$available" ] && ours "$available" &&
        "$available" pin --help >/dev/null 2>&1 </dev/null; then
      reader=$available
      break
    fi
  done
  if [ -z "$reader" ]; then
    fetch_binary "$installer_release" || exit 1
    reader=$binary
  fi
  # The native reader owns TOML parsing and settings precedence. With Git
  # absent it reads the current folder, so locate the enclosing worktree first.
  if command -v git >/dev/null 2>&1; then
    release=$("$reader" pin </dev/null)
  else
    here=$(pwd -P)
    root=$here
    while [ ! -e "$root/.git" ] && [ "$root" != / ]; do
      root=${root%/*}
      root=${root:-/}
    done
    if [ ! -e "$root/.git" ]; then root=$here; fi
    case "$reader" in /*) ;; *) reader="$here/$reader" ;; esac
    release=$(cd "$root" && "$reader" pin </dev/null)
  fi
  # Only a plain release number may become part of a URL or filesystem path.
  case "$release" in
    '' | *[!0-9.]* | .* | *. | *..* | *.*.*.*) valid=0 ;;
    *.*.*) valid=1 ;;
    *) valid=0 ;;
  esac
  if [ "$valid" != 1 ]; then
    echo "gitmoji pin answered with something other than a release number: $release" >&2
    exit 1
  fi
fi
if ! fetch_binary "$release"; then
  if [ -z "$checkout" ] || [ "$pinned" = 1 ] || ! command -v cargo >/dev/null 2>&1; then
    exit 1
  fi
  echo "Building gitmoji $release from this checkout with cargo."
  (cd "$checkout/package" && cargo build --release --locked --quiet)
  binary="$checkout/package/target/release/$exe"
fi

on_path() {
  case ":$PATH:" in *":$1:"*) return 0 ;; esac
  return 1
}

allow_replacement() {
  other=$1
  echo "Another tool named gitmoji is installed at $other." >&2
  echo "Replacing it affects scripts and hooks that use it." >&2
  if [ -z "$overwrite" ] && [ -t 0 ]; then
    printf 'Replace it with Gitmoji Triangle? [y/N] ' >&2
    read -r answer || answer=
    case "$answer" in y | Y | yes | Yes | YES) overwrite=1 ;; esac
  fi
  if [ "$overwrite" != 1 ]; then
    echo "Nothing was changed. To replace it, use --overwrite or GITMOJI_OVERWRITE=1." >&2
    exit 3
  fi
}

folder=${GITMOJI_BIN:-}
if [ -n "$current" ] && [ -f "$current" ]; then
  if ! ours "$current"; then allow_replacement "$current"; fi
  if [ -z "$folder" ] && [ -w "$(dirname "$current")" ]; then folder=$(dirname "$current"); fi
fi
if [ -z "$folder" ]; then
  for candidate in "$HOME/.local/bin" "$HOME/bin" "$HOME/.cargo/bin" /opt/homebrew/bin /usr/local/bin; do
    if on_path "$candidate" && [ -d "$candidate" ] && [ -w "$candidate" ]; then
      folder=$candidate
      break
    fi
  done
fi
folder=${folder:-"$HOME/.local/bin"}
if [ -f "$folder/$exe" ] && [ "$folder/$exe" != "$current" ] && ! ours "$folder/$exe"; then
  allow_replacement "$folder/$exe"
fi
if [ -d "$folder/$exe" ]; then echo "The destination $folder/$exe is a directory." >&2; exit 1; fi
if [ -x "$folder/$exe" ] && cmp -s "$binary" "$folder/$exe"; then
  echo "Already installed: $folder/$exe (gitmoji $release)."
else
  mkdir -p "$folder"
  staged=$(mktemp "$folder/.gitmoji-install.XXXXXX")
  cp "$binary" "$staged"
  chmod 755 "$staged"
  mv -f "$staged" "$folder/$exe"
  staged=
  echo "Installed $("$folder/$exe" --version) as $folder/$exe"
fi

if [ -n "$target" ]; then
  cleanup
  exec "$folder/$exe" init "$target" --tickets "$tickets"
fi
if ! on_path "$folder"; then
  echo "$folder is not on your PATH. Prepend it so git hooks can find gitmoji."
elif [ -n "$current" ] && [ "$current" != "$folder/$exe" ] &&
     [ "$(command -v gitmoji)" != "$folder/$exe" ]; then
  echo "Prepend $folder to PATH so git hooks find it before $current."
fi
