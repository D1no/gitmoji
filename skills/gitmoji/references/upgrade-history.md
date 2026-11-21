# Reviewing existing merge and revert history

Run the history upgrade script from a full clone of the project whose history you want to review. Use Python 3.9 or newer, Git, an installed `gitmoji` with the `merge-caption` command, and an authenticated `gh` with GraphQL pagination and `--slurp` support. The GitHub account needs read access to the repository and its pull requests. Pass `--hostname HOST` for an enterprise host, or set `GH_HOST`. The example paths below use the skill installed by `gitmoji init`.

## Preview

```sh
python3 .claude/skills/gitmoji/scripts/gitmoji-upgrade-history.py \
  --repo OWNER/REPO --ref trunk > history-plan.json
```

The default run reads Git objects and the repository’s merged PRs. The JSON report includes the original tip, proposed candidate tip, old and proposed commit IDs, new subjects, decisions needing review and signatures that would become invalid. Use `--base <commit>` to exclude a verified ancestor and all its ancestors from the upgrade. Choose a path outside the checkout for the report if you want to keep the working directory clean.

Review these decisions against the linked PRs:

- A two-parent merge qualifies when exactly one merged PR in the named repository reports that exact commit as its merge commit. Its caption uses the incoming commits between the two parents. The new body includes the source commit ID and available head and base names from the PR record, followed by the entire original message, including conflict notes. An unmerged closed PR or a number typed into a subject cannot supply this evidence.
- A merge that lands a rollback PR still gets a `Merge` caption for the PR that landed. Its individual undo commit can qualify separately. For example, when PR #31 rolls back PR #27, its integration row reads `Merge #31 …`. A proven undo inside it reads `Revert #27 …`, with `Introduced by PR #31.` in the body when GitHub associates that commit uniquely and confirms it appears in the PR.
- An undo candidate needs a recognizable operation subject and one full Git revert target ID in its parent history. To prove the undo, the tool compares the complete inverse against the actual change using original Git trees. Every changed path, file mode and pair of blob IDs must match. An undo of a merge also needs Git’s first-parent mainline record. These checks apply at every step of a revert chain. Reverting the verified merge of PR #27 yields `Revert #27` followed by the target PR’s caption. Reverting that exact undo yields `Reapply #27`.
- Intervening changes to other files can coexist with an exact undo. An edit elsewhere in the same affected file changes its blob ID and requires review, even when Git reversed the intended lines cleanly. Partial rollbacks, additional files, empty changes and unproved conflict resolutions also require review. A squash that both reverts a change and adds a feature keeps its original subject. A recorded one-parent PR landing always goes to review because it may be a squash or a rebase.
- For an undo of an individual authored commit, GitHub must associate the target with a unique merged PR and list the target on that PR. The original target subject describes what was undone, for example `Revert #27 🎨✨ #19 (preview): open the preview in a new tab`. The first number identifies the affected PR. The second belongs to the original subject. An undo of an exact one-parent landing names the target as that PR’s landing commit. The tool respects the target commit’s declared text encoding and sends unreadable or empty target subjects to review.
- Branch syncs, ambiguous PR links, and subjects such as `Rewind #27` without a provable inverse appear for review. The candidate keeps the original message for those entries. Keep the report alongside the source history when investigating them.
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

Promoting any candidate onto shared history is a separate repository-owner decision. This command never pushes, updates hosted PRs, moves tags, or overwrites a branch.

## Normalize old version stamps

From 0.13.0 the binary, vocabulary, skill reference and commit stamp use the
same release number. Install the current release and set the existing
`[vocabulary].version` in your project settings to that number in a 🔺
adoption commit. Historical normalization is optional.

| Old rule stamp | Equivalent Gitmoji release |
| --- | --- |
| `2.0.0-draft.21` | `0.10.0` |
| `2.0.0-draft.22` | `0.10.1` |
| `2.0.0-draft.23` | `0.11.0` |
| `2.0.0-draft.24` | `0.12.0` |

These numbers name the rules used on each commit. Earlier concept binaries
also shipped some of the same rules. The migration changes a closing △ stamp
and a 🔺 adoption subject where the mapping is known. It keeps the file trees
and message prose. Unknown or ambiguous stamps appear in the review list.
Run this separately from a PR-caption migration. It works offline.

From a full clone with the skill installed, save the backup and report outside
the project. Substitute the actual branch for `trunk` when needed.

```sh
git bundle create ../before-version-normalization.bundle --all
git bundle verify ../before-version-normalization.bundle
python3 .claude/skills/gitmoji/scripts/gitmoji-upgrade-history.py \
  --normalize-versions --ref trunk > ../versions-preview.json
```

Review the mappings and any signed or unresolved commits. Use the two full
object IDs printed as `source_tip` and `candidate_tip` for the following run.

```sh
python3 .claude/skills/gitmoji/scripts/gitmoji-upgrade-history.py \
  --normalize-versions --ref SOURCE_TIP --expect-candidate CANDIDATE_TIP \
  --apply review/versions
git diff --exit-code SOURCE_TIP review/versions
git log --graph --format='%h %s' review/versions
```

If the owners choose to replace the remote branch, use an explicit lease
against its reviewed original remote tip. Keep published release tags pinned
to the source used to build their assets. For another machine, a fresh
clone after the push starts from the normalized history. Preserve any local-only
work in its own backup before switching checkouts.
