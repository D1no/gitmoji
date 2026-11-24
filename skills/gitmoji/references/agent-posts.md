# Mark agent-written posts on GitHub

When a human and an agent post using the same GitHub account, put a visible `🤖` chip before the agent's text. Use it on issue and PR **bodies**, conversation comments, review summaries and inline review comments or replies. A short prose reply shows the chip on the same line. Structured or multiline Markdown starts below the chip so headings, lists and code keep their formatting. Titles retain their normal format. Gitmoji signs the Markdown locally. You choose when and where to send it.

```sh
gitmoji comment sign < draft.md > signed.md
gitmoji comment check < signed.md
gh issue comment 42 --body-file "$PWD/signed.md"
```

Use separate input and output files. For GitHub Enterprise, supply the same signed file to the CLI or tool for your host. The check reads the raw Markdown, including the final HTML comment. GitHub's rendered view shows the chip and prose. Anyone who can read the raw post can read the metadata too.

## Lineage and edits

The default records the harness, model and effort. Use `--full-lineage` when sharing the session identifier, role and available helper/team/activity data is appropriate for the destination. This is useful on a private project that needs to connect a comment to a local session. Missing information stays unknown or absent. Claude Code, Codex and Antigravity use Gitmoji's existing session discovery. Other harnesses can identify themselves explicitly, for example `gitmoji comment sign --harness cursor`.

The versioned footer carries a SHA-256 fingerprint. A matching fingerprint means the current Markdown matches the recorded text. Someone with write access can copy, replace or recompute the entire record. It carries no signing key. Line-ending normalization permits LF and CRLF. Markdown spaces, code and wording are significant.

```sh
gitmoji comment check --json < raw-body.md
gitmoji comment check --allow-human --json < mixed-source-body.md
gitmoji comment trace 'https://github.com/OWNER/REPO/pull/17#discussion_r12345' --json
```

A check reports `verified` when the checksum matches, `edited` when the text has changed under a readable record, `invalid` for incomplete, malformed or unsupported marks, and `unsigned` for unmarked text. Strict checks return exit code 1 for the last three states. Trace is a read operation and returns the observed status in its report.

For tools that read the raw format, the final paragraph is `<!-- gitmoji-post:v1 PAYLOAD -->`. The payload is UTF-8 JSON encoded with standard unpadded Base64. The `sha256` field contains the lowercase hex digest of the visible Markdown, including the chip, up to the blank-line separator added before the footer. The `lineage` object holds the recorded fields. Use `comment check --json` to decode and validate both fields.

Use strict checking for a known agent post. Mixed-source mode also accepts unmarked text because humans participate in the same threads. If both marks have been removed, the current body alone cannot tell who originally wrote it. Keep ordinary GitHub credentials scoped to the intended repository. For a configured Enterprise host, use `--hostname HOST` with trace.

Trace follows the specific body in the permalink. On GitHub.com the GraphQL `Comment` interface exposes the last edit, editor account and visible history on issues, PRs, conversation comments, reviews and inline review comments. Trace reports the fingerprint separately from that history. It shows up to 20 visible revisions with timestamps, editor accounts and deletion markers where available. The report also indicates whether further pages exist. A general update timestamp can also advance when another property changes. An editor account can represent either the person or their agent. Restricted or deleted history leaves gaps. GitHub Enterprise Cloud publishes the same interface; check history availability with the credentials used on your deployment. [GitHub schema](https://docs.github.com/en/graphql/reference/interfaces#comment) · [Enterprise Cloud schema](https://docs.github.com/en/enterprise-cloud@latest/graphql/reference/interfaces#comment)

When an agent revises a post, remove both old marks, review the revised Markdown, and sign the new revision. An automated change to the visible provisional follow-up block also changes the fingerprint. The workflow preserves the original footer so trace can report the edit. When a person takes over authorship, remove both the leading chip and the metadata footer. GitHub-hosted workflows can inspect a post after it appears. For pre-post checks in supported local harnesses, see [post hooks](post-hooks.md).

## Harness-generated credit

A duplicate tool footer is an advisory finding for posts. Read the warning before removing text. Commit author and co-author checks still honor the repository's existing commit policy. Consult the configuration for the installed harness and distinguish CLI posts made with a person's account from posts made by a hosted app.

| Harness | Native attribution and pre-post integration |
| --- | --- |
| Claude Code | Set `attribution.commit` and `attribution.pr` to empty strings to suppress its generated commit trailer and PR footer. On versions supporting `attribution.sessionUrl`, set it to `false` to omit the additional cloud-session URL. Local `PreToolUse` can inspect tool input. [Attribution settings](https://code.claude.com/docs/en/settings-reference#attribution) · [Hooks](https://code.claude.com/docs/en/hooks) |
| Codex CLI | Gitmoji discovers the Codex session. In CLI 0.171.0, `codex features list` reports the historical `codex_git_commit` feature as removed. Use the project's instructions for generated prose and inspect the body being sent. PreToolUse support depends on the tool surface and client. [Hooks](https://developers.openai.com/codex/hooks) |
| Cursor | The CLI documents `attribution.attributeCommitsToAgent` and `attribution.attributePRsToAgent`. Set each to `false` to suppress its respective footer. Cursor documents pre-shell and pre-MCP hooks with its own JSON response format. [CLI configuration](https://cursor.com/docs/cli/reference/configuration) · [Hooks](https://cursor.com/docs/hooks) |
| GitHub Copilot | The CLI's `includeCoAuthoredBy: false` disables its commit credit. Cloud-agent PRs have GitHub-managed identity and activity. CLI settings apply to their documented surface. Repository pre-tool hooks are available on supported surfaces. [CLI settings](https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-config-dir-reference) · [Hooks](https://docs.github.com/en/copilot/concepts/customizations/hooks) |
| Gemini CLI | `BeforeTool` can reject shell or MCP tool calls. No commit/PR footer control was verified in the cited configuration. Inspect the actual proposed Markdown. [Configuration](https://geminicli.com/docs/reference/configuration/) · [Hook reference](https://geminicli.com/docs/hooks/reference/) |
| Antigravity | Gitmoji discovers its recorded session. Plugin `PreToolUse` returns a JSON decision on stdout with exit code 0. A nonzero hook exit is treated as an execution error; use a harness-specific adapter. A footer toggle was not verified. [Plugin hooks](https://antigravity.google/docs/build-plugins) |
| Devin | Devin CLI documents `PreToolUse` for local calls. A hosted Devin session and a post made through a local user's credentials have different integration paths. A cross-surface footer toggle was not verified. [CLI lifecycle hooks](https://docs.devin.ai/cli/extensibility/hooks/lifecycle-hooks.md) |
