# package: the gitmoji command

One static binary, written in Rust, that checks commit messages and explains the
vocabulary. Everything it knows about the convention comes from `vocabulary.toml`,
which is compiled in.

## Commands

Run these in `package/`. CI checks formatting, lint and tests on Linux, macOS and
Windows. The release workflow also builds the optimized binary for each target.

```
cargo fmt -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
cargo build --release --locked        # target/release/gitmoji
```

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

## Merge captions

`gitmoji merge-caption --pr 42 --title 'Give reviewers a preview' BASE HEAD`
prints the live PR title from the authored commits arriving in `BASE..HEAD`.
Add `--json` for the PR `title`, landed `subject` ending in `(#42)`, the count
with recognized headings and the total number of incoming non-merge commits. Local replacement refs and grafts do
not change the inventory.
Use the [GitHub workflow](../skills/gitmoji/references/pull-requests.md) for
the subject and original branch information together. For an existing
repository, see [upgrading history](../skills/gitmoji/references/upgrade-history.md).

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
