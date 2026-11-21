#!/bin/sh
# Runs once when the dev container is created. Safe to run again.
set -eu

# The checkout is mounted from the host and may belong to another user id there, which
# git refuses to work in unless told the folder is safe.
git config --global --add safe.directory "$(pwd)"

# The volume for build output is created owned by root.
sudo chown "$(id -u):$(id -g)" package/target

# lefthook runs the git hooks. One static binary, pinned.
lefthook_version=2.2.2
case "$(uname -m)" in
  x86_64) arch=x86_64 ;;
  aarch64 | arm64) arch=arm64 ;;
  *) echo "no lefthook build for $(uname -m)" >&2; exit 1 ;;
esac
if ! command -v lefthook >/dev/null 2>&1; then
  sudo curl -fsSL -o /usr/local/bin/lefthook \
    "https://github.com/evilmartians/lefthook/releases/download/v${lefthook_version}/lefthook_${lefthook_version}_Linux_${arch}"
  sudo chmod +x /usr/local/bin/lefthook
fi

# The hooks call `gitmoji`. Here it is the command built from this checkout, so the
# repository checks its commits with its own current code.
(cd package && cargo install --path . --locked --quiet)

# Where the checkout belongs to another user id on the host, as under rootless Docker,
# the hooks cannot be rewritten from in here. Hooks installed on the host still run.
lefthook install || echo "The git hooks could not be written from inside the container; run 'lefthook install' on the host once."
gitmoji --version
