# Land a pull request with its authored history

Choose **Create a merge commit** for Gitmoji repositories. Each reviewed decision then keeps its subject, ticket and commit on the target branch. A merge caption summarizes the PR in the file list. Open the PR or its authored commits to see which decision changed a particular file.

## Set up GitHub or GitHub Enterprise Cloud

1. Under **Settings → General → Pull Requests**, enable **Allow merge commits**. To make this the repository’s only landing method, disable squash merging and rebase merging there. Teams that also want squash can keep it available.
2. Check the rulesets and branch protection covering the target, including organization or enterprise rules. The permitted merge methods must include merge commits. **Require linear history** excludes two-parent merges. Where that rule is intentional, keep the caption with the PR and use the repository’s permitted method.
3. Keep required reviews, status checks and protected-branch rules in effect. For a required merge queue, choose the merge-commit method in the queue settings if the repository allows it. A local commit hook runs on the machine creating the commit. GitHub's default message choices are static. Generate the tag groups before landing.
4. Use a full Git clone, Python 3.9 or later, the current Gitmoji command and an authenticated GitHub CLI. For Enterprise Cloud on a dedicated domain such as `octocorp.ghe.com`, authenticate with `gh auth login --hostname octocorp.ghe.com` and pass `--hostname octocorp.ghe.com` to the helper. Read access suffices for inspection. Landing requires permission to merge the PR.

[Repository merge settings](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/configuring-pull-request-merges/configuring-commit-merging-for-pull-requests) · [Enterprise Cloud settings](https://docs.github.com/en/enterprise-cloud@latest/repositories/configuring-branches-and-merges-in-your-repository/configuring-pull-request-merges/configuring-commit-merging-for-pull-requests) · [Merge queues](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/configuring-pull-request-merges/managing-a-merge-queue)

## Preview and land

Run this from the target repository. The example uses the skill path installed by `gitmoji init`. For another skill installation, use the same script under that skill’s directory.

```sh
python3 .claude/skills/gitmoji/scripts/github-merge.py --repo OWNER/REPO --pr 42
```

The helper uses GitHub’s PR number, full title, head and base. If your clone lacks the commits, fetch from the remote for the **base repository**, including the PR head for a fork. For a PR targeting `release/2.x` on `origin`, for example:

```sh
git fetch origin refs/heads/release/2.x refs/pull/42/head
```

In a shallow clone first fetch the full history with `git fetch --unshallow origin`. Standard output contains the proposed commit subject and body, including the source and destination. Standard error shows the two revision IDs, how many incoming non-merge commits supplied recognized tags, and a SHA-256 preview digest. Read the message and coverage before approving the merge. For example, one recognized heading across six commits means the emojis describe only one of the six. To preserve custom text, conflict notes and trailers, pass the complete original message in a UTF-8 file with `--original-message FILE` on both invocations.

When the repository’s merge decision is made, request a direct merge of an ordinary open PR with the digest from that reviewed preview:

```sh
python3 .claude/skills/gitmoji/scripts/github-merge.py --repo OWNER/REPO --pr 42 \
  --merge --expect-preview REVIEWED_SHA256
```

The helper verifies the host, repository, PR, source and destination revisions, method, proposed message and coverage. If an input changes, inspect a fresh preview. GitHub applies the rules configured for the caller and atomically checks the expected **head**. Its merge endpoint has no corresponding expected-base parameter. The helper rereads the destination immediately before sending the request, so a change after that read can still affect the merge. [GitHub merge endpoint](https://docs.github.com/en/rest/pulls/pulls#merge-a-pull-request)

For a human merge, copy the first line of standard output to GitHub’s merge title and the remaining text to the message. A merge into a release or integration branch names that destination in the body. A branch sync without a PR keeps its source, destination and conflict notes.

## Read the caption

```text
Merge #42 [`🧱🎨` / `🛂✨`] Give reviewers a working preview
Merge #43 [`🧱🎨`… / `🔧✨📝🛂♻️`…] Roll out previews across environments
```

The number opens the PR. Areas appear on the left and activities, modifiers and markers on the right. A repeated pair appears once, and a single distinct heading stays together. Distinct symbols retain their order of first appearance. Three areas fit in full. At four or more, show the first two and `…`. Nine visible symbols fit between the brackets, counting each ellipsis once. An ellipsis that follows a backtick belongs to the group that overflowed. The full PR title follows the brackets. If the commits contain no recognized intentions the subject contains the operation, number and title alone.

For a local preview without a host connection, supply the target and source revisions in that order:

```sh
gitmoji merge-caption --pr 42 --title 'Give reviewers a working preview' origin/trunk HEAD
```

For an existing two-parent merge `M`, supply `M^1` and `M^2` to reconstruct the range that arrived at that point in history. Nested PRs use authored commits newly reachable from the destination. Earlier merge captions do not contribute a second set of symbols.

A PR that undoes a change still lands as `Merge #42 …`. Its number opens the integrating PR, including any additional work it contains. Inside that PR, a verified standalone undo can point to its target with `Revert #17 …`. The [history upgrade guide](upgrade-history.md) describes how undo evidence is checked.

## Stacks, queues and squash

Each native stacked PR summarizes its own reviewed layer against its immediate base. The stack lands bottom-up. When an upper layer is retargeted, preview it against its current base. GitHub’s PR page carries the stack map. Keep the layer caption in the PR description when the host controls the commit text. GitHub requires its asynchronous stack merge path, and custom commit title and body parameters are supported for direct merges only. Land the stack or queued PR through its usual protected path. [Stacked PRs](https://docs.github.com/en/pull-requests/get-started/about-stacked-prs) · [Merge API](https://docs.github.com/en/rest/pulls/pulls#merge-a-pull-request-asynchronously)

For a team that chooses **Squash and merge**, generate the caption while the reviewed head and base are available. Paste it into the squash dialog and keep the original squash details in the body. The target branch then has one commit for the whole PR. Its caption can point back to the review, but cannot restore the per-issue sequence of authored commits. Rebase merging produces no separate merge row. [GitHub merge methods](https://docs.github.com/en/pull-requests/reference/pull-request-merges)

For a repository with existing merges, start with the [history upgrade preview](upgrade-history.md).
