# Show commit intentions on a pull request

A PR starts with the reviewer’s title. Gitmoji adds the areas and intentions of its incoming commits and updates them as the PR changes. With **Create a merge commit**, each authored decision stays in the target branch’s history.

| Where | Example |
| --- | --- |
| Reviewer’s title | Give reviewers a working preview |
| Open PR | <code>[`🧱🎨` / `🛂✨`] Give reviewers a working preview</code> |
| Merge commit after landing PR 42 | <code>[`🧱🎨` / `🛂✨`] Give reviewers a working preview (#42)</code> |

## Enable automatic PR titles

1. From the project checkout, run `gitmoji init --github-workflows` and commit the generated `.github/workflows/gitmoji-pr-title.yml` and `.github/workflows/gitmoji-commits.yml`. The first maintains the title with `pull-requests: write`. The second uses `contents: read` to check the incoming commit range. Both also support reusable workflow callers. The runner needs access to the project’s GitHub host and the public Gitmoji distribution. Replace your previous title updater and commit-check job when the new jobs pass.

   For an existing project, first set `[vocabulary].version` to the release you are adopting. Set `[rules] signature = "agents"` if you want the hook to require agent signatures. On upgrade from 0.15.0, run plain `gitmoji init`. It migrates the old `pr-title.yml` and `check-commits.yml` files bearing Gitmoji’s managed header. Project-owned files stay in place. Review the additions and deletions together, including any reported conflict, before committing. Projects with reusable workflow callers should update both the tagged reference and its filename.

   Let the new `gitmoji-commits` job run on an upgrade PR, then select it in the target branch’s required checks. Keep `check` required if it also belongs to the project’s own CI. If it belonged solely to Gitmoji, replace the old requirement once the new check is available. The title job is named `gitmoji-pr-title` and starts using that name after the new workflow reaches the default branch. Reusable workflow callers should select the reported caller/job check name. [GitHub required checks](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches/about-protected-branches)

   GitHub loads the title workflow from the default branch. The PR that first adds it can keep its human title, or you can paste the output of `gitmoji merge-caption --pr NUMBER --title 'Human title' BASE HEAD` once. Subsequent PRs refresh automatically. [GitHub workflow events](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#pull_request_target)

2. Under **Settings → General → Pull Requests**, enable **Allow merge commits** and select **Pull request title and description** as the default message. Administrators can also apply these two settings with the GitHub CLI.

   ```sh
   gh api --method PATCH repos/OWNER/REPO \
     -F allow_merge_commit=true -f merge_commit_title=PR_TITLE -f merge_commit_message=PR_BODY
   ```

   Use `gh api --hostname HOST` on Enterprise Cloud. Required reviews, checks, queue settings and branch rules still apply. A branch requiring linear history needs its permitted landing method. [GitHub merge settings](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/configuring-pull-request-merges/configuring-commit-merging-for-pull-requests)
3. Open a PR or edit an existing PR’s title. After the title job finishes, push a commit with another intention and check that the prefix changes. Edit the human wording directly in GitHub. When ready, use **Create a merge commit** with its suggested title and body. GitHub puts the PR number at the end and carries the description into the merge body.

The workflow uses the current base, head and human title when the PR opens, reopens, receives a push, or is edited (including a base change). It fetches commit objects from the base repository and runs the formatter from the trusted Gitmoji release. It skips an already-correct title. A human edit made at the same instant as the final title update can race with it. GitHub’s title API has no expected-title parameter. A subsequent edit or push runs the sync again. [GitHub PR API](https://docs.github.com/en/rest/pulls/pulls#update-a-pull-request)

The managed workflow runs `gitmoji pr-title --pr NUMBER`, which updates the title on GitHub. Actions supplies `GITHUB_SERVER_URL`, `GITHUB_API_URL` and `GITHUB_REPOSITORY`. The workflow passes its job token as `GITMOJI_TITLE_TOKEN`, with `contents: read` and `pull-requests: write`. To preview an open PR from a full local clone, run `gitmoji github-merge --repo OWNER/REPO --pr NUMBER` and add `--hostname HOST` for an enterprise host. This invocation is read-only.

For automated dependency updates, configure a [Dependabot commit prefix](dependabot.md) so both the commit check and the PR title recognize the upgrade.

## Read or preview a caption

```text
[`🎨✨`] Give reviewers a working preview
[`🧱🎨` / `🛂✨`] Give reviewers a working preview
[`🧱🎨`… / `🔧✨📝🛂♻️`…] Roll out previews across environments
```

A single distinct heading stays together. With mixed headings, areas appear on the left and activities, modifiers and markers on the right, in order of first appearance. Repeated symbols use one slot. Three areas fit in full. At four or more, the first two and an ellipsis appear. An ordinary PR shows up to nine visible symbols, counting ellipses. A PR that introduces a merge commit shows up to fourteen. A merge already reachable from the base uses no extra space. If no incoming commit carries a recognized heading, the human title stands alone. Labels such as `[WIP]` remain part of the human title. A trailing reference to this same PR is removed before formatting so the merge gets one number. References to other PRs stay in place.

Open the pull request’s **Commits** tab to review the full sequence of intentions. An ellipsis marks omitted symbols, and even an untruncated title folds repeated symbols together.

From a full local clone, supply the target revision first and the source revision second.

```sh
gitmoji merge-caption --pr 42 --title 'Give reviewers a working preview' origin/trunk HEAD
```

The text output is the live PR title. With `--json`, `title` is that same string, `subject` is the proposed merge subject with the trailing PR number, and `recognized` and `total` report coverage. For an existing merge `M`, use `M^1 M^2`. A nested merge contributes its newly reachable authored commits once.

A PR that rolls back earlier work carries its own integrating number at the end, for example `(#42)`. A verified standalone undo inside it can name the earlier PR with `Revert #17 …`. See the [history upgrade guide](upgrade-history.md).

## Local branch merges

Branches can merge entirely on one machine. Both names and their incoming intentions come from Git, so the merge needs neither a remote nor a PR number. For example, if a search branch brings a new feature into an integration branch, its local merge subject can read:

```text
[`🧩✨`] search into integration: offer saved searches
```

`gitmoji init` installs the prepare-commit-msg hook. Update the CLI and the project’s version pin first. Re-run init in each clone with plain Git hooks. For a project-maintained Lefthook configuration, add the following command alongside any existing prepare-commit-msg commands, commit the configuration, and run `lefthook install` in each clone. Gitmoji refreshes its own previous unmodified template automatically.

```yaml
prepare-commit-msg:
  commands:
    gitmoji-merge-message:
      run: gitmoji merge-message "{1}" "{2}"
```

Fetch the base and merge it locally. Gitmoji reads Git’s pending merge parents, totals the newly arriving authored intentions across all of them and uses up to fourteen symbols. A shared ancestor counts once. A branch with no recognized tags still gets a direction and summary. A fast-forward creates no merge commit. A custom subject stays as written. Git supplies identical hook inputs if someone explicitly types its exact generated subject; use `git merge --no-commit` followed by `git commit -m '…'` to preserve a deliberately composed message in that case.

The body lists up to twenty incoming first-parent entries, newest first. Where available, it retains the paths in Git’s conflict notice. A local subject can read `trunk into search: #42 Show saved searches`. The reference at the front of the incoming title names the earlier PR. Its original heading, including any trailing `(#42)`, stays in the body. A later PR landing gets its own number at the end. One distinct incoming emoji pair stays together as `🧩✨`; multiple distinct headings use the two groups.

### Signing a local merge from an agent terminal

For an interactive merge, use `git merge --edit origin/<base>` and append the signature in the editor. In a terminal without an editor, start with `git merge --no-commit origin/<base>`. A fast-forward completes immediately. If there are conflicts, resolve and stage them first. Once a merge commit is pending, run the following from the repository root. Gitmoji reads the signature from the active harness session.

```sh
(
  set -eu
  pending=$(git rev-parse --git-path MERGE_MSG)
  signed=$(mktemp)
  trap 'rm -f "$signed"' EXIT
  gitmoji merge-message "$pending" merge
  git stripspace --strip-comments < "$pending" > "$signed"
  printf '\n' >> "$signed"
  gitmoji signature >> "$signed"
  git commit --file="$signed"
)
```

The commit-msg hook checks and stamps the supplied signature. Use `gitmoji history BASE..HEAD` to check a range or `gitmoji explain --commit HEAD` to identify a particular merge. A subject string alone cannot establish its parentage. GitHub's **Update branch** button cannot run this local hook. Use the [history upgrade preview](upgrade-history.md) for recorded integrations. Its `--local-merges-only` mode works offline.

## When the merge button is unavailable

For a direct merge of an ordinary PR, preview the title and body from a full clone. Gitmoji uses the selected GitHub host and credentials. An existing GitHub CLI login can supply those credentials.

```sh
gitmoji github-merge --repo OWNER/REPO --pr 42
```

Review the message, commit coverage and preview digest. Add `--merge --expect-preview REVIEWED_SHA256` to submit that exact proposal. See `--help` for enterprise hosts and preserving a custom message. Use the repository’s normal merge button, stack or queue whenever available.

## Stacks, queues and squash

Each native stacked PR summarizes its own reviewed layer against its immediate base. The stack lands bottom-up. When an upper layer is retargeted, the title workflow refreshes it against its new base. GitHub’s PR page carries the stack map. Review each layer’s title in GitHub. The queue or stack controls its final commit message. GitHub requires its asynchronous stack merge path, and custom commit title and body parameters are supported for direct merges only. Land the stack or queued PR through its usual protected path. [Stacked PRs](https://docs.github.com/en/pull-requests/get-started/about-stacked-prs) · [Merge API](https://docs.github.com/en/rest/pulls/pulls#merge-a-pull-request-asynchronously)

For a team that chooses **Squash and merge**, generate the caption while the reviewed head and base are available. Paste it into the squash dialog and keep the original squash details in the body. The target branch then has one commit for the whole PR. Its caption can point back to the review, but cannot restore the per-issue sequence of authored commits. Rebase merging produces no separate merge row. [GitHub merge methods](https://docs.github.com/en/pull-requests/reference/pull-request-merges)

For a repository with existing merges, start with the [history upgrade preview](upgrade-history.md).
