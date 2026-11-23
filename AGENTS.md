# Contributing to Gitmoji Triangle

Gitmoji Triangle includes the CLI, commit vocabulary, agent skill and website.
Start with the guide for the part of the project you are changing.

| Area | Guide |
| --- | --- |
| Rust CLI and vocabulary | [Package](package/README.md) |
| Agent skill and integrations | [Skills](skills/README.md) |
| Website | [Landing page](website/README.md) |
| Contributor setup and worktrees | [Development](docs/development.md) |
| Checks and publication | [CI and releases](.github/ci-and-releases.md) |

## Commits

- Use the [commit convention](skills/gitmoji/SKILL.md) for one decision per
  commit. Reference a repository issue and explain why the change belongs in
  the project.
- The author is the person responsible for the commit. Agents use the closing
  signature described in the skill. For this public repository, use the model,
  effort and harness. Omit personal names, workspace paths, local session IDs
  and private team identifiers from agent signatures.
- Keep private working notes, credentials and machine-specific configuration
  outside tracked files. Put durable contributor guidance in the relevant guide.
- Update links, examples and instructions alongside the changes they describe.
- Land changes on `trunk`. The maintainer publishes tested versions and verifies
  a fresh installation. [Release steps](.github/ci-and-releases.md#releases-workflowsreleaseyml)
