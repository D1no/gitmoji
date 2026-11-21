# .github: checks and releases

## CI (`workflows/ci.yml`)

Runs on every push to `trunk` and on every pull request.

| Job | What it proves | What it does not prove |
|---|---|---|
| `package` on Linux, macOS, Windows | The command is formatted, lints clean and passes its tests on each system | That a released binary works: nothing is packaged here |
| `coverage and scenarios` | Reports line coverage of the command, the number of tests of each kind, and the size of the scenario story. The report is on the run's summary page, the raw data an artifact | That covered code is right: coverage counts lines run, not behaviour checked. It sets no minimum yet |
| `website` | The pnpm workspace passes its type and formatting checks and builds the static page | Its appearance in a browser |
| `pointers` | Every relative link between the guides resolves, and every path in the map of `AGENTS.md` exists or names the issue that brings it | That a guide says the right thing |
| `commit messages` | Every commit of the repository passes `gitmoji history`, with the command built from the same checkout | That a commit holds one decision: that is a reader's judgement |

The [package guide](../package/README.md) and [website guide](../website/README.md)
give the local checks for each area. From the repository root, run
`python3 .github/scripts/pointers.py` to check documentation links. With the compiled command on PATH, run `python3 -m unittest discover -s package/tests/python`
for the GitHub merge and history-upgrade helpers. To check commit
history with this checkout's command, build it with `cargo build --locked` in
`package/`, then run `package/target/debug/gitmoji history` from the root.

## Incoming commit checks

The [commit check workflow](workflows/gitmoji-commits.yml) validates the commits introduced by a pull request, from its base SHA to its head SHA. Open the **gitmoji-commits** job under **Gitmoji commit checks** to see which commit failed and why. Correct that commit on the PR branch and push the revised history. The check runs again.

When replacing an existing required check, let this job run on a PR first. Select its reported check name in the target branch’s rules and retire the former job after the new requirement is active. The [downstream setup guide](../skills/gitmoji/references/pull-requests.md#enable-automatic-pr-titles) covers project settings and installation.

## Pull request titles

The [title workflow](workflows/gitmoji-pr-title.yml) refreshes the emoji prefix when a PR opens, reopens, receives commits or is edited. To rename a PR, edit its human title normally. Use **Create a merge commit** with the suggested message to keep the authored commits and the PR link.

A failed **gitmoji-pr-title** check contains the reason the title could not be updated. After correcting the title, repository permissions or runner access, rerun that check. The [setup guide](../skills/gitmoji/references/pull-requests.md#enable-automatic-pr-titles) lists the workflow permissions and merge settings.

## Releases (`workflows/release.yml`)

The maintainer owns publication of each ready tool version. Push the reviewed
changes to this repository, wait for CI on the exact `trunk` commit, then tag
that commit. Follow the tag build through publication and verify a fresh install
before pointing downstream projects at the release.

1. Write the entry in `CHANGELOG.md`: `## <version> (<date>)`,
   what changed, and what a project that already uses the convention has to do.
2. Raise `version` in `package/Cargo.toml` and run `cargo check` in `package/` to refresh
   its lockfile. The CLI and vocabulary take their number from this file. For every new version,
   [regenerate the skill reference](../skills/README.md). Commit the updated files and
   wait for CI on `trunk`.
3. Tag the tested commit with `git tag -a v<version> <tested-sha> -m "gitmoji <version>"`
   and push it with `git push origin refs/tags/v<version>`.
4. Wait for the Release workflow. Verify all five platform downloads against
   `SHA256SUMS`. From a fresh checkout of the tag, run the installer and
   confirm the downloaded binary and its vocabulary report the same release number. Give downstream projects the published release link.

The workflow builds the command for five platforms, runs the tests on each one it can
run, and publishes a release with one file per platform and `SHA256SUMS`. The title and
notes are the changelog's entry. It stops when the tag does not match the crate version
or the changelog has no entry. `install.sh` downloads the file for the machine from that
release and checks it against the sums.

Nothing built is committed to the repository.
