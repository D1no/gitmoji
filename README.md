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
installer=$(mktemp)
curl -fsSL https://github.com/D1no/gitmoji/releases/latest/download/install.sh -o "$installer" &&
  sh "$installer" /path/to/your/project
```

The installer downloads the binary for your platform and verifies its checksum. Signatures, PR titles, direct merges and history upgrades run in that binary. Repository operations use Git; hosted operations also use your GitHub credentials.
With a project path, it also installs the skill, settings and Git hooks. Run
`sh "$installer"` by itself to install only the command. The CLI, skill
and vocabulary use one release number. Run `gitmoji --version` to check it.

### Check a project installation

Run `gitmoji doctor` inside a checkout to check the active release, Git hooks, installed skill and enrolled workflows. For a session-start check, `gitmoji doctor --json` emits one array and exits with status 1 when a component needs attention. It works offline and leaves the checkout alone. [Diagnostic fields and examples](package/README.md#check-an-installation)

### Pin the CLI in CI

On GitHub Actions, run `gitmoji init --github-workflows` in the project to install the PR commit check and title sync. Commit the generated workflows. On upgrade, change `[vocabulary].version` and rerun `gitmoji init` to refresh managed copies. [Workflow setup](skills/gitmoji/references/pull-requests.md#enable-automatic-pr-titles)

For other CI systems, run the installer from the project checkout. It reads
`[vocabulary].version` from `gitmoji.toml` or `.config/gitmoji.toml` and installs
that release. The installer uses a local Gitmoji that supports `pin`, or downloads
a temporary reader. It verifies every downloaded binary against its own release’s
`SHA256SUMS`. Installing the CLI requires a POSIX shell, curl or wget, and
sha256sum or shasum. Git is needed only for subsequent repository operations.

```sh
tool=$(mktemp -d)
url=https://github.com/D1no/gitmoji/releases/latest/download/install.sh
curl -fsSL "$url" -o "$tool/install.sh" &&
  GITMOJI_BIN="$tool/bin" sh "$tool/install.sh" --pinned &&
  "$tool/bin/gitmoji" --version
```

With wget, use `wget -q -O "$tool/install.sh" "$url"` for the download. For a
fixed bootstrap, copy the installer URL from a specific release’s assets.
`GITMOJI_BIN` selects the destination directory. If another tool named
`gitmoji` is installed, the installer asks before replacing it. In CI the
collision stops installation unless you pass `--overwrite` or set
`GITMOJI_OVERWRITE=1`. Repeated installs verify the existing binary against the
release checksum and leave an identical executable in place.
Linux and macOS releases cover arm64 and x86_64.

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
