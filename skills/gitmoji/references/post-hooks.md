# Check agent posts before they leave the harness

Enroll a project by adding this entry to `hooks.PreToolUse` in the harness settings. Claude Code reads `.claude/settings.json`. Codex CLI reads `.codex/hooks.json` in a trusted project. Merge the entry with any existing hooks. Make the project's pinned `gitmoji` binary available on the hook process's PATH and approve the installed hook in the harness. [Claude Code hooks](https://code.claude.com/docs/en/hooks#pretooluse) · [Codex hooks and trust](https://developers.openai.com/codex/hooks/)

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "^(Bash|mcp__github__.*)$",
        "hooks": [{"type": "command", "command": "gitmoji comment hook"}]
      }
    ]
  }
}
```

A matching fingerprint leaves the harness's normal permission checks in place. An unsigned or edited body blocks the tool call with exit code 2 and a message. An attribution warning on a correctly signed body is advisory. The hook works offline and sends nothing to GitHub. [Claude hook exit codes](https://code.claude.com/docs/en/hooks#exit-code-output) · [Codex PreToolUse](https://developers.openai.com/codex/hooks/#pretooluse)

## GitHub CLI

Sign the final Markdown into a regular file. Then make the publishing command a separate Bash tool call, using the **literal absolute path** to the completed file. This lets the hook inspect the same content across working directories.

```sh
gitmoji comment sign < review.md > /absolute/path/review.signed.md
```

```sh
gh pr comment 17 --body-file /absolute/path/review.signed.md
```

This covers direct `gh issue` and `gh pr` creation, edits and comments, and `gh pr review`. Metadata-only edits and bodyless approvals can proceed. For a closing or reopening explanation, publish a signed comment and then change the state. When using the GitHub REST API for an inline finding or thread reply, prepare one JSON request with the signed Markdown in `body` and use `gh api ... --input /absolute/path/request.json`. A review request can include `comments`; every inline body is checked. The hook accepts regular UTF-8 body and JSON files up to 1 MiB each. [GitHub CLI comments](https://cli.github.com/manual/gh_pr_comment) · [GitHub CLI API requests](https://cli.github.com/manual/gh_api)

## GitHub MCP

For the official MCP server named `github`, supply the signed Markdown directly in the structured `body` field. The hook checks `issue_write`, `create_pull_request`, `update_pull_request`, `add_issue_comment`, `update_issue_comment`, `pull_request_review_write`, `add_comment_to_pending_review`, and `add_reply_to_pull_request_comment`. Named text actions include inline findings and replies. Changes to labels, state, and reaction-only replies can proceed without a signed body. [GitHub MCP tool reference](https://github.com/github/github-mcp-server#tools)

## Coverage

The hook inspects the Bash and named MCP events delivered by the enrolled harness. Browser edits, other clients, shell scripts and aliases, GraphQL, and other MCP implementations have their own publishing paths. For those paths, check the raw outgoing Markdown with `gitmoji comment check`, or verify the body after publication with `gitmoji comment trace URL`. Use `gitmoji comment check --allow-human` for mixed human and agent text. A pre-tool hook checks the file at inspection time; a post-publication trace checks what GitHub actually stored.
