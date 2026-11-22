# Reviewing existing merge and revert history

Run the history upgrade script from a full clone of the project whose history you want to review. Use Python 3.9 or newer, Git, an installed `gitmoji` with the `merge-caption` and `merge-message --historical` commands, and an authenticated `gh` with GraphQL pagination and `--slurp` support. The GitHub account needs read access to the repository and its pull requests. Pass `--hostname HOST` for an enterprise host, or set `GH_HOST`. The example paths below use the skill installed by `gitmoji init`.

## Preview

```sh
python3 .claude/skills/gitmoji/scripts/gitmoji-upgrade-history.py \
  --repo OWNER/REPO --ref trunk > history-plan.json
```

The default run reads Git objects and the repository’s merged PRs. The JSON report includes the original tip, proposed candidate tip, old and proposed commit IDs, new subjects, decisions needing review and signatures that would become invalid. Use `--base <commit>` to exclude a verified ancestor and all its ancestors from the upgrade. Choose a path outside the checkout for the report if you want to keep the working directory clean.

Review each proposed caption against its incoming history and any linked PR.

- An exact match between a two-parent merge and a single merged PR’s recorded merge ID identifies the landing. The tool rebuilds the emoji inventory from the incoming commits. It preserves the original message and records the original commit ID and available branch names in a changed commit’s body.
- An earlier history rewrite can change every merge ID. For a stored caption such as <code>Merge #42 [`🎨✨`] Show preview</code>, the tool also checks the merged PR in the selected repository, the human wording and the inventory recomputed from both parents. The source history must have a single merge claiming that caption. When the original landing object is available, Gitmoji compares the result tree and both parent trees in their original order. All three commit IDs can change. A rewrite may also change every snapshot, for example by adding the same unrelated file throughout history. In that case, Gitmoji checks the complete change from each parent to the merge, including paths, modes and blob IDs. At least one side must change, and the merge and its two parents must retain their corresponding Git author identities and dates. Missing evidence or disagreement goes to review. A qualifying merge becomes <code>[`🎨✨`] Show preview (#42)</code>. Repeated trailing references to the integrating PR collapse to one.
- Branch merges whose stored Git heading identifies the source and destination can be captioned from their ordered parents, even after the branches have been deleted. For example, `Merge remote-tracking branch 'origin/layout' into prototype` can become <code>[`🎨✨`] layout into prototype: open the preview</code>. The caption allows up to fourteen symbols. Its body lists the incoming first-parent commits and keeps the original commit ID and message, including any surviving conflict notes or trailers. When an arriving commit also gets rewritten, its inventory entry uses the full candidate commit ID and new subject so it remains reachable after publication. The upgrade cannot recover conflict paths Git discarded. A PR number in an arriving title belongs to that incoming PR and never triggers a hosted title update for the branch merge.
- A PR integrating a branch that contains a newly arriving merge can gain a longer inventory. A verified older nine-slot caption can expand to fourteen using the original authored commits. Ordinary PRs retain the nine-slot inventory.
- A merge that lands a rollback PR carries the integrating PR number at the end. Its individual undo commit can qualify separately. For example, when PR #31 rolls back PR #27, its integration row ends in `(#31)`. A proven undo inside it reads `Revert #27 …`, with `Introduced by PR #31.` in the body when GitHub associates that commit uniquely and confirms it appears in the PR.
- An undo candidate needs a recognizable operation subject and one full Git revert target ID in its parent history. To prove the undo, the tool compares the complete inverse against the actual change using original Git trees. Every changed path, file mode and pair of blob IDs must match. An undo of a merge also needs Git’s first-parent mainline record. These checks apply at every step of a revert chain. Reverting the verified merge of PR #27 yields `Revert #27` followed by the target PR’s caption. Reverting that exact undo yields `Reapply #27`.
- Intervening changes to other files can coexist with an exact undo. An edit elsewhere in the same affected file changes its blob ID and requires review, even when Git reversed the intended lines cleanly. Partial rollbacks, additional files, empty changes and unproved conflict resolutions also require review. A squash that both reverts a change and adds a feature keeps its original subject. A recorded one-parent PR landing always goes to review because it may be a squash or a rebase.
- For an undo of an individual authored commit, GitHub must associate the target with a unique merged PR and list the target on that PR. The original target subject describes what was undone, for example `Revert #27 🎨✨ #19 (preview): open the preview in a new tab`. The first number identifies the affected PR. The second belongs to the original subject. An undo of an exact one-parent landing names the target as that PR’s landing commit. The tool respects the target commit’s declared text encoding and sends unreadable or empty target subjects to review.
- Branch merges whose stored heading omits the destination, custom headings without a verified PR, ambiguous PR links, and subjects such as `Rewind #27` without a provable inverse appear for review. The candidate keeps the original message for those entries. Keep the report alongside the source history when investigating them.
- A commit whose parent changes acquires a new ID even if its entire message stays the same. Each rewritten commit retains its tree, parent order, author and committer identities and timestamps. The report maps the old and new IDs. GitHub’s existing PR records, checks, URLs, and other external references still refer to the original history. Keep an accessible copy of that history.

## Build and inspect the candidate

After reviewing the plan, use the reported `source_tip` and the same base and hostname options. Choose a fresh local branch name.

```sh
python3 .claude/skills/gitmoji/scripts/gitmoji-upgrade-history.py \
  --repo OWNER/REPO --ref ORIGINAL_TIP_SHA --expect-candidate REVIEWED_CANDIDATE_SHA \
  --apply review/gitmoji-history > history-candidate.json
git diff --exit-code ORIGINAL_TIP_SHA review/gitmoji-history
git log --graph --format='%h %s' review/gitmoji-history
```

The tool reads the current PR metadata again when building the candidate. Passing the reviewed `candidate_tip` to `--expect-candidate` stops the operation before branch creation if a PR title or another input would change the candidate. It creates the candidate ref only after the complete graph has been processed. It checks for existing branches, symbolic refs and branch names held by any listed worktree, including a branch awaiting its first commit. It repeats this check before atomically creating the ref. Reserve the name for this operation while it runs because another process can check out an unborn branch between the worktree check and ref creation. The source branch, its tags, the remote and the working tree keep their original state. When an undo target also received a new ID, the new body records its candidate ID alongside Git’s original undo message. The final `git diff` should be empty. Open selected commits with `git show -s --format=%B <sha>` to inspect their original messages and provenance.

Rewriting invalidates Git commit signatures. The default dry run lists affected signed commits. Applying that plan requires `--allow-unsigned` and drops their signature headers on the candidate. A changed commit containing a signed merge tag, or a recaptioned commit with an explicit legacy message encoding, requires separate review and blocks candidate creation. A failed application can leave unreachable objects for Git’s normal garbage collection.

For an entry marked for review, inspect the original commit and its parents with
`git show -s --format=raw COMMIT` and open the related PR timeline. A number in
the old subject is a lead. Check its repository, merged state and commit IDs
before proposing a link. Record unresolved associations in the review report.

Before a shared rollout, keep the source refs and report in a backup. For example,
`git bundle create ../before-gitmoji.bundle --all` followed by
`git bundle verify ../before-gitmoji.bundle` preserves reachable Git history.
GitHub PR pages and review metadata live on the host. Coordinate with active
contributors and check the target branch's signing and protection policies.

Promoting any candidate onto shared history is a separate repository-owner decision. The local candidate operation creates only the new branch. Updating a hosted title uses the separate operation below.

## Update titles of already merged PRs

Use the same full clone and the same repository, host and revision range. This also works for a merge whose local subject already has its final form. Creating or publishing a replacement branch is optional.

```sh
tool=.claude/skills/gitmoji/scripts/gitmoji-upgrade-history.py
python3 "$tool" --repo OWNER/REPO --ref trunk --update-pr-titles > titles.json
```

Read `pr_title_summary` before interpreting an empty update list. The counts cover the selected revision range.

| Field | What it counts |
| --- | --- |
| `to_change` | Verified landings whose hosted PR title would change |
| `already_correct` | Verified landings whose hosted PR already has the proposed title |
| `unmatched` | Merges with unresolved identity or direction, and recorded one-parent PR landings requiring review. A recognized local branch merge is excluded from all three counts |

For example, `{"to_change": 0, "already_correct": 12, "unmatched": 3}` means twelve hosted titles were verified. The three unresolved landings have review reasons in `review_or_changed_commits`. When an original merge is available but the trees differ, the reason names the original SHA and the evidence that failed.

Inspect the `pr_title_updates` entries, including the PR number, the evidence, and each title before and after. Gitmoji takes the title from the landed caption and drops the trailing reference to the PR itself. When the changes are ready, use an account with pull request write access.

```sh
python3 "$tool" --repo OWNER/REPO --ref trunk --apply-pr-titles titles.json
```

The apply step recreates the plan and rereads each merged PR immediately before editing its title. If another title changed or GitHub stopped a batch partway, inspect the result and generate a fresh preview. An already-correct title needs no write. GitHub’s title endpoint has no atomic expected-title check. [GitHub PR API](https://docs.github.com/en/rest/pulls/pulls#update-a-pull-request)

## Adopt a release with earlier stamps in history

Change `[vocabulary].version` in the project’s settings, install the matching Gitmoji release and run `gitmoji init`. Commit the version change with the `🔺 gitmoji` adoption marker. New commits receive the new stamp. Historical stamps record the vocabulary used at the time. Projects that previously used draft numbers may retain those stamps. The caption upgrade preserves them. Use `gitmoji history --unmigrated` to inspect older messages against today’s convention.
