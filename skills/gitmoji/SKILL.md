---
name: gitmoji
description: Write git commits in the Gitmoji Triangle convention (emoji pair, ticket, scope, summary) and split staged work into one decision per commit. Use before planning the commits for a ticket, before every `git commit`, when the `gitmoji` commit-msg hook refuses a subject, when landing a pull request, and when reviewing generated messages in older history.
compatibility: Needs the gitmoji command on PATH (https://github.com/D1no/gitmoji). The signature scripts need python3.
---

# Triangle commits

A subject looks like this:

```
<area><behaviour or intent>[modifier] <ticket> (<scope>): <summary>
🧩👔 #412 (server): allow an appeal within 30 days of refusal
🎨💄 — (tokens): take the new hues from Figma
```

The emojis are the first characters of the line, with nothing around them. The last
example is a commit made by a job: it has no ticket, so it carries an em dash.

The vocabulary is large on purpose, and you do not need to hold it in your head. The
`gitmoji` command hands it out in small pieces. Read only the piece you need.

## The command

Use the `gitmoji` command. If it is not on PATH, read `references/vocabulary.txt` in this skill's folder
for the whole vocabulary, and tell the person that the command is missing.

| When | Run |
|---|---|
| You need the list of areas | `gitmoji guide areas` |
| You know the area | `gitmoji guide 🧩` (any area emoji or name) |
| Nothing in the area guide fits | `gitmoji guide groups`, then one group: `gitmoji guide "repair"` |
| Two entries both seem to fit | `gitmoji guide 🛡️`, or the group: `gitmoji guide "protection"` |
| The work is a trial, a stopgap or breaking | `gitmoji guide modifiers` |
| You need the scope names | `gitmoji guide scopes` |
| You are about to write the summary or the body | `gitmoji guide message` |
| Before `git commit` | `gitmoji staged`, then `gitmoji check -m '<subject>'` for the first line, or `gitmoji check <file>` for a whole message |
| You typed shortcodes such as `:jigsaw::necktie:` | Nothing: the hook writes the emojis for you |
| You are undoing a commit | `git revert <commit>`; git's own first line stays as it is |
| The hook refused a subject | Read its message; it names the fix |
| You want to look over the commits of your branch before it lands | `gitmoji history trunk..HEAD` (the branch it will land on, then yours) |
| The hook says the repository adopted another version | Read what changed at the address it gives, and tell the user before you change the settings |

## Before you write code: plan the commits

Read the ticket and write the subjects you expect to make, as a list. Each one is a
decision a reviewer could accept on its own, and each one is something the ticket asks
for. The scope of each is the package that owns the decision (`gitmoji guide scopes`).

These are prompts, not a stack to fill. Plan a commit only where the ticket gives the
question an answer:

- What does the business decide (`👔`), and what happens over time (`⛓️`)?
- What is refused before a step is tried (`🛡️`)? When a step fails, how does it recover,
  which error code does it report, and what are people told (`🥅`)?
- What shape is agreed between pieces (`🏷️`), and what is stored (`💾`)?
- What do users see and do, and on which commit do they first gain it (`✨`)?
- How will we know a user gets through it on the real product (`✅`, its own commit, added
  once the product can be run that way)?

When the journey cannot be run yet, ship the feature and say on the ticket that its `✅`
is outstanding. Do not put `✅` on a smaller check to fill the gap; that is `☑️`.

Check for an existing owner before you plan a new endpoint, store or policy. If a
product decision is missing and would change what ships, write the question down and
ask. Do not settle it inside a commit.

A feature that is one decision, with its only caller and the checks it needs, is one
commit. Plan more lines only where the ticket holds more than one decision a reviewer
could take separately. A line the ticket never asked for is too fine.

## Before each commit: four questions

1. **Which area is the change part of, or about?** A test, a document and a playground
   take the area of the thing they are about.
2. **What is different now, or what did you do?** If the commit changes what is promised,
   name the behaviour (a noun). Otherwise name the intent (a verb).
3. **Is the work not settled?** Only then add a third emoji: `🧪` a trial, `🚧` a stopgap,
   `💥` consumers must adapt. A trial keeps `🧪` on every commit until `🏁` (kept) or
   `⚰️` (dropped), and nothing settled may depend on it before that. Building the
   direction a ticket has chosen is not a trial.
4. **Which ticket?** Use the repository's ticket format. Only a job writes `—`.

Then run `gitmoji check -m '<subject>'`.

Before you stage, read the change as someone who was not there. Where the reason cannot
be seen from the code, write a comment beside the code: why it is the way it is, what
must not be changed, where a value comes from. Say why, not what the line does. A
comment costs you almost nothing and saves the next reader, a person, a junior or an
agent without your context, from guessing.

## When one pair does not fit

That is the signal to look again, not to pick the nearest tag.

- When two labels fit, name what the reviewer could accept now and what they could
  defer. Split when those two decisions can differ and what is kept still works on its
  own. Stage by path or by hunk (`git add -p`).
- Keep the only caller, its wiring and the refusal checks it needs together when none of
  them has a promise of its own. Tag the commit by its point; the rest rides along.
- `gitmoji staged` reports two scopes: ask the same question. If the change only makes
  sense whole, such as one shape mirrored in two languages, keep it together and name the
  package that owns the decision.
- Parts that are forced by the main change ride along: its types, its test, a story, a lockfile.

Each commit must still build and pass on its own. If splitting would break that, keep
the forced parts together and say so in the body.

## Writing it

- **Summary:** what the system does now, in the words of the domain, starting with a verb
  in lower case. The whole first line fits in 140 characters, emojis, ticket and scope
  included, and shorter is better; `gitmoji check -m '<subject>'` counts it for you
  before you commit. No file names, no ticket numbers, no "how".
- **Description:** every commit you write has one, and the hook refuses a commit without
  it. Only a marker (`🔖`, `🔺`), a commit made by a job and a message git wrote need none. Under
  the first line, an empty line, then why the change was made: the reason, why this way
  and not the obvious one, or the value you chose. One or two lines are usually enough;
  six are the most. Keep ticket references inside the sentence (for example, “See #42 for the decision”).
  Git can strip a line starting with `#42` when someone edits the commit. Do not retell the diff, list files or describe your process.
- **Emojis, not shortcodes.** Only the characters are committed.

## Who wrote it

**Never add yourself as author or co-author.** No `Co-Authored-By:` line, no agent
identity in `git config user.*` or `--author`. The author is the person responsible, and
the hook refuses an agent there. Some tools add a co-author line for themselves without
being asked; when the hook refuses a commit for that, take the line out and commit again.

Instead, end the body with an empty line and a signature that says what went into the
commit:

```
_
example-model (high), editor
Claude Code (example-session)
Sub-agents: 2 × example-helper (medium)
Team: reviewer, tester
Research: 3 searches, 12 pages
```

The first three lines are always there. The last three appear only when they apply.
When the commit is made, the hook replaces the underscore by `△` and, between backticks,
the version of the vocabulary the message was checked against, so a stored signature looks like this:

```
△ `1.2.3`
example-model (high), editor
Claude Code (example-session)
```

Write the underscore; never write the `△` line yourself. The hook closes every message
with it, also a person's, which then has that one line and no signature.
`gitmoji history` reads it to tell which commits are on which version.

Projects can set `signature = "agents"` under `[rules]` to require the model and harness lines when a supported agent session runs the commit hook. A human can commit with the ordinary description. A captioned local merge follows the project's signature policy. Release markers and the other recognized Git operations keep their exemptions.

| Line | What it says |
|---|---|
| `_` | The signature starts here. The hook turns it into ``△ `<version>` ``. |
| model line | The model and its effort. After a comma, your name, only if a person gave you one. |
| harness line | The harness and, in parentheses, the session id, if there is one. |
| `Sub-agents:` | How many sub-agents did part of this commit's work, by model and effort. |
| `Team:` | The other agents you exchanged messages with for this commit. |
| `Research:` | Web searches, pages read and documentation lookups for this commit. |

**Do not write any of this from memory.** Run the script for your harness in `scripts/`
in this skill's folder and paste what it prints. The path is relative to the folder this
file is in, wherever the skill is installed:

```
python3 scripts/claude-code.py
```

In Claude Code that folder is `${CLAUDE_SKILL_DIR}`. The script reads the harness's own
record of the session. If your harness has no script there, `scripts/README.md` says how
to work one out once and add it. Where no record can be found, the script prints
`unknown`. A repository may ask you to leave a line out, such as the session id; its own
instructions say so.

If the session spans several tasks or repositories, check the optional activity lines
against this commit. Keep only the contributions you can attribute to it.

## Lines the body may carry

- `🩹` needs `Limitation: <what the dependency cannot do>`.
- `📥` needs `Source: <the run, tool or system it came from>`.
- `🏁` and `⚰️` say what decided it.
- `🐛` may name the cause: `Broken-by: <sha>`.

## When upgrading Gitmoji

The CLI, skill and vocabulary share a release number. The hook warns when the repository's settings name another version than the command
speaks. Tell the user, and read what changed at the address the warning gives. When the
user wants the new version, set it under `[vocabulary]` in the settings and commit that
alone, with the version between backticks:

```
🔺 gitmoji `<version>`: <what changes for this repository>
```

Do not rewrite a history that is already shared unless the user asks for it: commits
made under earlier rules may stay as they are. `gitmoji history --unmigrated` shows
which of them would not pass today. Run `gitmoji init` after changing the pin to refresh the installed skill and managed workflows.

## Landing pull requests and upgrading history

Enable the [PR title workflow](references/pull-requests.md#enable-automatic-pr-titles)
and select **Pull request title and description** as the default merge message.
Write the human title. Gitmoji maintains its emoji prefix as the commits change.
Use **Create a merge commit** with the suggested message. GitHub preserves the authored commits and appends the integrating PR number.

For an existing history, use the [upgrade guide](references/upgrade-history.md)
and the installed `scripts/gitmoji-upgrade-history.py`. Inspect the dry run
before creating a local candidate. Links require evidence of which PR landed.
A shared-branch replacement is a separate decision for the repository owners.

## Messages Git writes

Update a contributor branch locally with `git fetch origin` followed by
`git merge origin/<base>`. The prepare-commit-msg hook captions Git’s default
branch merge message with the incoming intentions, source and destination.
GitHub’s **Update branch** button runs on GitHub and bypasses local hooks.
For an agent-signed merge, use `git merge --edit origin/<base>` and append
the usual signature in the editor. A conflicted merge can be signed when you
finish it with `git commit`. See [local merge captions](references/pull-requests.md#local-branch-merges)
for hook setup, examples and reading back the arriving commits.

`gitmoji history` identifies a stored merge by its parents. Use
`gitmoji explain --commit HEAD` to inspect a stored merge.
A text-only `check -m` has only the subject to work with. It recognizes the
usual Git branch, tag and commit merge headings, GitHub pull request merge headings,
numbered `Merge #N` captions, quoted Git revert and reapply subjects, numbered
`Revert #N` and `Reapply #N` captions with a title, Git’s default
squash log heading and the autosquash prefixes `fixup! `, `squash! ` and `amend! `.

A squash merge with a custom PR title produces an ordinary one-parent commit.
Cherry-picks and rebases can also carry forward ordinary subjects. Check those
like other authored commits. A PR number on its own does not identify a merge.

During adoption, a project can set `[rules]` → `untagged = "warn"` in
`gitmoji.toml` to accept plain subjects and flag them for review. The default
is `"error"`. With `"off"`, the missing tag is accepted quietly. Other checks
still apply. Use `gitmoji history trunk..HEAD` on your branch to find subjects
to reword before sharing. History always checks untagged subjects strictly.

## When nothing fits

Do not bend an entry. Use the nearest honest one, add a body line
`Miss: <what you wanted to say>`, and tell the user. Misses are how the vocabulary gets fixed.
