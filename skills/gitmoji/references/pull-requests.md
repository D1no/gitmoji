# Show commit intentions on a pull request

A PR starts with the reviewer’s title. Gitmoji adds the areas and intentions of its incoming commits and updates them as the PR changes. With **Create a merge commit**, each authored decision stays in the target branch’s history.

| Where | Example |
| --- | --- |
| Reviewer’s title | Give reviewers a working preview |
| Open PR | <code>[`🧱🎨` / `🛂✨`] Give reviewers a working preview</code> |
| Merge commit after landing PR 42 | <code>[`🧱🎨` / `🛂✨`] Give reviewers a working preview (#42)</code> |

## Enable automatic PR titles

1. Add the workflow to your default branch. On GitHub.com, use this caller in `.github/workflows/pr-title.yml`. A project that already runs a title formatter should replace it so a single workflow owns the prefix.

   ```yaml
   name: Pull request title
   on:
     pull_request_target:
       types: [opened, reopened, synchronize, edited]
   permissions:
     contents: read
     pull-requests: write
   jobs:
     caption:
       uses: D1no/gitmoji/.github/workflows/pr-title.yml@v0.14.0
   ```

   On a GitHub Enterprise Cloud host with its own domain, or when your organization requires workflows to live in the consumer repository, copy the [complete workflow from v0.14.0](https://github.com/D1no/gitmoji/blob/v0.14.0/.github/workflows/pr-title.yml) to that same path instead. It uses the host of the PR and downloads Gitmoji from its public release. The runner needs access to both hosts.
2. Under **Settings → General → Pull Requests**, enable **Allow merge commits** and select **Pull request title and description** as the default message. Administrators can also apply these two settings with the GitHub CLI.

   ```sh
   gh api --method PATCH repos/OWNER/REPO \
     -F allow_merge_commit=true -f merge_commit_title=PR_TITLE -f merge_commit_message=PR_BODY
   ```

   Use `gh api --hostname HOST` on Enterprise Cloud. Required reviews, checks, queue settings and branch rules still apply. A branch requiring linear history needs its permitted landing method. [GitHub merge settings](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/configuring-pull-request-merges/configuring-commit-merging-for-pull-requests)
3. Open a PR or edit an existing PR’s title. After the title job finishes, push a commit with another intention and check that the prefix changes. Edit the human wording directly in GitHub. When ready, use **Create a merge commit** with its suggested title and body. GitHub puts the PR number at the end and carries the description into the merge body.

The workflow uses the current base, head and human title when the PR opens, reopens, receives a push, or is edited (including a base change). It fetches commit objects from the base repository and runs the formatter from the trusted Gitmoji release. It skips an already-correct title. A human edit made at the same instant as the final title update can race with it. GitHub’s title API has no expected-title parameter. A subsequent edit or push runs the sync again. [GitHub PR API](https://docs.github.com/en/rest/pulls/pulls#update-a-pull-request)

## Read or preview a caption

```text
[`🎨✨`] Give reviewers a working preview
[`🧱🎨` / `🛂✨`] Give reviewers a working preview
[`🧱🎨`… / `🔧✨📝🛂♻️`…] Roll out previews across environments
```

A single distinct heading stays together. With mixed headings, areas appear on the left and activities, modifiers and markers on the right, in order of first appearance. Repeated symbols use one slot. Three areas fit in full. At four or more, the first two and an ellipsis appear. The complete inventory uses up to nine visible symbols, including ellipses. If no incoming commit carries a recognized heading, the human title stands alone. Labels such as `[WIP]` remain part of the human title. A trailing reference to this same PR is removed before formatting so the merge gets one number. References to other PRs stay in place.

From a full local clone, supply the target revision first and the source revision second.

```sh
gitmoji merge-caption --pr 42 --title 'Give reviewers a working preview' origin/trunk HEAD
```

The text output is the live PR title. With `--json`, `title` is that same string, `subject` is the proposed merge subject with the trailing PR number, and `recognized` and `total` report coverage. For an existing merge `M`, use `M^1 M^2`. A nested merge contributes its newly reachable authored commits once.

A PR that rolls back earlier work carries its own integrating number at the end, for example `(#42)`. A verified standalone undo inside it can name the earlier PR with `Revert #17 …`. See the [history upgrade guide](upgrade-history.md).

## Preview or merge from the terminal

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

The terminal helper requires Python 3.9 or later, a full Git clone, a matching Gitmoji CLI and an authenticated GitHub CLI. On a dedicated Enterprise Cloud domain, pass `--hostname HOST`. Read access suffices for a preview. Landing requires permission to merge.

The explicit merge keeps the PR number at the end of the subject and stores the branch provenance in the body. A merge into a release or integration branch names that destination in the body. A branch sync without a PR keeps its source, destination and conflict notes.

## Stacks, queues and squash

Each native stacked PR summarizes its own reviewed layer against its immediate base. The stack lands bottom-up. When an upper layer is retargeted, the title workflow refreshes it against its new base. GitHub’s PR page carries the stack map. Review each layer’s title in GitHub. The queue or stack controls its final commit message. GitHub requires its asynchronous stack merge path, and custom commit title and body parameters are supported for direct merges only. Land the stack or queued PR through its usual protected path. [Stacked PRs](https://docs.github.com/en/pull-requests/get-started/about-stacked-prs) · [Merge API](https://docs.github.com/en/rest/pulls/pulls#merge-a-pull-request-asynchronously)

For a team that chooses **Squash and merge**, generate the caption while the reviewed head and base are available. Paste it into the squash dialog and keep the original squash details in the body. The target branch then has one commit for the whole PR. Its caption can point back to the review, but cannot restore the per-issue sequence of authored commits. Rebase merging produces no separate merge row. [GitHub merge methods](https://docs.github.com/en/pull-requests/reference/pull-request-merges)

For a repository with existing merges, start with the [history upgrade preview](upgrade-history.md).
