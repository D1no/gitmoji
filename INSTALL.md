# Install

```sh
installer=$(mktemp)
curl -fsSL https://github.com/D1no/gitmoji/releases/latest/download/install.sh -o "$installer" &&
  sh "$installer" /path/to/your/project
```

The installer downloads the binary for your platform and verifies its checksum. Signatures, PR titles, direct merges and history upgrades run in that binary. Repository operations use Git; hosted operations also use your GitHub credentials.
With a project path, it also installs the skill, settings and Git hooks. Run
`sh "$installer"` by itself to install only the command. The CLI, skill
and vocabulary use one release number. Run `gitmoji --version` to check it.

## Check a project installation

Run `gitmoji doctor` inside a checkout to check the active release, Git hooks, installed skill and enrolled workflows. For a session-start check, `gitmoji doctor --json` emits one array and exits with status 1 when a component needs attention. It works offline and leaves the checkout alone. [Diagnostic fields and examples](package/README.md#check-an-installation)

## Pin the CLI in CI

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
