# Gitmoji Triangle

Gitmoji Triangle helps people and coding agents decide when to commit. Each
commit records one decision. The emojis describe its area and intent, the
issue connects it to the work, and the scope names the part of the project it
changes.

```
🧩👔 #412 (server): allow an appeal within 30 days of refusal
🧩🛡️ #412 (server): reject an appeal without a refusal reference
🎨✨ #412 (app): let applicants appeal a refused permit
```

The CLI checks commit messages and explains the vocabulary. The accompanying
skill helps agents plan and write commits using the same rules.

## Install

```sh
git clone https://github.com/D1no/gitmoji.git
./gitmoji/install.sh /path/to/your/project
```

The installer downloads the binary for your platform and verifies its checksum.
With a project path, it also installs the skill, settings and Git hooks. Run
`./gitmoji/install.sh` by itself to install only the command. The CLI, skill
and vocabulary use one release number. Run `gitmoji --version` to check it.

### Pin the CLI in CI

For a project already using Gitmoji, install the version recorded in
`gitmoji.toml`. This example uses Python 3.11 or later to read TOML and
requires Git, curl and a SHA-256 utility. Run it from the project checkout.
Public release downloads work without GitHub credentials.

```sh
version=$(python3 -c 'from pathlib import Path; import tomllib; print(tomllib.loads(Path("gitmoji.toml").read_text())["vocabulary"]["version"])')
checkout=$(mktemp -d)
git -c advice.detachedHead=false clone --quiet --depth 1 --branch "v$version" \
  https://github.com/D1no/gitmoji.git "$checkout"
mkdir -p "$checkout/bin"
GITMOJI_BIN="$checkout/bin" "$checkout/install.sh"
"$checkout/bin/gitmoji" --version
```

Use the installed binary by path or add its directory to the runner’s `PATH`.
On GitHub Actions, `echo "$checkout/bin" >> "$GITHUB_PATH"` makes the command
available to later steps in the same job. The installer can build the command
with Cargo when a prebuilt binary is unavailable.

## Pull request titles

An open pull request can show the intentions of its commits before you read the diff:

```text
[`🎨👷` / `✨🔧`] Preview on pull requests
```

Push another commit and Gitmoji refreshes the emoji prefix. Reviewers can edit the words. When the PR lands with GitHub’s merge button, the number appears once at the end, for example `(#42)`. Enable this with the [PR title workflow and merge settings](skills/gitmoji/references/pull-requests.md#enable-automatic-pr-titles).

## Guides

| Guide | What it covers |
| --- | --- |
| [Commit convention](skills/gitmoji/SKILL.md) | Plan, write and check a commit |
| [Pull requests](skills/gitmoji/references/pull-requests.md) | Summarize a PR while preserving the individual decisions it contains |
| [Existing history](skills/gitmoji/references/upgrade-history.md) | Preview changes to historical merge and revert captions |
| [Command](package/README.md) | Build and test the Rust CLI |
| [Skill](skills/README.md) | Install the agent skill |
| [Development](docs/development.md) | Contributor setup and worktrees |
| [Website](website/README.md) | Run the landing page |
| [CI and releases](.github/README.md) | Run checks and publish a release |

Contributors can find repository-specific guidance in [AGENTS.md](AGENTS.md).
See the [changelog](CHANGELOG.md) for release notes and the [MIT license](LICENSE)
for licensing terms.
