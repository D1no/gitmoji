# Tag Dependabot version updates

In `.github/dependabot.yml`, add these two lines to the existing `updates` entry for the dependency set. Keep the project's ecosystem, directories, groups and schedule.

```yaml
commit-message:
  prefix: "🧩⬆️ — (deps)"
```

Choose the area the dependencies serve. For example, `🧩⬆️` fits an application service and `👷⬆️` fits GitHub Actions updates. Replace `deps` with a scope your repository accepts. The em dash (`—`) marks an automated job with no issue ticket. It needs no hand-written rationale or agent signature under the default rules. Dependabot supplies the body and sign-off.

End the prefix at the closing parenthesis. Dependabot appends the colon and space. Omit `include: scope` when the prefix already contains a parenthesized scope. If an entry has a separate `prefix-development`, give it a complete Gitmoji heading too. [Dependabot commit-message options](https://docs.github.com/en/code-security/reference/supply-chain-security/dependabot-options-reference#commit-message)

## What a reviewer sees

A single npm update and a two-package group using the same prefix produced the following commit subjects on GitHub. Dependabot preserved the Unicode in the PR titles and stored the arrow as a shortcode in Git. Gitmoji reads this spelling for recorded Dependabot job upgrades. The repository keeps the original signed commits. [Upstream shortcode behavior](https://github.com/dependabot/dependabot-core/issues/4565)

```text
🧩:arrow_up: — (deps): Bump ms from 2.1.0 to 2.1.3
🧩:arrow_up: — (deps): Bump the number-helpers group with 2 updates
```

With the [managed workflows](pull-requests.md#enable-automatic-pr-titles), `gitmoji-commits` checks the incoming commit on each PR. The title workflow combines its recognized heading with Dependabot's description. The grouped PR has one heading because the two upgrades share a commit. A reviewer can edit the description directly on the PR.

```text
[`🧩⬆️`] Bump ms from 2.1.0 to 2.1.3
[`🧩⬆️`] Bump the number-helpers group with 2 updates
```

To verify your setup, open a new Dependabot PR and check both the commit check and the rendered title. From a full clone with the bot branch fetched, `gitmoji history BASE..HEAD` validates the stored message and `gitmoji merge-caption --pr NUMBER --json --title 'PR title' BASE HEAD` shows the recognized commit count. A one-commit update should report `"recognized":1` and `"total":1`.
