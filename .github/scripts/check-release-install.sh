#!/bin/sh
# Run from package/ after naming this platform's binary in ../dist.
set -eu
target=$1
version=$2
project=$(cd .. && pwd)
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
mkdir -p "$scratch/tools" "$scratch/project/.config"
for tool in sh awk cat chmod cmp cp curl dirname mkdir mktemp mv rm uname; do
  ln -s "$(command -v "$tool")" "$scratch/tools/$tool"
done
asset="gitmoji-$target"
if command -v sha256sum >/dev/null 2>&1; then
  ln -s "$(command -v sha256sum)" "$scratch/tools/sha256sum"
  (cd "$project/dist" && sha256sum "$asset") > "$scratch/SHA256SUMS"
else
  ln -s "$(command -v shasum)" "$scratch/tools/shasum"
  (cd "$project/dist" && shasum -a 256 "$asset") > "$scratch/SHA256SUMS"
fi
cp "$project/dist/$asset" "$scratch/$asset"
sed "s/^installer_release=''$/installer_release='$version'/" "$project/install.sh" > "$scratch/install.sh"
printf '[vocabulary]\nversion = "%s"\n' "$version" > "$scratch/project/.config/gitmoji.toml"
cd "$scratch/project"
(
  export PATH="$scratch/tools" GITMOJI_BIN="$scratch/bin" GITMOJI_RELEASES="file://$scratch"
  for absent in git python3 cargo; do
    if command -v "$absent"; then exit 1; fi
  done
  sh -s -- --pinned < "$scratch/install.sh"
)
cmp "$scratch/bin/gitmoji" "$project/dist/$asset"
"$scratch/bin/gitmoji" --version
