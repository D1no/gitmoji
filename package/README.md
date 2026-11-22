# package: the gitmoji command

One static binary, written in Rust, that checks commit messages and explains the
vocabulary. Everything it knows about the convention comes from `vocabulary.toml`,
which is compiled in.

## Check an installation

Run `gitmoji doctor` anywhere inside the project’s Git worktree. Use `gitmoji doctor --json` for setup scripts. Every object includes five common string fields. For example, this row reports the running binary in a project pinned to the same version:

```json
{"component":"binary","status":"ok","found":"0.18.0","expected":"0.18.0","remedy":""}
```

| Field | Meaning |
| --- | --- |
| `component` | A stable name such as `binary`, `pin`, `hook:prepare-commit-msg`, `workflow:gitmoji-pr-title` or `skill`. Outside a working tree, the report contains a `repository` diagnostic. |
| `status` | `ok` and `not-enrolled` allow exit 0. Any `missing`, `mismatch`, `stale` or `bypassed` row yields exit 1. A temporary Lefthook bypass has its own status so callers can display it as a warning. |
| `found`, `expected` | The observed and expected state. Use the status for decisions and display these details to the user. |
| `remedy` | A suggested repair for the user to review. Doctor never applies it. |

Doctor checks Git’s effective hook paths, including the shared hook directory of a linked worktree and `core.hooksPath` overrides. For a Lefthook wrapper, it reads standalone YAML, JSON or TOML configuration, including `.config/lefthook.yml`, and checks the command for that particular hook. If a configuration needs a local override, a remote or an `extends` file to resolve it, the report flags it for inspection. It does not run hooks or contact a remote. Projects can use the ordinary Git hooks without installing Lefthook.

A valid pin that selects a different release produces a `binary` mismatch. That row carries the version in `expected` and the additional field `"install":"install.sh --pinned"`. Setup scripts can use those values to select their own installation command. Install the pinned binary and rerun the check to compare managed file contents against that release. A project can use Gitmoji without enrolling in either managed GitHub workflow.

## Commands

Run these in `package/`. CI checks formatting, lint and tests on Linux, macOS and
Windows. The release workflow also builds the optimized binary for each target.

```
cargo fmt -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
cargo build --release --locked        # target/release/gitmoji
```

## Draft release notes

Run `gitmoji changelog v1.0.0..v1.1.0` in a complete Git checkout. For a first release, `gitmoji changelog v1.0.0` includes history from the root through that tag. Branches and commit IDs work too. The command writes Markdown to standard output and leaves the working tree unchanged. Equal endpoints produce an empty draft. If the end predates the start along the same ancestry, the command reports a reversed range and exits with an error. Diverged branches use ordinary Git two-dot semantics.

For example, a PR changes the interface and its deployment. One of its commits marks a breaking change. The draft gives the release editor one line.

```markdown
## People-facing

- **💥 Breaking** — Replace the old export dialog (#42; #35, #38) — also: 🧱
```

The default sections are **People-facing** (🎨), **System** (🧩 🛰️ 🗃️ 🧠), and **Behind the scenes** (🧱 👷 🧑‍💻 🤖), in that order. To replace them, add ordered groups to `gitmoji.toml`.

```toml
[[changelog.groups]]
name = "For operators"
areas = ["🧱", "👷"]

[[changelog.groups]]
name = "For users"
areas = ["🎨", "🧩"]
```

A PR appears under the first group that matches an authored area. The line shows areas outside that group so an editor can split the story. PRs without a matching area appear under the reserved heading **Other changes**. The draft takes the PR number and human title from a recognized landing, then the tickets, areas and breaking flags from the arriving authored commits. It reads the complete headings even when the short PR inventory ends in an ellipsis. Tickets follow the project's configured patterns.

The range selects merge commits. Gitmoji recognizes its PR captions and GitHub's default merge subjects. Local branch merges can carry PRs into a release. Each PR appears once, and a PR integrated inside another keeps its own tickets and breaking flags. When direct, squash or rebased work appears on the first-parent release history, stderr gives the count and lists each omitted commit’s abbreviated Git ID and subject, oldest first. Standalone release and adoption markers are excluded from the count. Review that history and add any changes readers need. When the draft is empty, stderr explains why and stdout stays empty. Release-marker commits (`🔖`) contribute no entry, and a PR containing only those markers is omitted. Review the draft before publishing to combine related work and describe the change for your readers.

## Choosing policy for a commit

The pre-commit check (`gitmoji staged`) and commit-msg check read the settings staged for that commit. After `gitmoji init`, stage the generated settings to apply them. A staged policy change applies immediately, including on the first commit. You can keep editing the working copy without changing what either hook checks.

Both hooks look for `gitmoji.toml`, then `.config/gitmoji.toml`, then the legacy `triangle.toml` at the worktree root. A staged deletion exposes the next file in this order. If none is staged or already tracked in the proposed commit, the built-in defaults apply. Gitmoji follows tracked links to files inside the proposed tree. Linked worktrees and path-limited commits use the index Git selected for that commit.

For a local experiment, `gitmoji --config path/to/policy.toml staged` uses that file from disk. Checking text with `gitmoji check -m` or checking a separate draft also uses the working copy so you can try a policy before staging it.

## Find commits by emoji

Run `gitmoji log '⬆' trunk..HEAD` to see dependency upgrades on a branch, newest first. Omit the range to search everything reachable from HEAD. The filter accepts a single vocabulary emoji or its GitHub shortcode. It matches any slot in the authored heading, including an optional modifier or a standalone marker. For example, `gitmoji log '💥'` finds breaking changes across areas. Merge inventories and emoji mentioned only in prose do not add matches.

The log displays the vocabulary’s canonical glyphs. Searching for `⬆`, `⬆️` or `:arrow_up:` gives the same results, including recorded Dependabot job headings. Use `git show -s --format=%s <sha>` to inspect the original spelling in Git.

## Reviewing untagged commits during adoption

The hook normally rejects a missing intention. To allow ordinary prose while a team
adopts the convention, put this in the repository’s `gitmoji.toml`:

```toml
[rules]
untagged = "warn"
```

`warn` prints a note. `off` suppresses that note. In both modes a prose subject
beginning with a word or number can pass without a vocabulary stamp. A misplaced
ticket or an invalid emoji heading still gets its usual diagnostic. Author and
trailer checks remain in force. To return to strict checking, set `untagged = "error"`.

Run `gitmoji history trunk..HEAD` to list nonconforming commits before the branch
lands. History returns a failure for those commits even if the hook allowed them.
For private branch work, `git commit --amend` can reword the tip. In a linear
branch, use `git rebase -i trunk` and select `reword` to edit an older message.
Choose the actual base branch for either command. History inspection is read-only.

## Requiring signatures from agents

The settings installed by `gitmoji init` use this policy. Existing repositories can
select it in their settings:

```toml
[rules]
signature = "agents"
```

During `gitmoji check <file>` and `check --fix <file>`, ordinary commits from
Claude Code, Codex and Antigravity must include the model and harness lines.
The checker accepts a signature under either the draft underscore or the
stamped version line. A person's commit can have the version line alone.
Generated branch captions follow the project’s signature rule while the merge is pending. Release and adoption markers and other Git-generated messages pass without signatures.

Detection uses `CLAUDECODE=1` or a nonempty `CLAUDE_CODE_SESSION_ID`,
`CLAUDE_CODE_ENTRYPOINT`, `CODEX_THREAD_ID`, `CODEX_SESSION_ID` or
`ANTIGRAVITY_CONVERSATION_ID` in the committing process. A launcher that
clears its session markers leaves Gitmoji without that evidence.
`[agents].markers` matches names and email addresses for the separate
author/co-author rule. History has no record of the committing shell, so
`signature = "agents"` adds no retrospective signature requirement.
The existing `warn` and `error` settings check unsigned commits regardless of
the author's shell, including during history checks. Omitting the setting
keeps signature checking off.

## Merge captions

`gitmoji merge-caption --pr 42 --title 'Give reviewers a preview' BASE HEAD`
prints the live PR title from the authored commits arriving in `BASE..HEAD`.
Add `--json` for the PR `title`, landed `subject` ending in `(#42)`, the count
with recognized headings and the total number of incoming non-merge commits. Local replacement refs and grafts do
not change the inventory.
Use the [GitHub workflow](../skills/gitmoji/references/pull-requests.md) for
the subject and original branch information together. For an existing
repository, see [upgrading history](../skills/gitmoji/references/upgrade-history.md).

`gitmoji merge-message <message-file> [source]` is the prepare-commit-msg entry point for local branch merges. `gitmoji explain --commit HEAD` reads the stored parents and message to identify a local caption. [Branch merge setup and examples](../skills/gitmoji/references/pull-requests.md#local-branch-merges)

## Where things are

| Path | What it is |
|---|---|
| `vocabulary.toml` | The entries, rules and writing notes. The only place they are written down |
| `src/vocabulary.rs` | Reads the vocabulary |
| `src/config.rs` | A repository's settings, `gitmoji.toml` |
| `src/subject.rs` | Reads a first line and says what is wrong with it |
| `src/trailers.rs` | Reads the trailers that end a message |
| `src/check.rs` | Checks a whole message: description, closing block, author |
| `src/fix.rs` | What the hook rewrites: shortcodes, and the version line |
| `src/guide.rs` | The guide, a piece at a time |
| `src/staged.rs` | Looks at staged files |
| `src/history.rs` | Checks commits that are already made |
| `src/init.rs` | Sets a repository up: the skill, settings, hooks and instructions. The skill's files are compiled in from `../skills/gitmoji` |
| `examples/` | The settings and hook files `init` writes into a repository |
| `src/main.rs` | The command line |
| `tests/` | The command as a user runs it, and a story of 181 subjects |

## Rules for this folder

- **Change the vocabulary in `vocabulary.toml` only**, and regenerate the reference using the crate release version. Wording
  that an author reads must never say which patterns a reviewer looks for.
- **No `unwrap` or `expect` outside tests.** Errors say what to do next; the reader is
  often an agent in the middle of a commit.
- **One reading of a message.** `check::parts` decides which lines are the subject, the
  description, the closing block and the trailers. The check, the hook and `history`
  all use it; do not parse a message a second way.
- **The hook changes two things in a message and nothing else:** shortcodes in the
  first line, and the version line. Every other byte stays.
