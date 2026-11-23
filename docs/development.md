# Development: the environment, the editor and worktrees

What a contributor, person or agent, needs before the first change. The commands for
each area are in that area's README.

## Tools

| Tool | Why | Version |
|---|---|---|
| Rust | builds `package/` | pinned in `package/rust-toolchain.toml` |
| lefthook | runs the git hooks | 2.x |
| `gitmoji` | the hooks call it | a release, or `cargo install --path . --locked` from `package/` |
| Python 3 | the repository’s integration test drivers and documentation link check | verified with 3.12 |

After cloning: `lefthook install`. That is once per clone; every worktree of the clone
then uses the same hooks.

For website work, use Node 24 or later and the pnpm version pinned in
`website/package.json`. Run the workspace commands from `website/`. [Website setup](../website/README.md)

## Repository README

The root README is generated in full. Change the vocabulary in
[`package/vocabulary.toml`](../package/vocabulary.toml), command descriptions in the CLI,
installation instructions in [INSTALL.md](../INSTALL.md), or the
[checked examples and page layout](../package/examples/readme/). The examples use the
same subject checker and PR caption formatter as the released command.

The repository’s pre-commit hook refreshes the page from staged changes. CI
regenerates it independently and checks the committed bytes. If a path-limited
commit needs a new README, stage the intended changes and commit without a path
so Git includes the generated page. To preview a change
from the repository root, run:

```sh
cargo run --quiet --locked --manifest-path package/Cargo.toml --example readme -- --write
```

## Dev container

The dev container sets up the Rust command and Git hooks. Open the folder in an
editor that supports dev containers, or run
`npx @devcontainers/cli up --workspace-folder .`. Its setup builds `gitmoji` from the
checkout, so the hooks check commits with the repository's own current code.

Cargo build output is kept in a volume that belongs to the container.

## Editor workspace

Open `gitmoji.code-workspace`. It shows one root per area and recommends extensions for Rust, TOML,
EditorConfig and Biome. It enables format-on-save and hides the generated Cargo,
website and Storybook output from the file tree.

## Worktrees

Several worktrees of one clone can be built, tested and committed in side by side.

- The branch everything lands on is `trunk`.
- Make each with a branch of its own: `git worktree add -b <branch> ../<folder>`.
- Hooks are shared across worktrees. Each checkout gets its own Cargo `target/`, pnpm
  `node_modules/` links, and website output in `dist/` and `storybook-static/`.
- Each website server needs a free port when several worktrees are running.
  [Run the landing page alongside another checkout](../website/landingpage/README.md)
- A dev container opened on a linked worktree cannot see the clone's git data, which
  lives outside the folder it mounts. Open the main checkout in the container, or use
  the worktree on the host.
- Agree who commits what before working in parallel. One committer per area is less
  trouble than merging many branches.
