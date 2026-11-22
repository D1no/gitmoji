# Tag Dependabot version updates

In `.github/dependabot.yml`, add these two lines to the existing `updates` entry. Copy the arrow from this example to keep it as Unicode in the signed commit subject.

```yaml
commit-message:
  prefix: "🧑‍💻⬆ — (deps)"
```

The arrow in the example is the single character `⬆` (U+2B06). Adding the optional emoji presentation selector U+FE0F produces `⬆️`. Dependabot can replace that two-code-point spelling with the ASCII shortcode `:arrow_up:` in Git while leaving the PR title as Unicode. Gitmoji accepts both Unicode spellings and the recorded bot shortcode. Generated title and merge inventories use the canonical `⬆️`. [Upstream shortcode behavior](https://github.com/dependabot/dependabot-core/issues/4565)

Choose the area the dependencies serve. For example, `🧩⬆ — (deps)` fits an application service and `👷⬆ — (ci)` fits GitHub Actions. Replace the scope with one your repository accepts. The em dash (`—`) marks an automated job with no ticket. Dependabot supplies the body and sign-off. Gitmoji’s default length rule warns about long bodies and lets them pass. With `[rules] length = "error"`, a long generated body can fail the commit check.

End the prefix at the closing parenthesis. Dependabot appends the colon and space. Omit `include: scope` when the prefix already contains a parenthesized scope. If an entry has a separate `prefix-development`, give it a complete heading too. Keep the project's ecosystems, directories, groups and schedules. [Dependabot commit-message options](https://docs.github.com/en/code-security/reference/supply-chain-security/dependabot-options-reference#commit-message)

## Compare the raw commit and the PR title

On a single version update, the signed Git subject and the managed PR title can look like this:

```text
👷⬆ — (ci): Bump ms from 2.1.0 to 2.1.3
[`👷⬆️`] Bump ms from 2.1.0 to 2.1.3
```

A grouped update has one heading when the bot groups its changes in one commit:

```text
🧑‍💻⬆ — (deps): Bump the number-helpers group across 1 directory with 2 updates
[`🧑‍💻⬆️`] Bump the number-helpers group across 1 directory with 2 updates
```

Use `git show -s --format=%s HEAD` on the bot branch to inspect the stored subject. To search subjects in an existing history, `git log --format=%s | grep -E '⬆|:arrow_up:'` finds both Unicode spellings and older shortcode subjects.

The [managed workflows](pull-requests.md#enable-automatic-pr-titles) validate the original signed commit and keep the full PR description, including version numbers or a directory that Dependabot shortened in the commit. From a full clone, `gitmoji history BASE..HEAD` checks the stored messages. `gitmoji merge-caption --pr NUMBER --json --title 'PR title' BASE HEAD` shows the recognized count and the generated title. A single bot commit should report `"recognized":1` and `"total":1`.
