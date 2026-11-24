<!-- Generated from Gitmoji's vocabulary, CLI, examples and INSTALL.md. See docs/development.md. -->

# Gitmoji Triangle

Gitmoji helps people and coding agents record one decision per commit. When agents land changes faster than people can read every diff, the first emoji shows the area and the second identifies the behavior or intent. The issue number keeps related decisions together across pull requests.

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

### Code review

👌 marks a response to feedback. The enclosed emojis identify what the author changed. For written feedback, `Review` links to the reviewer’s request and an optional `Addresses` list names related comments.

- 👌 <`🧩🐛`> #42 (checkout): reserve the last item once

### Provisional work

A 🚧 commit makes the open PR title provisional. The same issue tracks the demo through its later replacement.

- 🚧 [`🎨🎭`] Preview appeal decisions
  - 🎨🎭🚧 #425 (portal): show sample appeal decisions for the demo

A later 🏁 commit records the outcome on the same ticket.

- 🎨🏁 #425 (portal): replace sample decisions with the live appeal record

### Vocabulary upgrade

When a repository changes its Gitmoji pin, record the adopted version in a standalone commit.

- 🔺 gitmoji `0.24.0`: check commits with the same vocabulary in CI and locally

### Automated job

A job without a ticket puts an em dash in the ticket position.

- 🎨💄 — (tokens): take the new hues from Figma

## Vocabulary by lane

Start with **one Area** from the left column. Then choose **one Behavior** from the middle column or **one Intent** from the right. For example, 🧩👔 pairs Capability with Business rule, and 🧩♻️ pairs Capability with Refactor.

<table>
<thead><tr><th>Area</th><th>Behavior</th><th>Intent</th></tr></thead>
<tbody><tr>
<td valign="top" width="33%">
<h3>system</h3>
<p>🎨 <strong>Experience</strong> — The user's side of the coin: what people see, touch, read and feel, whether it is a feature or a quality such as speed or polish.</p>
<p>🧩 <strong>Capability</strong> — The other side of the coin: what the system does to make that experience possible. Services and logic.</p>
<p>🛰️ <strong>Integration</strong> — Connections to systems outside this repo, internal or external.</p>
<p>🗃️ <strong>Data</strong> — The running system's own schemas, migrations and stored data.</p>
<p>🧠 <strong>AI</strong> — Behaviour that was fitted, not written out: everything a model or agent does inside the system. Choose 🧠 when a model's output is what changes, 🗃️ when stored or derived data is.</p>
<p>🧱 <strong>Infrastructure</strong> — Cloud resources, containers and environments.</p>
<h3>workbench</h3>
<p>🧑‍💻 <strong>Builder experience</strong> — What the people who build the product work with, engineers, data scientists and product people alike: local setup, scripts, internal pages and explorers.</p>
<p>👷 <strong>CI</strong> — Pipelines, builds and checks.</p>
<p>🤖 <strong>Agent toolkit</strong> — What only coding agents work with: their instructions on how to work, memory, skills and their toolbelt.</p>
</td>
<td valign="top" width="33%">
<h3>domain logic</h3>
<p>👔 <strong>Business rule</strong> — An answer the domain gives: yes, no or how much. Eligibility, fees, limits. No time passes.</p>
<p>⛓️ <strong>Process</strong> — A sequence the domain wants: what happens next, and when. Waiting, retries, schedules, allowed status changes.</p>
<h3>controls</h3>
<p>🎚️ <strong>Switch</strong> — A feature flag. The product side turns it, for everyone or for a subset. Often stays for the life of the product, because permissions are built on it.</p>
<p>🛂 <strong>Permissions</strong> — Who may do what: roles, policies and entitlements for people, built on switches, and the identities our own services run as.</p>
<p>⚙️ <strong>Setting</strong> — Part of the product's design: something each user chooses for themselves, such as the start screen or dark mode. A tenant admin's choices for their organisation count too.</p>
<p>🔧 <strong>Configuration</strong> — What engineers set, per environment or per case: how many pods, which endpoint, a model's temperature for one kind of question.</p>
<h3>protection</h3>
<p>🛡️ <strong>Validation guard</strong> — Stops what should not come in, input or data, before acting on it. In the AI area it is a best-guess check and arrives with examples.</p>
<p>🥅 <strong>Error handling</strong> — What happens after a step has failed, and how the failure is reported back: how it recovers, the stable error code it carries, and the meaningful description people are shown. The codes are what tools such as Sentry or PostHog pick up later.</p>
<p>🔒️ <strong>Security</strong> — Definite controls against an attacker: encryption, authentication, patched vulnerabilities. Also privacy.</p>
<h3>connections</h3>
<p>🏷️ <strong>Contract</strong> — An agreed shape: types, interfaces and API schemas between modules, or the schema of something stored.</p>
<p>🔌 <strong>Wiring</strong> — Connects existing pieces: a resolver, a route, a registration, navigation.</p>
<p>📤 <strong>Sends out</strong> — Something leaves our system: an email, a notification, a webhook call, an event on a queue, an exported file.</p>
<h3>what it keeps</h3>
<p>💾 <strong>State</strong> — What it remembers, in memory or stored for good: fields, tables, caches, client stores.</p>
<p>🔐 <strong>Secrets</strong> — Repo and deployment secrets: added, rotated, removed. Not user key management.</p>
<h3>from data</h3>
<p>🎭 <strong>Synthetic data</strong> — Made-up or anonymised records that are safe to keep in the repository: fixtures, seeds, evaluation sets, samples derived from production. Real data never enters the repository.</p>
<p>🧮 <strong>Transformation</strong> — Data made from other data: a dbt model, a cleaning or normalising step, a feature computed for a model.</p>
<p>🎯 <strong>Objective</strong> — What counts as a good result: a loss function, a metric, the scorer of an evaluation.</p>
<p>📊 <strong>Report</strong> — What the numbers show people: statistics, dashboards, the recorded scores of a run, a notebook rendered to PDF. Built to last, unlike a study.</p>
<h3>looks</h3>
<p>💄 <strong>Style</strong> — Colours, typography, theme, design tokens.</p>
<p>📐 <strong>Layout</strong> — The arrangement itself changes: spacing, positioning, sizing, how things reflow. On any screen, including across a fold.</p>
<p>💫 <strong>Delight</strong> — A small moment that makes the product a pleasure to use: an animation, a transition, a touch of feedback people notice. Movement is the usual means, not the point.</p>
<p>🍱 <strong>Assets</strong> — Logos, images, icons, video, fonts.</p>
<h3>feel</h3>
<p>🚸 <strong>Interaction</strong> — Touch, gestures, forms and the steps of a flow.</p>
<p>📱 <strong>Fit</strong> — Existing behaviour is made to work on a named device, screen class, browser or operating system. The arrangement and the steps stay as they are.</p>
<h3>words</h3>
<p>💬 <strong>Text</strong> — Wording and literals.</p>
<p>🌐 <strong>Language</strong> — Making the product speak more than one language: looking messages up by key or code, the catalogue files translators work in, formats for dates and numbers, and the translations themselves.</p>
<p>📝 <strong>Documentation</strong> — Explains something to its readers: user guides, API docs, runbooks, contributor guides. It takes the area of what it explains; the summary names the reader when that matters.</p>
<h3>quality</h3>
<p>☑️ <strong>Test</strong> — A check on one piece of our own work that passes or fails before it ships: unit, contract, snapshot. It takes the area of what it checks, not 👷.</p>
<p>✅ <strong>Journey</strong> — A user's story played through on the real thing: someone who has state, creates state and extends it, run in a simulator or a production-like environment. Written so it could become product documentation.</p>
<p>🕹️ <strong>Playground</strong> — A place to try one part of the system on its own, without building the whole product: a component library, a notebook page, a simulator, a device rig.</p>
<p>⚡️ <strong>Speed</strong> — Faster or lighter.</p>
<p>🔊 <strong>Telemetry</strong> — Signals the system emits: logs, metrics, traces, healthchecks.</p>
</td>
<td valign="top" width="33%">
<h3>add</h3>
<p>🆕 <strong>New</strong> — A new unit with a name and a job of its own, such as a service, a collector or a package, when no behaviour names it.</p>
<p>✨ <strong>Feature</strong> — Users can now do something they could not do before.</p>
<p>🏗️ <strong>Scaffold</strong> — Groundwork with no behaviour of its own: project setup, base components, helpers, empty views and routes.</p>
<h3>repair</h3>
<p>🐛 <strong>Fix</strong> — It did the wrong thing; now it does the right thing.</p>
<p>🩹 <strong>Accommodation</strong> — Adapts our code to a limit in a dependency or tool. The body names the limit in a Limitation: line. It can be permanent and can be the right decision.</p>
<p>👽️ <strong>Outside change</strong> — It broke or had to change because something outside this repo moved.</p>
<p>⏪️ <strong>Reverted</strong> — Undoes earlier work by hand, such as a migration rollback or a downgrade. A commit made with git revert keeps the line git writes and needs no tag.</p>
<h3>reshape</h3>
<p>♻️ <strong>Refactor</strong> — Restructuring inside a unit: moves, renames, internal architecture. No behaviour change. Often prepares an extraction.</p>
<p>✂️ <strong>Extracted</strong> — Split out into a self-standing unit with its own entry point.</p>
<p>🪢 <strong>Folded in</strong> — Merged into something that already exists: a duplicate into the original when it is the same thing and not merely similar, or an extracted unit back into its consumers.</p>
<p>👌 <strong>Code review</strong> (prefix) — Wrap the decision prompted by feedback. Use 👌 &lt;<code>🧩♻️</code>&gt; for a refactor. <a href="#code-review">See a bug-fix example</a>.</p>
<h3>remove</h3>
<p>🧹 <strong>Hygiene</strong> — Dead code, redundant tests, formatting. No design change.</p>
<p>🔥 <strong>Remove</strong> — Takes out something that was in use.</p>
<h3>take in</h3>
<p>➕ <strong>Dependency added</strong> — A new dependency enters.</p>
<p>⬆️ <strong>Dependency upgraded</strong> — Upgraded or pinned.</p>
<p>➖ <strong>Dependency removed</strong> — A dependency leaves.</p>
<p>📦️ <strong>Regenerated</strong> — Generated files rebuilt from sources in this repo: code graphs, compiled output, generated types.</p>
<p>📥 <strong>Imported</strong> — Brought in from a run or system outside the repo. The commit body names the source.</p>
<h3>close</h3>
<p>🏁 <strong>Finished</strong> — Closes what was left open and keeps the result: a trial adopted and its switch removed, a provisional version replaced, a migration done. It says what decided it.</p>
<p>⚰️ <strong>Buried</strong> — Closes what was left open and removes the result. It says what decided it.</p>
<p>🚀 <strong>Rolled out</strong> — A rollout step: a switch or a trial opened to more users, or a version promoted to an environment.</p>
</td>
</tr></tbody>
</table>

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

- `gitmoji comment` — Mark agent-written GitHub posts, check their fingerprints, and trace their source
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
