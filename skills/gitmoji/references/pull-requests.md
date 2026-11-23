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

If GitHub refuses a direct merge with HTTP 405, Gitmoji reports the host's reason when available and reads the PR once to see what changed. A changed head or base requires a fresh preview. If GitHub is still calculating mergeability, let the checks settle and preview again. A green PR on that later read can indicate a temporary condition. Only the host's original response can identify what blocked the earlier attempt. Inspect required reviews, checks or rules on the PR when GitHub gives no specific cause.

The command submits a single merge request. Before retrying, confirm the PR is still open, then run the preview and submit its new digest. For HTTP 409, fetch the current head and review a new preview before submitting it. HTTP 422 calls for checking the proposed merge and repository rules. A connection failure, server error or incomplete confirmation leaves the outcome uncertain. Check the PR before trying again.

## Stacks, queues and squash

Each native stacked PR summarizes its own reviewed layer against its immediate base. The stack lands bottom-up. When an upper layer is retargeted, the title workflow refreshes it against its new base. GitHub’s PR page carries the stack map. Review each layer’s title in GitHub. The queue or stack controls its final commit message. GitHub requires its asynchronous stack merge path, and custom commit title and body parameters are supported for direct merges only. Land the stack or queued PR through its usual protected path. [Stacked PRs](https://docs.github.com/en/pull-requests/get-started/about-stacked-prs) · [Merge API](https://docs.github.com/en/rest/pulls/pulls#merge-a-pull-request-asynchronously)

For a team that chooses **Squash and merge**, generate the caption while the reviewed head and base are available. Paste it into the squash dialog and keep the original squash details in the body. The target branch then has one commit for the whole PR. Its caption can point back to the review, but cannot restore the per-issue sequence of authored commits. Rebase merging produces no separate merge row. [GitHub merge methods](https://docs.github.com/en/pull-requests/reference/pull-request-merges)

For a repository with existing merges, start with the [history upgrade preview](upgrade-history.md).

## Land a demo or comparison with a follow-up

A greybox feed for a stakeholder demo can start with `🎨✨🚧 #42 (feed): show the demo feed`. Use 🧪 for an experiment, such as comparing two feed layouts before choosing one. If the implementation will ship and only its styling is temporary, mark the commit that introduces the temporary styling. Keep settled decisions in their own commits.

The managed title puts 🚧 or 🧪 before the inventory whenever any incoming authored commit uses that modifier. Both can appear on a mixed PR. For example, the open title could read <code>🚧 [`🎨` / `✨💄`] Preview the feed</code>. The merge commit carries the same title with its PR number. Write the human title when opening the PR. Gitmoji maintains the prefix, including when a commit is amended away.

1. Create one issue in the base repository, label it `provisional` (or the configured label), and use that issue number in each 🚧 or 🧪 commit. Describe what can wait and how to decide whether to finish or remove it. A project board can use that label to set its own risk field.
2. Install the opt-in integration with `gitmoji init --github-workflows --provisional-workflow`. Commit the generated workflows and reviewer instructions. Keep a PR in draft while iterating. Repository-owned previews and on-demand builds can run on drafts. Where the repository disables automatic draft review, mark the PR ready or request a review when the demo is ready. For one review round with review-on-push enabled, make the final push while the PR is a draft, mark it ready, then merge the reviewed head after checks pass. A fix pushed after that review can trigger another round. [Copilot automatic review settings](https://docs.github.com/en/copilot/how-tos/copilot-on-github/set-up-copilot/configure-code-review)
3. Keep required build, types, tests, commit validation, code scanning and secret protection in force. Narrow discretionary polish review only for the provisional part. A broken demo action, incorrect permissions, or data corruption still requires a fix. Reviewer instructions guide human and AI judgment. Required checks and review counts are configured by repository administrators.
4. Use `gitmoji defer-findings --repo OWNER/REPO --pr 17` from a full clone with the current base and PR head fetched. Inspect the findings. Add `--thread THREAD_ID --apply` for each batch of selected polish findings (repeat `--thread` for multiple findings). Gitmoji appends a checklist comment with the file, line when available, finding and permanent discussion link to the verified issue, replies in the original thread, and resolves it after both writes succeed. Rerun with the same identity after an interrupted transfer. For Enterprise Cloud supply `--hostname YOUR-ENTERPRISE.ghe.com`; for a custom issue label pass `--label LABEL`.
5. Merge when the required checks and project review policy allow it. `gitmoji open HEAD --json` reports open work. `gitmoji changelog FROM..TO` discloses it in release notes; a project can select `[changelog] provisional = "refuse"` once releases must contain settled work only. After finishing (🏁) or removing (⚰️) the work, use the same ticket on a closing commit without a 🚧 or 🧪 modifier and put `Closes #42` in the landing PR description to close the GitHub issue when that PR merges into the default branch. [GitHub closing keywords](https://docs.github.com/en/issues/tracking-your-work-with-issues/using-issues/linking-a-pull-request-to-an-issue)

### Enroll a checked-in branch ruleset

For direct merges, let the workflow run once on the default branch’s PRs. Its commit status on the PR head is named `gitmoji/provisional`. An ordinary PR receives `pending` and then `success` after validation. A marked PR must also pass the follow-up issue and label checks. An existing PR can use the workflow’s manual recheck. The read-only classifier publishes no status.

To add the entry to a ruleset maintained as JSON, ask the target GitHub host for its Actions app ID. On github.com this prints `{"context":"gitmoji/provisional","integration_id":15368}`.

```sh
gh api --hostname YOUR-HOST.ghe.com /apps/github-actions \
  --jq '{context:"gitmoji/provisional", integration_id:.id}'
```

Merge this entry into `parameters.required_status_checks` on the existing `required_status_checks` rule. Preserve the other checks and the repository’s branch strictness. The integration ID identifies the GitHub App publishing the status. [GitHub rulesets](https://docs.github.com/en/enterprise-cloud@latest/rest/repos/rules#update-a-repository-ruleset)

### Recognize a deferred review thread

`defer-findings` writes the same hidden receipt as the last line of the issue entry and the thread reply. For PR 17, follow-up issue 42, and root review comment 12345, its literal form is:

```html
<!-- gitmoji-deferred pr=17 issue=42 finding=12345 -->
```

A watcher can match those three numbers and the authorized replying identity, then check that the thread is resolved. The finding ID is the database ID from the original comment’s `discussion_r12345` permalink. A reply can appear before resolution succeeds. A deferred finding still belongs on the follow-up checklist.

### Use the type in CI

`merge-caption --json` exposes the complete `modifiers` and `modifier_tickets` even when the displayed inventory is abbreviated. For reporting or preview routing, read those fields from the incoming base and head commits. For decisions that relax review, use the validated hosted result. Run the command from trusted workflow code, with the expected event head and the repository’s configured label. Use successful, current output to select the optional review job. On failure, require normal review or block the decision.

```sh
gitmoji provisional-pr --check --json --repo "$GITHUB_REPOSITORY" \
  --pr "$PR_NUMBER" --expect-head "$PR_HEAD_SHA" --label provisional
```

On success the JSON field `policy` is `ordinary` or `provisional`. A project can pass this value as a protected workflow job output and use `if: needs.classify.outputs.policy == 'ordinary'` for a discretionary final-polish job. Keep the classification check itself required if its failure should block landing. Check `modifiers` when a draft preview should run specifically for 🧪. The project owns which preview environment or build channel receives a build.

Keep always-required jobs independent of this decision. GitHub treats skipped jobs as successful. Editing a PR title does not change a branch ruleset, and narrowing optional polish review does not grant merge permission. [GitHub check statuses](https://docs.github.com/en/pull-requests/reference/status-checks)

### Keep a greybox through two cleanup PRs

The team previews a card feed, fixes its screen-reader labels and keyboard flow, then decides to keep the same screen. Issue #42 tracks the provisional feed. Issues #51 and #52 track independently reviewable improvements. The destination in this example is the repository’s default branch.

| PR | Authored commit subjects, oldest first | PR description | `gitmoji open HEAD` on the destination after merge |
| --- | --- | --- | --- |
| #17 · Demo | `🎨✨🚧 #42 (feed): show the card feed for the demo` | `Preview the card feed. Follow-up: #42.` | `🚧 #42` |
| #18 · First cleanup | `🎨💬 #51 (feed): name each card for screen readers` | `Closes #51. Part of #42.` | `🚧 #42` |
| #19 · Final cleanup | `🎨🚸 #52 (feed): keep focus on the selected card`<br>`🎨🏁 #42 (feed): keep the demo feed as the production screen` | `Closes #52. Closes #42.` | Empty output. With `--json` the result is `[]`. |

After the keyboard fix and before the 🏁 commit, the output is still `🚧 #42`. An author can finish an individual finding without deciding whether the whole feed is ready. The maintainer records acceptance after reviewing the remaining findings. The final PR’s inventory then retains both interaction and acceptance. 🏁 means the team kept the result. A later rollout or release is a separate decision.

If the team discards an experimental layout, a subject such as `🎨⚰️ #42 (feed): remove the comparison layout after the trial` records that outcome. Each closer covers qualifying ancestor openings on its own ticket. Either closer settles that ticket in the listing, so save closure until all of its obligations are settled. Give independently retained or discarded alternatives distinct follow-up tickets when opening them in separate marked PRs. For a mixed existing ticket, describe both outcomes in the issue, keep the ticket open during partial cleanup, and record the final decision once.

When the screen is accepted as built, a maintainer can record the reason in an acceptance note or use an explicitly empty decision commit if project policy allows it. The normal subject, body and signature rules still apply. Manually closing a GitHub issue or checking off its last item leaves the Git ledger as it was. `gitmoji changelog` reads that ledger as of the release revision. For a more descriptive release note, add the user-facing consequence alongside the ticket, for example “Screen-reader card labels are pending (#42).”
