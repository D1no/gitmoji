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

On GitHub Actions, run `gitmoji init --github-workflows` in the project to install the PR commit check and title sync. Commit the generated workflows. On upgrade, change `[vocabulary].version` and rerun `gitmoji init` to refresh managed copies. [Workflow setup](skills/gitmoji/references/pull-requests.md#enable-automatic-pr-titles)

For another CI system, run the following from the project checkout. The installer reads the release number from `gitmoji.toml` or `.config/gitmoji.toml` using Python 3.11 or later. It fetches the release tag and runs its checksum-verifying installer. Public downloads work without a GitHub token.

```sh
tool=$(mktemp -d)
git clone --quiet --depth 1 https://github.com/D1no/gitmoji.git "$tool/source"
GITMOJI_BIN="$tool/bin" "$tool/source/install.sh" --pinned
"$tool/bin/gitmoji" --version
```

When contributor branches merge, the local Git hook names the incoming branch, its destination and the intentions in the arriving commits. [Local branch merges](skills/gitmoji/references/pull-requests.md#local-branch-merges)

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
