# Changelog

The CLI, skill, and bundled vocabulary share one release number.

## 0.24.0 (24 November 2026)

- Agent posts now put the robot chip beside the opening words of both short replies and longer issues, PRs and reviews. When a new draft starts with a heading, list or code block, add a short opening paragraph before signing. Gitmoji keeps the headings and their section links intact. Existing v1 posts with either chip layout still verify. [Examples and template guidance](https://github.com/D1no/gitmoji/blob/v0.24.0/skills/gitmoji/references/agent-posts.md)
- To adopt, set `[vocabulary].version` to `"0.24.0"`, run the pinned installer, then run `gitmoji init` and `gitmoji doctor`.

## 0.23.0 (24 November 2026)

- Use `gitmoji comment sign` to mark an agent-written issue or pull request description, thread comment, review or inline reply. Short replies show the robot chip beside the prose. Full session lineage is opt-in with `--full-lineage`. A repository that copies PR descriptions into merge messages can also retain that metadata in Git history. [Signing and privacy](https://github.com/D1no/gitmoji/blob/v0.23.0/skills/gitmoji/references/agent-posts.md)
- Check the fingerprint offline with `gitmoji comment check`. For a GitHub or GitHub Enterprise permalink, `gitmoji comment trace URL` reports the checksum and visible edit history. A provisional workflow update keeps the original fingerprint so the edit remains observable.
- Claude Code and Codex can check supported GitHub CLI and MCP posts before publication with `gitmoji comment hook`. Duplicate harness credits produce an advisory. [Hook installation and coverage](https://github.com/D1no/gitmoji/blob/v0.23.0/skills/gitmoji/references/post-hooks.md)
- The repository README now generates its vocabulary, command overview and worked examples from project sources. Its three-column vocabulary shows how an area pairs with a behavior or intent.
- To adopt, set `[vocabulary].version` to `"0.23.0"`, run the standalone installer with `--pinned`, then run `gitmoji init` and `gitmoji doctor`. To enable pre-post checks, merge the hook entry from the installation guide into your harness settings.

## 0.22.0 (23 November 2026)

- Mark an authored review response with a heading such as ``👌 <`🧩🐛`> #42 (checkout): reserve the last item once``. The issue, scope, reason and signature follow the project’s usual rules. The hook understands `:ok_hand:` while typing. The stored prefix is 👌.
- For written feedback, add one `Review:` permalink to the reviewer’s existing request and one optional `Addresses: [URL, URL]` list of the exact findings. The signature command accepts `--review URL` and repeated `--addresses URL` flags. Verbal feedback can use just the ordinary reason and signature. After pushing, verify the fixing commit is reachable from the remote PR branch, then reply to the reviewer with its SHA.
- Search authored responses with `gitmoji log '👌'`. PR and local merge captions, release notes, provisional checks and history cleanup recognize the enclosed decision. [Authoring guide and examples](https://github.com/D1no/gitmoji/blob/v0.22.0/skills/gitmoji/SKILL.md#respond-to-review-feedback)
- To adopt, set `[vocabulary].version` to `"0.22.0"`, run the pinned installer, then run `gitmoji init` and `gitmoji doctor`. A project on 0.20.2 can upgrade directly. To enroll in the optional provisional-PR workflow at the same time, use `gitmoji init --github-workflows --provisional-workflow`. Land the updated workflow pins before using the new subject form on incoming PRs.

## 0.21.0 (23 November 2026)

- Pull request titles and landing captions display 🚧 for provisional work and 🧪 for exploration before the emoji inventory. The corresponding commits determine the type. `merge-caption --json` includes their modifiers and tickets.
- The opt-in provisional workflow verifies one open, labelled follow-up issue, maintains a link in the PR body, and can add a backlink. `provisional-pr --check --json` gives policy jobs a read-only check of the live PR. `defer-findings` lets maintainers transfer selected review threads to the issue before resolving them.
- `gitmoji open [TIP|FROM..TO]` lists unfinished provisional or exploratory work. Release notes disclose open tickets at the release tip. Set `provisional = "refuse"` in the `[changelog]` table to require closure before release.
- To upgrade, set `[vocabulary].version` to `"0.21.0"`, run the standalone installer with `--pinned`, and run `gitmoji init`. For follow-up checks and reviewer instructions also pass `--provisional-workflow`. Run `gitmoji doctor` and review the generated files. [Provisional PR guide](https://github.com/D1no/gitmoji/blob/v0.21.0/skills/gitmoji/references/pull-requests.md#land-a-demo-or-comparison-with-a-follow-up)

## 0.20.2 (22 November 2026)

- After GitHub refuses a direct merge with HTTP 405, `gitmoji github-merge` shows the host's explanation when available and reads the PR once. It compares the current head and base with the reviewed ones and reports the current mergeability state. If checks are settling or the PR now looks ready, review a fresh preview before submitting another merge. Gitmoji never repeats the write automatically. [Handling a refused merge](skills/gitmoji/references/pull-requests.md)
- To upgrade from 0.20.0 or 0.20.1, set `[vocabulary].version` to `"0.20.2"`, run the standalone installer with `--pinned`, then run `gitmoji init` and `gitmoji doctor`.

## 0.20.1 (22 November 2026)

- `gitmoji github-merge` now explains why a reviewed merge needs attention. HTTP 405 points to merge requirements, 409 calls for a fresh preview of the current PR head, and 422 points to merge validation. A dropped connection or an unconfirmed response gives the PR URL to inspect before trying again. The command sends the reviewed merge request once. [Direct PR merges](skills/gitmoji/references/pull-requests.md)
- To upgrade, set `[vocabulary].version` to `"0.20.1"`, run the standalone installer with `--pinned`, then run `gitmoji init` and `gitmoji doctor` in the project.

## 0.20.0 (22 November 2026)

- The pre-commit and commit-msg hooks now read the settings Git will commit. An unstaged policy edit cannot change the outcome of either hook. When adopting Gitmoji for the first time, stage the new settings to use them in that commit. Standalone message checks and an explicit `--config` still read from disk. [Staged project settings](package/README.md#choosing-policy-for-a-commit)
- Find authored changes with `gitmoji log '⬆' trunk..HEAD`. The filter searches any slot in a heading and shows canonical emoji, including on older Dependabot upgrade commits that store `:arrow_up:`. The original signed objects keep their spelling. [Find commits by emoji](package/README.md#find-commits-by-emoji)
- `gitmoji doctor` checks explicit Dependabot prefixes, including development and multi-ecosystem group prefixes. When a project has both configuration files, Dependabot and Gitmoji read `.github/dependabot.yml` first. Omitting the main prefix lets Dependabot infer its style from history. A supported bot rewrite appears in the optional `warnings` array and keeps the row healthy. An explicit prefix such as `chore` that produces an invalid Gitmoji heading now returns a mismatch and exit 1. Projects without a Dependabot file keep the existing component list. [Dependabot setup and manual check](skills/gitmoji/references/dependabot.md#check-a-prefix-before-enabling-it)
- `gitmoji changelog` prints the abbreviated ID and subject of each unlisted single-parent commit on the first-parent release line. The existing count stays on stderr, followed by the individual commits. Markdown on stdout keeps its existing format. [Draft release notes](package/README.md#draft-release-notes)
- To upgrade, set `[vocabulary].version` to `"0.20.0"`, run the standalone installer with `--pinned`, then run `gitmoji init` and `gitmoji doctor` from the project. The filter and diagnostics require no additional runtime or configuration.

## 0.19.1 (22 November 2026)

- Use a bare Unicode upgrade arrow in Dependabot prefixes, for example `🧑‍💻⬆ — (deps)`, to keep the arrow visible in raw Git history. Gitmoji accepts the bare arrow, its emoji-presentation form and earlier bot shortcodes. The generated PR inventory uses `⬆️`. [Dependabot setup](skills/gitmoji/references/dependabot.md)
- Editing the presentation of the arrow in a Dependabot PR title now keeps one inventory and the full description. The original signed commit remains intact.
- To upgrade, change the project pin to `0.19.1`, run the standalone installer with `--pinned`, and run `gitmoji init` to refresh the installed guide and workflows. To change future raw bot commits, copy the prefix from the guide into the project's Dependabot configuration.

## 0.19.0 (22 November 2026)

- Draft release notes with `gitmoji changelog FROM..TO`. For a first release, pass its tag alone. Each landed pull request appears once with its tickets and breaking changes. Projects can set their own audience groups. The draft uses merge-preserving history and leaves the final wording to the release editor. [Changelog guide](package/README.md#draft-release-notes)
- Use a Gitmoji prefix for Dependabot version updates. The managed commit check and PR title workflow recognize the upgrade in single and grouped bot commits, including when GitHub stores it as a shortcode. [Dependabot setup](skills/gitmoji/references/dependabot.md)
- Download the standalone `install.sh` from the release and run `sh install.sh --pinned` in your project to install its adopted version without cloning Gitmoji. The installer checks the platform binary against the release checksums and preserves an identical installation. [Installation](README.md#install)
- To adopt this release, set `[vocabulary].version` to `"0.19.0"`, install the pin and run `gitmoji init` to refresh installed skill files and enrolled workflows. Run `gitmoji doctor` to verify your installation.

## 0.18.0 (22 November 2026)

- Run `gitmoji doctor` inside a checkout for a read-only, offline report on the running binary, project pin, effective Git hooks, installed skill and enrolled workflows. The report works from linked worktrees and supports plain Git and Lefthook. [Installation checks](package/README.md#check-an-installation)
- Use `gitmoji doctor --json` in setup checks. The command emits a JSON array and exits with status 1 when a component needs attention. Projects can display the remedy for each failing component without parsing CLI prose.
- Install 0.18.0 and update the project's version pin. Run `gitmoji init` to refresh the installed files, then `gitmoji doctor` to verify the installation.

## 0.17.1 (22 November 2026)

- Refreshing Gitmoji’s unmodified 0.15.x and 0.16.0 title workflows no longer leaves a redundant `.before-update` file. Customized managed workflows still get a backup. Update the project pin and run `gitmoji init` to refresh the installed files. If 0.17.0 already created a backup, compare it with the version in Git before removing it.

## 0.17.0 (22 November 2026)

- Read session signatures with `gitmoji signature`, keep PR titles in sync with `gitmoji pr-title`, and preview or submit direct merges with `gitmoji github-merge`. These operations run in the released executable. Local Git operations require Git. Hosted operations also require credentials for the selected GitHub host. [Pull request guide](skills/gitmoji/references/pull-requests.md)
- Use `gitmoji history --upgrade` for historical merge and revert captions. The offline mode runs using only Git and Gitmoji. The preview, candidate checks and explicit apply options are preserved. [History upgrade guide](skills/gitmoji/references/upgrade-history.md)
- `gitmoji pin` reads the adopted version from the project settings. The pinned installer bootstraps a checksum-verified reader on fresh machines. The release installer and shipped commands run without Python or uv.
- To upgrade, install 0.17.0, update `[vocabulary].version`, and run `gitmoji init`. Commit the refreshed skill and managed workflows. Replace direct calls to the old Python helpers with the commands above. Init removes recognized released copies and preserves local customizations.
- Setup suggestions attribute imported skill and workflow files to the release with a `Source:` line and show separate staging commands for local configuration changes.
- If Gitmoji’s retired helpers were the repository’s only Python files, remove Python from the enabled languages in CodeQL and GitHub Code Quality. Keep it enabled wherever the repository still contains Python. [CodeQL language settings](https://docs.github.com/en/code-security/how-tos/find-and-fix-code-vulnerabilities/configure-code-scanning/configure-code-scanning)

## 0.16.0 (22 November 2026)

- Local branch merges show the source, destination and intentions of incoming commits, including in projects with no remote or pull requests. Their bodies list what arrived and any conflicted paths Git recorded. Local merges and PRs that introduce a merge use up to fourteen visible symbols. Ordinary PRs keep nine. [Examples and signatures](skills/gitmoji/references/pull-requests.md#local-branch-merges)
- Install 0.16.0, update the project's version pin and run `gitmoji init` to refresh the skill, managed workflows and hooks. Projects with their own Lefthook configuration should add the documented `prepare-commit-msg` command and run `lefthook install` in each clone.
- The historical upgrade can recover default branch merges with recorded source and destination names. Use `--local-merges-only` for a repository without GitHub. Its preview distinguishes recorded branch merges from PR landings and preserves the links to incoming commits in the candidate. [History upgrade](skills/gitmoji/references/upgrade-history.md)

## 0.15.1 (21 November 2026)

- Managed workflows and their jobs now use `gitmoji-commits` and `gitmoji-pr-title`. Update the project pin, install this release and run `gitmoji init` to migrate enrolled 0.15.0 workflows. Project-owned files keep their names. When the new checks appear on GitHub, select the Gitmoji check you need and retain any existing requirement for your own `check` job. [Setup and required checks](skills/gitmoji/references/pull-requests.md#enable-automatic-pr-titles)
- The historical title preview reports `to_change`, `already_correct` and `unmatched`. Rewritten merges can also match when both ordered parent changes and corresponding author records agree despite unrelated changes to the surrounding snapshots. Review entries identify the evidence that differs. [History guide](skills/gitmoji/references/upgrade-history.md#update-titles-of-already-merged-prs)

## 0.15.0 (21 November 2026)

- Require signatures from recognized coding-agent sessions with `[rules] signature = "agents"`. Fresh installations enable this rule. Existing projects can opt in without changing the policy for human commits or old history.
- Run `gitmoji init --github-workflows` to install the PR commit check and title sync together. On later upgrades, update the project version and run `gitmoji init` to refresh managed workflows. Other CI systems can run `install.sh --pinned` from a project checkout with root or `.config` settings. [Setup](skills/gitmoji/references/pull-requests.md#enable-automatic-pr-titles)
- Recover historical merge captions after a previous rewrite when the PR and incoming commits corroborate them. Preview and apply merged PR title updates independently of local history. Repeated references to the integrating PR collapse to one. [History and hosted titles](skills/gitmoji/references/upgrade-history.md)

## 0.14.0 (21 November 2026)

- Show a pull request’s current commit intentions at the start of its title. The public workflow updates the prefix on pushes and title or base edits, including for fork PRs.
- Select **Pull request title and description** as the merge default to land the same title with its PR number at the end. Projects with their own title updater can replace it with the [v0.14.0 workflow and setup guide](skills/gitmoji/references/pull-requests.md#enable-automatic-pr-titles).
- `gitmoji merge-caption` now prints the open PR title. For scripts that need the landed commit subject, read `subject` from `--json`. The JSON also includes `title`, `recognized` and `total`. Direct merges and verified historical upgrades use the same format.

## 0.13.3 (21 November 2026)

- The checker and commit hook now flag a description line that starts with an issue number, such as `#42`. Git can strip that line during an amend or reword. Start the line with a word, for example `See #42 for the decision.`
- The rule applies to new drafts across cleanup modes. Stored history keeps its existing validation behavior. Projects can adopt this patch through their usual version pin.

## 0.13.2 (21 November 2026)

- Install Gitmoji in CI using the public HTTPS clone and checksum-verified release assets.
  A GitHub token is optional. See the [pinned CI install](README.md#pin-the-cli-in-ci).
- The public distribution also includes compatibility builds for projects pinning
  0.13.0 or 0.13.1. Use the corresponding public checksums if your build pins binary digests.

## 0.13.1 (20 November 2026)

- Keep the history helper focused on PR merge and undo captions.
- Retry private release downloads quietly when a GitHub CLI login is available.

## 0.13.0 (20 November 2026)

- Check commit intentions with the CLI and install the agent skill and hooks into a project.
- Preview pull request captions and review a local candidate before updating existing history.
