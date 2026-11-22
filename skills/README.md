# skills: what agents load before they commit

One skill, `gitmoji/`, in the layout the Agent Skills specification describes: the
folder is named like the skill, `SKILL.md` is the entry, and everything else is read
only when the skill points to it.

| Path | What it is |
|---|---|
| `gitmoji/SKILL.md` | When to ask the command for which piece, how to plan, write and sign |
| `gitmoji/references/vocabulary.txt` | The whole guide, for when the command is not installed. Generated |
| `gitmoji/references/pull-requests.md`, `upgrade-history.md` | Setup, landing and retrospective history upgrades |

## Rules for this folder

- **`references/vocabulary.txt` is generated.** After a change to the vocabulary, run
  `cargo run --manifest-path package/Cargo.toml -- export text > skills/gitmoji/references/vocabulary.txt`
  from the root. A test in `package/` fails when it is out of date.
- **The skill must work wherever it is installed.** Paths in `SKILL.md` are relative to
  the skill's own folder, never to a repository.
- **Text an author reads never says which patterns a reviewer looks for.**
- **Keep `SKILL.md` under 500 lines**, and one level of files beneath it.

## How it reaches an agent

- In this repository, `.claude/skills/gitmoji` and `.agents/skills/gitmoji` are links
  to `skills/gitmoji`, so the agents working here use the copy being edited.
- Elsewhere: `gitmoji init` writes it into a project; `npx skills add D1no/gitmoji`
  installs it for many agents; `.claude-plugin/marketplace.json` offers the same folder
  as a Claude Code plugin.
