# Changelog

The CLI, skill, and bundled vocabulary share one release number.

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
