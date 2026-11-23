<!-- Generated from Gitmoji's vocabulary, CLI, examples and INSTALL.md. See docs/development.md. -->

# Gitmoji Triangle

Gitmoji helps people and coding agents record one decision per commit. When agents land changes faster than people can read every diff, the emoji pair shows the area and intent of each change. The issue number keeps related decisions together across pull requests.

## One issue, several decisions

**Traditional commit**

`feat(appeals): let applicants appeal a refused permit (#412)`

**With Gitmoji**

- [`🧩` / `👔🛡️`] Set the filing rules for appeals
  - 🧩👔 #412 (appeals): allow filing within 30 days of refusal
  - 🧩🛡️ #412 (appeals): require a refusal reference
- [`🎨` / `✨💬🐛`] Let applicants appeal from the portal
  - 🎨✨ #412 (portal): open an appeal from the refusal notice
  - 🎨💬 #412 (portal): explain which documents to attach
  - 👌 <`🎨🐛`> #412 (portal): keep the form open on the final day

Before writing the first PR, an agent can ask “How long can someone appeal?”
and “What must they supply?” Months later, a human scanning the same issue
can jump to 👔 for the deadline, 🛡️ for the guard, or 👌 for the fix prompted by review.

## How to combine the emojis

`<area><behaviour or intent>[modifier] <ticket> (<scope>): <summary>`

- **Area and second emoji.** Write the decision emojis together. An ordinary subject starts with that group. A review response starts with 👌 and encloses the group in literal angle brackets and backticks. First the area, then a behaviour or an intent. A lone second emoji is allowed when the area adds nothing. Projects can require the area with `lone_second = false` under `[rules]`.
- **Behaviour or intent.** A behaviour is a noun: the system now has a different one. An intent is a verb: what you did. If the commit changes what is promised, name the behaviour. Otherwise say what you did.
- **Modifier.** Append at most one (🧪 exploration, 🚧 provisional, 💥 breaking) and give the change a ticket.
- **Standalone marker.** 🔖 release and 🔺 convention revised each have their own subject with no ticket or scope.

The ticket ties the commits together. The scope names the part of the project that owns the decision.

## Special commit and pull request forms

**In response to review.** After 👌, the enclosed emojis identify the decision prompted by feedback. For written feedback, `Review` links to the reviewer’s request and an optional `Addresses` list names related comments.

- 👌 <`🧩🐛`> #42 (checkout): reserve the last item once

**Provisional work.** A 🚧 commit makes the open PR title provisional. The same issue tracks the demo through its later replacement.

- 🚧 [`🎨🎭`] Preview appeal decisions
  - 🎨🎭🚧 #425 (portal): show sample appeal decisions for the demo

A later 🏁 commit records the outcome on the same ticket.

- 🎨🏁 #425 (portal): replace sample decisions with the live appeal record

**Vocabulary upgrade.** When a repository changes its Gitmoji pin, record the adopted version in a standalone commit.

- 🔺 gitmoji `0.22.0`: check commits with the same vocabulary in CI and locally

**Automated job.** A job without a ticket puts an em dash in the ticket position.

- 🎨💄 — (tokens): take the new hues from Figma

## Vocabulary by lane

<details>
<summary>Browse the areas, behaviours and intents by group</summary>

| Lane | Group | Emojis and meanings |
| --- | --- | --- |
| Area | system | 🎨 **Experience** — The user's side of the coin: what people see, touch, read and feel, whether it is a feature or a quality such as speed or polish.<br>🧩 **Capability** — The other side of the coin: what the system does to make that experience possible. Services and logic.<br>🛰️ **Integration** — Connections to systems outside this repo, internal or external.<br>🗃️ **Data** — The running system's own schemas, migrations and stored data.<br>🧠 **AI** — Behaviour that was fitted, not written out: everything a model or agent does inside the system. Choose 🧠 when a model's output is what changes, 🗃️ when stored or derived data is.<br>🧱 **Infrastructure** — Cloud resources, containers and environments. |
| Area | workbench | 🧑‍💻 **Builder experience** — What the people who build the product work with, engineers, data scientists and product people alike: local setup, scripts, internal pages and explorers.<br>👷 **CI** — Pipelines, builds and checks.<br>🤖 **Agent toolkit** — What only coding agents work with: their instructions on how to work, memory, skills and their toolbelt. |
| Behaviour | domain logic | 👔 **Business rule** — An answer the domain gives: yes, no or how much. Eligibility, fees, limits. No time passes.<br>⛓️ **Process** — A sequence the domain wants: what happens next, and when. Waiting, retries, schedules, allowed status changes. |
| Behaviour | controls | 🎚️ **Switch** — A feature flag. The product side turns it, for everyone or for a subset. Often stays for the life of the product, because permissions are built on it.<br>🛂 **Permissions** — Who may do what: roles, policies and entitlements for people, built on switches, and the identities our own services run as.<br>⚙️ **Setting** — Part of the product's design: something each user chooses for themselves, such as the start screen or dark mode. A tenant admin's choices for their organisation count too.<br>🔧 **Configuration** — What engineers set, per environment or per case: how many pods, which endpoint, a model's temperature for one kind of question. |
| Behaviour | protection | 🛡️ **Validation guard** — Stops what should not come in, input or data, before acting on it. In the AI area it is a best-guess check and arrives with examples.<br>🥅 **Error handling** — What happens after a step has failed, and how the failure is reported back: how it recovers, the stable error code it carries, and the meaningful description people are shown. The codes are what tools such as Sentry or PostHog pick up later.<br>🔒️ **Security** — Definite controls against an attacker: encryption, authentication, patched vulnerabilities. Also privacy. |
| Behaviour | connections | 🏷️ **Contract** — An agreed shape: types, interfaces and API schemas between modules, or the schema of something stored.<br>🔌 **Wiring** — Connects existing pieces: a resolver, a route, a registration, navigation.<br>📤 **Sends out** — Something leaves our system: an email, a notification, a webhook call, an event on a queue, an exported file. |
| Behaviour | what it keeps | 💾 **State** — What it remembers, in memory or stored for good: fields, tables, caches, client stores.<br>🔐 **Secrets** — Repo and deployment secrets: added, rotated, removed. Not user key management. |
| Behaviour | from data | 🎭 **Synthetic data** — Made-up or anonymised records that are safe to keep in the repository: fixtures, seeds, evaluation sets, samples derived from production. Real data never enters the repository.<br>🧮 **Transformation** — Data made from other data: a dbt model, a cleaning or normalising step, a feature computed for a model.<br>🎯 **Objective** — What counts as a good result: a loss function, a metric, the scorer of an evaluation.<br>📊 **Report** — What the numbers show people: statistics, dashboards, the recorded scores of a run, a notebook rendered to PDF. Built to last, unlike a study. |
| Behaviour | looks | 💄 **Style** — Colours, typography, theme, design tokens.<br>📐 **Layout** — The arrangement itself changes: spacing, positioning, sizing, how things reflow. On any screen, including across a fold.<br>💫 **Delight** — A small moment that makes the product a pleasure to use: an animation, a transition, a touch of feedback people notice. Movement is the usual means, not the point.<br>🍱 **Assets** — Logos, images, icons, video, fonts. |
| Behaviour | feel | 🚸 **Interaction** — Touch, gestures, forms and the steps of a flow.<br>📱 **Fit** — Existing behaviour is made to work on a named device, screen class, browser or operating system. The arrangement and the steps stay as they are. |
| Behaviour | words | 💬 **Text** — Wording and literals.<br>🌐 **Language** — Making the product speak more than one language: looking messages up by key or code, the catalogue files translators work in, formats for dates and numbers, and the translations themselves.<br>📝 **Documentation** — Explains something to its readers: user guides, API docs, runbooks, contributor guides. It takes the area of what it explains; the summary names the reader when that matters. |
| Behaviour | quality | ☑️ **Test** — A check on one piece of our own work that passes or fails before it ships: unit, contract, snapshot. It takes the area of what it checks, not 👷.<br>✅ **Journey** — A user's story played through on the real thing: someone who has state, creates state and extends it, run in a simulator or a production-like environment. Written so it could become product documentation.<br>🕹️ **Playground** — A place to try one part of the system on its own, without building the whole product: a component library, a notebook page, a simulator, a device rig.<br>⚡️ **Speed** — Faster or lighter.<br>🔊 **Telemetry** — Signals the system emits: logs, metrics, traces, healthchecks. |
| Intent | add | 🆕 **New** — A new unit with a name and a job of its own, such as a service, a collector or a package, when no behaviour names it.<br>✨ **Feature** — Users can now do something they could not do before.<br>🏗️ **Scaffold** — Groundwork with no behaviour of its own: project setup, base components, helpers, empty views and routes. |
| Intent | repair | 🐛 **Fix** — It did the wrong thing; now it does the right thing.<br>🩹 **Accommodation** — Adapts our code to a limit in a dependency or tool. The body names the limit in a Limitation: line. It can be permanent and can be the right decision.<br>👽️ **Outside change** — It broke or had to change because something outside this repo moved.<br>⏪️ **Reverted** — Undoes earlier work by hand, such as a migration rollback or a downgrade. A commit made with git revert keeps the line git writes and needs no tag. |
| Intent | reshape | ♻️ **Refactor** — Restructuring inside a unit: moves, renames, internal architecture. No behaviour change. Often prepares an extraction.<br>✂️ **Extracted** — Split out into a self-standing unit with its own entry point.<br>🪢 **Folded in** — Merged into something that already exists: a duplicate into the original when it is the same thing and not merely similar, or an extracted unit back into its consumers. |
| Intent | remove | 🧹 **Hygiene** — Dead code, redundant tests, formatting. No design change.<br>🔥 **Remove** — Takes out something that was in use. |
| Intent | take in | ➕ **Dependency added** — A new dependency enters.<br>⬆️ **Dependency upgraded** — Upgraded or pinned.<br>➖ **Dependency removed** — A dependency leaves.<br>📦️ **Regenerated** — Generated files rebuilt from sources in this repo: code graphs, compiled output, generated types.<br>📥 **Imported** — Brought in from a run or system outside the repo. The commit body names the source. |
| Intent | close | 🏁 **Finished** — Closes what was left open and keeps the result: a trial adopted and its switch removed, a provisional version replaced, a migration done. It says what decided it.<br>⚰️ **Buried** — Closes what was left open and removes the result. It says what decided it.<br>🚀 **Rolled out** — A rollout step: a switch or a trial opened to more users, or a version promoted to an environment. |

</details>

## Install

```sh
installer=$(mktemp)
curl -fsSL https://github.com/D1no/gitmoji/releases/latest/download/install.sh -o "$installer" &&
  sh "$installer" /path/to/your/project
```

The installer downloads the binary for your platform and verifies its checksum. Signatures, PR titles, direct merges and history upgrades run in that binary. Repository operations use Git; hosted operations also use your GitHub credentials.
With a project path, it also installs the skill, settings and Git hooks. Run
`sh "$installer"` by itself to install only the command. The CLI, skill
and vocabulary use one release number. Run `gitmoji --version` to check it.

### Check a project installation

Run `gitmoji doctor` inside a checkout to check the active release, Git hooks, installed skill and enrolled workflows. For a session-start check, `gitmoji doctor --json` emits one array and exits with status 1 when a component needs attention. It works offline and leaves the checkout alone. [Diagnostic fields and examples](package/README.md#check-an-installation)

### Pin the CLI in CI

On GitHub Actions, run `gitmoji init --github-workflows` in the project to install the PR commit check and title sync. Commit the generated workflows. On upgrade, change `[vocabulary].version` and rerun `gitmoji init` to refresh managed copies. [Workflow setup](skills/gitmoji/references/pull-requests.md#enable-automatic-pr-titles)

For other CI systems, run the installer from the project checkout. It reads
`[vocabulary].version` from `gitmoji.toml` or `.config/gitmoji.toml` and installs
that release. The installer uses a local Gitmoji that supports `pin`, or downloads
a temporary reader. It verifies every downloaded binary against its own release’s
`SHA256SUMS`. Installing the CLI requires a POSIX shell, curl or wget, and
sha256sum or shasum. Git is needed only for subsequent repository operations.

```sh
tool=$(mktemp -d)
url=https://github.com/D1no/gitmoji/releases/latest/download/install.sh
curl -fsSL "$url" -o "$tool/install.sh" &&
  GITMOJI_BIN="$tool/bin" sh "$tool/install.sh" --pinned &&
  "$tool/bin/gitmoji" --version
```

With wget, use `wget -q -O "$tool/install.sh" "$url"` for the download. For a
fixed bootstrap, copy the installer URL from a specific release’s assets.
`GITMOJI_BIN` selects the destination directory. If another tool named
`gitmoji` is installed, the installer asks before replacing it. In CI the
collision stops installation unless you pass `--overwrite` or set
`GITMOJI_OVERWRITE=1`. Repeated installs verify the existing binary against the
release checksum and leave an identical executable in place.
Linux and macOS releases cover arm64 and x86_64.

## What Gitmoji can do

**Set up a project**

- `gitmoji init` — Set a git repository up: the agent skill, gitmoji.toml, the hooks, CLAUDE.md
- `gitmoji doctor` — Check the installed binary, release pin, hooks, skill, workflows and Dependabot prefixes
- `gitmoji pin` — Print the release pinned in the project configuration

**Plan and write commits**

- `gitmoji guide` — Read the guide: no topic for the start, or an emoji, a name, a group or a topic
- `gitmoji staged` — Look at the staged files and say when they look like more than one commit
- `gitmoji check` — Check a commit message. As a commit-msg hook: `gitmoji check --fix {1}`
- `gitmoji signature` — Print a commit signature from the active agent session

**Review and merge**

- `gitmoji pr-title` — Synchronize a GitHub pull request title from its incoming commits
- `gitmoji merge-caption` — Print the synchronized PR title for the authored commits arriving on its base
- `gitmoji merge-message` — Caption Git’s generated local branch merge message, or preview a stored merge
- `gitmoji provisional-pr` — Check a provisional PR’s follow-up issue or synchronize its PR body
- `gitmoji defer-findings` — Preview unresolved review threads, or transfer selected findings to their verified follow-up
- `gitmoji github-merge` — Preview or submit a verified direct merge on GitHub

**Read history and prepare a release**

- `gitmoji explain` — Read a subject or a run of emojis back in words
- `gitmoji log` — Show authored commits whose heading contains a vocabulary emoji, newest first
- `gitmoji history` — Check commits that are already made, and list the ones that would not pass
- `gitmoji open` — List provisional and exploration tickets still open in authored Git history
- `gitmoji changelog` — Draft Markdown release notes from the pull requests landed in a Git range

**Use the vocabulary in other tools**

- `gitmoji vocabulary` — Print the version of the vocabulary this command speaks
- `gitmoji export` — Print the vocabulary for other tools
- `gitmoji site` — Build the website from a template
- `gitmoji help` — Print this message or the help of the given subcommand(s)

## Pull request titles

Gitmoji updates the emoji prefix as commits arrive. The words remain editable. A landed merge adds the PR number at the end. [PR and merge setup](skills/gitmoji/references/pull-requests.md) · [Local branch merges](skills/gitmoji/references/pull-requests.md#local-branch-merges)

## Guides

[Authoring commits](skills/gitmoji/SKILL.md) · [History upgrades](skills/gitmoji/references/upgrade-history.md) · [CLI](package/README.md) · [Agent skill](skills/README.md) · [Contributing](AGENTS.md) · [Development](docs/development.md) · [Website](website/README.md) · [CI and releases](.github/ci-and-releases.md)

[Changelog](CHANGELOG.md) · [MIT license](LICENSE)
