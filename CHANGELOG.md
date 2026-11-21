# Changelog

The CLI, skill, and bundled vocabulary share one release number.

## 0.13.3 (21 November 2026)

- The checker and commit hook now flag a description line that starts with an issue number, such as `#42`. Git can strip that line during an amend or reword. Start the line with a word, for example `See #42 for the decision.`
- The rule applies to new drafts across cleanup modes. Stored history keeps its existing validation behavior. Projects can adopt this patch through their usual version pin.

## 0.13.2 (21 November 2026)

- Install Gitmoji in CI using the public HTTPS clone and checksum-verified release assets.
  A GitHub token is optional. See the [pinned CI install](README.md#pin-the-cli-in-ci).
- The public distribution also includes compatibility builds for projects pinning
  0.13.0 or 0.13.1. Use the corresponding public checksums if your build pins binary digests.

## 0.13.1 (20 November 2026)

- Keep the history helper focused on PR merge and undo captions.
- Retry private release downloads quietly when a GitHub CLI login is available.

## 0.13.0 (20 November 2026)

- Check commit intentions with the CLI and install the agent skill and hooks into a project.
- Preview pull request captions and review a local candidate before updating existing history.
