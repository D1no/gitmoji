//! `gitmoji init`: set a git repository up for the convention.
//!
//! Everything the repository needs is compiled into this command: the agent skill, the
//! settings and the hooks. A file someone changed is kept or copied before it is
//! replaced, and a file of the repository's own, such as CLAUDE.md, is only edited in
//! the section this command owns.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use gitmoji_triangle::config::{Config, locate};
use gitmoji_triangle::guide;
use gitmoji_triangle::vocabulary::Vocabulary;

/// How the repository names its tickets.
#[derive(Clone, Copy, ValueEnum)]
pub enum Tickets {
    /// GitHub issues: #123.
    Github,
    /// Backlog.md tasks: TASK-150.
    Backlog,
}

pub(super) const SKILL_DIR: &str = ".claude/skills/gitmoji";

/// The reference inside the skill, written from the vocabulary so that it cannot differ
/// from what the command says.
const REFERENCE: &str = "references/vocabulary.txt";

/// The skill's other files, as they stand in `skills/gitmoji` of this repository.
const SKILL: &[(&str, &str)] = &[
    ("SKILL.md", include_str!("../../skills/gitmoji/SKILL.md")),
    (
        "references/dependabot.md",
        include_str!("../../skills/gitmoji/references/dependabot.md"),
    ),
    (
        "references/upgrade-history.md",
        include_str!("../../skills/gitmoji/references/upgrade-history.md"),
    ),
    (
        "references/pull-requests.md",
        include_str!("../../skills/gitmoji/references/pull-requests.md"),
    ),
];

/// Scripts and their README shipped under `scripts/` before those helpers moved into `gitmoji`.
const PREVIOUS_SKILL_SCRIPTS: &[&str] = &[
    "scripts/README.md",
    "scripts/antigravity.py",
    "scripts/claude-code.py",
    "scripts/codex.py",
    "scripts/github-merge.py",
    "scripts/github-pr-title.py",
    "scripts/gitmoji-upgrade-history.py",
];

/// SHA-256 digests (after CRLF -> LF normalization) of released `scripts/` files from
/// `v0.13.0` through `v0.16.0`, so committed downstream customizations are never deleted.
const RETIRED_SCRIPT_DIGESTS: &[(&str, &str)] = &[
    (
        "scripts/README.md",
        "4181ddcf712bf0ca7e4a77c04df734134654a3caa127c27eaa85d37615fe1372",
    ),
    (
        "scripts/README.md",
        "12debd59d01152e13711e2dd122723dccfd7ec7a92d349db8ad4e3f18413aa62",
    ),
    (
        "scripts/antigravity.py",
        "2598761ac9d4f6eb61ec43fdf887995c4b24ec2a348748dd5ce6df293451ae36",
    ),
    (
        "scripts/claude-code.py",
        "0907ce72f3aaa4964f9aae882d599c626352d02456042f7b99f8fad370e2356b",
    ),
    (
        "scripts/codex.py",
        "a6b0d2e48e2c3c052989b29001f1453c9d75356ef39f20630aa4e32da66b17cc",
    ),
    (
        "scripts/github-merge.py",
        "9ca7263f6e7256b442a5f3f179182af6f189b19a7a063456293ba397e59f38b4",
    ),
    (
        "scripts/github-merge.py",
        "8f82a9abdc418f3da24571f6144dc323fb48229ba952cb4f7b4a7e38911588fb",
    ),
    (
        "scripts/github-pr-title.py",
        "0d31322c46938b52ff17bd62bac4c232cb80083c9f8daca5714168a7138e1e50",
    ),
    (
        "scripts/gitmoji-upgrade-history.py",
        "76e2473612c889dfd396e4cc00ce4d81450eaeb891e3a567dab650b12b0a7c3c",
    ),
    (
        "scripts/gitmoji-upgrade-history.py",
        "9bcf56657c4453fb444b1901196661b322bff60790f6fccd5aef7bc2c0b32543",
    ),
    (
        "scripts/gitmoji-upgrade-history.py",
        "72c8dcff486296f3bf34e2b775b45b45e75cd928bfbd319163717806f206f6a1",
    ),
    (
        "scripts/gitmoji-upgrade-history.py",
        "6f2dea3ecd645a9246b5a0e0ea8d8c4e981947dbce80d19c0270dee95d1b1378",
    ),
    (
        "scripts/gitmoji-upgrade-history.py",
        "2a8b969ee21a3d4c0df0dd5ab721c6fbc38c08c087c78cc689b91f7f78368040",
    ),
    (
        "scripts/gitmoji-upgrade-history.py",
        "036564d709ccafef4684446c4ca739bc37af551eae6c8c83d59a086ef9b68a8c",
    ),
];

fn is_released_script(name: &str, bytes: &[u8]) -> bool {
    let mut normalized = Vec::with_capacity(bytes.len());
    for (index, &byte) in bytes.iter().enumerate() {
        if byte == b'\r' && bytes.get(index + 1) == Some(&b'\n') {
            continue;
        }
        normalized.push(byte);
    }
    let digest = format!("{:x}", Sha256::digest(&normalized));
    RETIRED_SCRIPT_DIGESTS
        .iter()
        .any(|(script, known)| *script == name && *known == digest)
}

/// The skill's files as releases up to 0.9 named them. They are taken away when a
/// project is brought up to date, so that an agent does not find two sets.
const EARLIER_SKILL_FILES: &[&str] = &[
    "SKILL.md",
    "harness/README.md",
    "harness/antigravity.py",
    "harness/claude-code.py",
    "harness/codex.py",
    "reference.txt",
];

const SETTINGS_GITHUB: &str = include_str!("../examples/gitmoji.github.toml");
const SETTINGS_BACKLOG: &str = include_str!("../examples/gitmoji.backlog.toml");
const LEFTHOOK: &str = include_str!("../examples/lefthook.yml");
const PREVIOUS_LEFTHOOK: &str = include_str!("legacy/lefthook-0.15.yml");
const LEFTHOOK_FILES: &[&str] = &[
    "lefthook.yml",
    "lefthook.yaml",
    ".lefthook.yml",
    ".lefthook.yaml",
    ".config/lefthook.yml",
    ".config/lefthook.yaml",
];

/// Where a new `gitmoji.toml` goes: beside the lefthook file when the repository keeps
/// that in a real `.config` folder of its own, otherwise at the root. A `.config` that
/// merely exists, or is a link to somewhere else, is not written to.
fn settings_home(top: &Path) -> &'static str {
    let config = top.join(".config");
    let own_folder = std::fs::symlink_metadata(&config).is_ok_and(|meta| meta.is_dir());
    let shared = [".config/lefthook.yml", ".config/lefthook.yaml"]
        .iter()
        .any(|name| top.join(name).is_file());
    if own_folder && shared {
        ".config/gitmoji.toml"
    } else {
        "gitmoji.toml"
    }
}

const COMMIT_MSG_HOOK: &str =
    "#!/bin/sh\n# Gitmoji Triangle: check the commit message.\nexec gitmoji check --fix \"$1\"\n";
const PREPARE_COMMIT_MSG_HOOK: &str = "#!/bin/sh\n# Gitmoji Triangle: describe the work arriving through a branch merge.\nexec gitmoji merge-message \"$1\" \"${2:-}\"\n";
const PRE_COMMIT_HOOK: &str = "#!/bin/sh\n# Gitmoji Triangle: say when the staged files look like more than one commit.\nexec gitmoji staged\n";

/// Names from before tool 0.7.0, when the command was called `triangle`.
const EARLIER_SETTINGS: &str = "triangle.toml";
const EARLIER_SKILL_DIR: &str = ".claude/skills/triangle-commit";
const EARLIER_COMMIT_MSG_HOOK: &str =
    "#!/bin/sh\n# Gitmoji Triangle: check the commit message.\nexec triangle check --fix \"$1\"\n";
const EARLIER_PRE_COMMIT_HOOK: &str = "#!/bin/sh\n# Gitmoji Triangle: say when the staged files look like more than one commit.\nexec triangle staged\n";

const BLOCK_START: &str = "<!-- gitmoji-triangle:start -->";
const BLOCK_END: &str = "<!-- gitmoji-triangle:end -->";
const BLOCK_BODY: &str = "## Commits

This repository uses the Gitmoji Triangle commit convention.

- Before you plan the commits for a ticket, and before every commit, use the `gitmoji`
  skill.
- A git hook checks each commit message with `gitmoji check`. When it refuses one, its
  message says what to change.
- A commit you write has a description under its first line that says why the change
  was made. Only a marker commit such as 🔺 needs none.
- Never add yourself as author or co-author of a commit. End the commit body with the
  signature the skill describes.

## Comments in code

Comment code where a reader could not guess the reason: why it is the way it is, what
must not be changed, where a value comes from. Say why, not what the line does. A comment
costs you almost nothing and saves the next reader, a person, a junior or an agent
without your context, from guessing.
";

/// Set up the repository that holds `path`.
pub fn run(
    vocab: &Vocabulary,
    path: &Path,
    tickets: Tickets,
    github_workflows: bool,
) -> Result<()> {
    let top = repository_root(path)?;
    // Every file this run wrote or changed, relative to the repository. Only these are
    // offered for the commit: never a backup, a file someone added, or a file left alone.
    let mut changed = Vec::new();

    let paths = SkillPaths::new(&top)?;
    let older_pin = older_pinned_version(&top)?;
    if let Some(pin) = &older_pin {
        if github_workflows {
            bail!(
                "the GitHub workflows need a numbered Gitmoji release at least 0.17.0 in gitmoji.toml (found {pin})"
            );
        }
        println!(
            "0. Compatibility: this repository pins gitmoji {pin} in gitmoji.toml; leaving the skill and managed workflows as they are."
        );
        println!(
            "   Install gitmoji {pin}, or raise [vocabulary].version to 0.17.0 or later and run `gitmoji init` again."
        );
    } else {
        rename_earlier(&top, &paths, &mut changed)?;

        println!("1. The skill, in {SKILL_DIR}");
        write_skill(&top, &paths, vocab, &mut changed)?;
    }
    let skill_dir = format!("{}/", paths.resolve(SKILL_DIR)?);
    let earlier_skill_dir = format!("{EARLIER_SKILL_DIR}/");
    let is_imported = |file: &str| {
        file.starts_with(&skill_dir)
            || file.starts_with(&earlier_skill_dir)
            || WORKFLOWS
                .iter()
                .any(|workflow| file == workflow.path || file == workflow.legacy_path)
    };

    println!("2. Settings");
    let adopted = write_settings(&top, vocab, tickets, &mut changed)?;

    println!("3. Git hooks");
    let lefthook_missing = write_hooks(&top, &mut changed)?;

    println!("4. Claude Code");
    write_claude_md(&top, &mut changed)?;
    write_claude_settings(&top, &mut changed)?;
    if older_pin.is_none() {
        write_workflows(&top, &paths, github_workflows, &mut changed)?;
    }

    println!();
    let this = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.canonicalize().ok());
    match (first_on_path(), &this) {
        (None, _) => {
            println!("The hooks run `gitmoji`, and it is not on your PATH yet.");
            if let Some(folder) = this.as_deref().and_then(Path::parent) {
                println!("Add this folder to your PATH: {}", folder.display());
            }
            println!();
        }
        (Some(first), Some(this)) if first.canonicalize().ok().as_ref() != Some(this) => {
            println!(
                "Another `gitmoji` comes first on your PATH: {}",
                first.display()
            );
            println!(
                "The hooks would run that one, not this one ({}). Replace it, or put this one earlier on PATH.",
                this.display()
            );
            println!();
        }
        _ => {}
    }
    changed.sort();
    changed.dedup();
    if changed.is_empty() {
        println!("Done. Nothing in the repository changed: the hooks are set up on this machine.");
    } else if adopted {
        println!("Done. This run wrote or changed these files. Commit exactly these:");
        println!();
        println!("  cd {}", shell_path(&top.to_string_lossy()));
        print_stage_command(&changed);
        println!(
            "  git commit -m '🔺 gitmoji `{}`: adopt the Triangle convention'",
            vocab.version
        );
    } else {
        let imported: Vec<String> = changed
            .iter()
            .filter(|file| is_imported(file))
            .cloned()
            .collect();
        let local: Vec<String> = changed
            .iter()
            .filter(|file| !is_imported(file))
            .cloned()
            .collect();
        for line in existing_repo_commit_guidance(&top, vocab, &imported, &local) {
            println!("{line}");
        }
    }
    let leftovers = leftovers(&top)?;
    if !leftovers.is_empty() {
        println!();
        println!("Worth a look, left over from an earlier version:");
        for line in leftovers {
            println!("  - {line}");
        }
    }
    println!();
    println!("Then try: gitmoji guide");
    println!();
    println!(
        "The skill was written for Claude Code, in {SKILL_DIR}. For other agents, such as Codex or Cursor, install it with: npx skills add D1no/gitmoji"
    );
    if lefthook_missing {
        println!();
        println!(
            "Recommended: install lefthook (https://github.com/evilmartians/lefthook) and run `gitmoji init` again."
        );
        println!(
            "Plain git hooks work, but they are not committed, so a fresh clone has none until `gitmoji init` runs there."
        );
    }
    Ok(())
}

fn git(folder: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(folder)
        .args(args)
        .output()
        .context("cannot run git")?;
    if !output.status.success() {
        bail!("{} is not inside a git repository", folder.display());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn repository_root(path: &Path) -> Result<PathBuf> {
    if !path.is_dir() {
        bail!("{} is not a folder", path.display());
    }
    Ok(PathBuf::from(git(path, &["rev-parse", "--show-toplevel"])?))
}

/// How this repository writes a ticket, for the commit this command suggests.
fn ticket_example(top: &Path) -> String {
    locate(top)
        .and_then(|file| Config::load(&file).ok())
        .map_or_else(|| "#123".to_string(), |config| config.ticket_example)
}

/// Which scope placeholder fits the repository's `[scope].allowed` setting.
fn suggested_scope(top: &Path) -> (String, Option<String>) {
    let allowed = locate(top)
        .and_then(|file| Config::load(&file).ok())
        .map(|config| config.scopes_allowed)
        .unwrap_or_default();
    if allowed.is_empty() || allowed.iter().any(|scope| scope == "gitmoji") {
        ("gitmoji".to_string(), None)
    } else {
        ("<scope>".to_string(), Some(allowed.join(", ")))
    }
}

fn print_stage_command(files: &[String]) {
    println!("  git --literal-pathspecs add -- \\");
    for (index, file) in files.iter().enumerate() {
        let more = if index + 1 < files.len() { " \\" } else { "" };
        println!("    {}{more}", shell_path(file));
    }
}

fn push_stage_command(lines: &mut Vec<String>, files: &[String]) {
    lines.push("  git --literal-pathspecs add -- \\".to_string());
    for (index, file) in files.iter().enumerate() {
        let more = if index + 1 < files.len() { " \\" } else { "" };
        lines.push(format!("    {}{more}", shell_path(file)));
    }
}

fn existing_repo_commit_guidance(
    top: &Path,
    vocab: &Vocabulary,
    imported: &[String],
    local: &[String],
) -> Vec<String> {
    let ticket = ticket_example(top);
    let (scope, allowed_note) = suggested_scope(top);
    let mut lines = Vec::new();
    let mixed = !imported.is_empty() && !local.is_empty();
    if mixed {
        lines.push(
            "Done. This run updated both shipped Gitmoji files and project-owned setup. Commit them separately:"
                .to_string(),
        );
    } else {
        lines
            .push("Done. This run wrote or changed these files. Commit exactly these:".to_string());
    }
    lines.push(String::new());
    lines.push(format!("  cd {}", shell_path(&top.to_string_lossy())));

    if !imported.is_empty() {
        if mixed {
            lines.push(String::new());
            lines.push("1. Shipped skill and enrolled workflow templates:".to_string());
        }
        push_stage_command(&mut lines, imported);
        let has_skill = imported.iter().any(|file| !file.starts_with(".github/"));
        let has_workflows = imported.iter().any(|file| file.starts_with(".github/"));
        let noun = match (has_skill, has_workflows) {
            (true, true) => "import the gitmoji skill and workflows",
            (true, false) => "import the gitmoji skill",
            (false, true) => "import the gitmoji workflows",
            (false, false) => "import the gitmoji skill",
        };
        lines.push(format!(
            "  git commit -m '🤖📥 <ticket> ({scope}): {noun} from v{}' \\",
            vocab.version
        ));
        lines.push(format!(
            "    -m 'Brings them in line with gitmoji {}.' \\",
            vocab.version
        ));
        lines.push(format!(
            "    -m 'Source: https://github.com/D1no/gitmoji/releases/tag/v{}'",
            vocab.version
        ));
        lines.push(String::new());
        lines.push(format!(
            "Put the ticket this update belongs to in place of <ticket>, written like {ticket}."
        ));
    }

    if !local.is_empty() {
        if mixed {
            lines.push(String::new());
            lines.push("2. Project-owned configuration, hooks, or instructions:".to_string());
        }
        push_stage_command(&mut lines, local);
        lines.push(String::new());
        lines.push(format!(
            "No shipped skill files are in this group; only local configuration, hooks, or instructions changed. Pick the intent that fits those files (for example `🤖🔧`, `🧱🔧`, or `👷🔧`), with a ticket like {ticket} and ({scope})."
        ));
    }

    if let Some(allowed) = allowed_note {
        lines.push(format!(
            "Put one of this repository's allowed scopes in place of <scope>: {allowed}."
        ));
    }
    lines.push(format!(
        "What changed in this version: {}",
        gitmoji_triangle::CHANGELOG
    ));
    lines.push(
        "To adopt this release, set its version in gitmoji.toml and commit that with:".to_string(),
    );
    lines.push(format!(
        "  🔺 gitmoji `{}`: <what changes for this repository>",
        vocab.version
    ));
    lines
}

/// The gitmoji commands a hook file runs, each as its words from `gitmoji` on. Comment
/// lines are not commands.
fn gitmoji_commands(text: &str) -> impl Iterator<Item = Vec<&str>> {
    text.lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .filter_map(|line| {
            let command = line.split(" #").next().unwrap_or(line);
            let words: Vec<&str> = command.split_whitespace().collect();
            let start = words.iter().position(|word| {
                let word = word.trim_matches(['"', '\'']);
                word == "gitmoji" || word.ends_with("/gitmoji")
            })?;
            Some(words[start..].to_vec())
        })
}

/// A path as a hook file writes it, without quotes or a leading `./`.
fn plain_path(word: &str) -> String {
    let word = word.trim_matches(['"', '\'']);
    word.strip_prefix("./").unwrap_or(word).to_string()
}

/// Things an earlier version needed and this one does not. They are pointed out, not
/// changed: they sit in files the repository owns.
fn leftovers(top: &Path) -> Result<Vec<String>> {
    let mut found = Vec::new();
    if let Some(file) = locate(top) {
        let text = read_if_present(&file)?.unwrap_or_default();
        let old_limit = text.lines().any(|line| {
            line.split('#')
                .next()
                .is_some_and(|setting| setting.replace(' ', "") == "subject_max=80")
        });
        if old_limit {
            let name = file
                .strip_prefix(top)
                .unwrap_or(&file)
                .display()
                .to_string();
            found.push(format!(
                "{name} sets subject_max = 80. The limit is 140 unless a repository says otherwise; delete the line to use that."
            ));
        }
    }
    // Only a command that names the very file gitmoji finds by itself needs no option.
    let Some(own) = locate(top).map(|file| {
        file.strip_prefix(top)
            .unwrap_or(&file)
            .to_string_lossy()
            .replace('\\', "/")
    }) else {
        return Ok(found);
    };
    let hooks = git(top, &["rev-parse", "--git-path", "hooks"]).map(|folder| top.join(folder));
    let places = LEFTHOOK_FILES
        .iter()
        .map(|name| (name.to_string(), top.join(name)))
        .chain(hooks.into_iter().flat_map(|folder| {
            ["commit-msg", "pre-commit", "prepare-commit-msg"]
                .map(|hook| (format!("the {hook} hook"), folder.join(hook)))
        }));
    for (name, path) in places {
        let text = read_if_present(&path)?.unwrap_or_default();
        let needless = gitmoji_commands(&text).any(|words| {
            words
                .windows(2)
                .any(|pair| pair[0] == "--config" && plain_path(pair[1]) == own)
                || words.iter().any(|word| {
                    word.strip_prefix("--config=").map(plain_path).as_deref() == Some(&own)
                })
        });
        if needless {
            found.push(format!(
                "{name} passes --config {own} to gitmoji. It finds that file by itself now, so the option can go."
            ));
        }
    }
    Ok(found)
}

/// The `gitmoji` a git hook would run: the first one on PATH.
fn first_on_path() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .flat_map(|folder| [folder.join("gitmoji"), folder.join("gitmoji.exe")])
        .find(|file| file.is_file())
}

/// The installer prints a shell command that stages only the files it changed.
fn shell_path(value: &str) -> String {
    if value
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"/.-_".contains(&b))
    {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

fn tracked(top: &Path, file: &str) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(top)
        .args([
            "--literal-pathspecs",
            "ls-files",
            "--error-unmatch",
            "--",
            file,
        ])
        .output()
        .is_ok_and(|output| output.status.success())
}

/// Whether the file has changes in the index that are not committed, or is in conflict.
fn staged(top: &Path, file: &str) -> Result<bool> {
    let output = Command::new("git")
        .arg("-C")
        .arg(top)
        .args([
            "--literal-pathspecs",
            "diff",
            "--cached",
            "--quiet",
            "--",
            file,
        ])
        .output()
        .context("cannot check the Git index")?;
    match output.status.code() {
        Some(0) => Ok(false),
        Some(1) => Ok(true),
        _ => bail!(
            "cannot check the index for {file}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ),
    }
}

/// Whether the file in the working tree is what the last commit holds.
fn same_as_committed(top: &Path, file: &str) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(top)
        .args(["--literal-pathspecs", "diff", "--quiet", "HEAD", "--", file])
        .output()
        .is_ok_and(|output| output.status.success())
}

/// A directory shared across harnesses can have several names in the worktree.
pub(super) struct SkillPaths {
    root: PathBuf,
    staged_entries: HashSet<Vec<u8>>,
}

impl SkillPaths {
    /// Inspection follows the same in-repository links without checking whether
    /// writing through them would alter a staged decision.
    pub(super) fn read_only(top: &Path) -> Result<Self> {
        Ok(Self {
            root: top.canonicalize()?,
            staged_entries: HashSet::new(),
        })
    }

    fn new(top: &Path) -> Result<Self> {
        let listed = Command::new("git")
            .arg("-C")
            .arg(top)
            .args(["diff", "--cached", "--name-only", "--no-renames", "-z"])
            .output()
            .context("cannot check the index before installing the skill")?;
        if !listed.status.success() {
            bail!(
                "cannot check the index: {}",
                String::from_utf8_lossy(&listed.stderr).trim()
            );
        }
        let staged_entries = listed
            .stdout
            .split(|&b| b == 0)
            .filter(|path| !path.is_empty())
            .map(<[u8]>::to_vec)
            .collect();
        Ok(Self {
            root: top.canonicalize()?,
            staged_entries,
        })
    }

    /// Resolve existing links even when the leaf is missing or staged for deletion.
    pub(super) fn resolve(&self, name: &str) -> Result<String> {
        let mut path = self.root.clone();
        let mut parts = Path::new(name).iter().peekable();
        while let Some(part) = parts.next() {
            path.push(part);
            match std::fs::symlink_metadata(&path) {
                Ok(meta) if meta.file_type().is_symlink() => {
                    path = path.canonicalize().with_context(|| {
                        format!("cannot resolve the skill link {}", path.display())
                    })?;
                    let target = path.strip_prefix(&self.root).with_context(|| {
                        format!("skill path {name} points outside the repository")
                    })?;
                    if target
                        .components()
                        .any(|part| part.as_os_str().eq_ignore_ascii_case(".git"))
                    {
                        bail!("skill path {name} points into Git metadata");
                    }
                }
                Err(err) if err.kind() != std::io::ErrorKind::NotFound => {
                    return Err(err).with_context(|| format!("cannot inspect {}", path.display()));
                }
                _ => {}
            }
            if parts.peek().is_some() {
                // A removed directory link can itself be staged. Creating children in its
                // place would cause the suggested git add to replace the staged decision.
                let dir = git_path(path.strip_prefix(&self.root)?)?;
                if self.staged_entries.contains(dir.as_bytes()) {
                    bail!(
                        "{dir} has staged changes; commit or unstage it before installing the skill"
                    );
                }
            }
        }
        git_path(path.strip_prefix(&self.root)?)
    }
}

fn git_path(relative: &Path) -> Result<String> {
    let parts = relative
        .iter()
        .map(|part| part.to_str().context("skill path is not UTF-8"));
    Ok(parts.collect::<Result<Vec<_>>>()?.join("/"))
}

/// Bring a repository set up before tool 0.7.0 over to the new names. Only what that
/// version wrote is touched.
fn rename_earlier(top: &Path, paths: &SkillPaths, changed: &mut Vec<String>) -> Result<()> {
    let mut said = false;
    let mut say = |line: String| {
        if !said {
            println!("0. Names and files from earlier releases");
            said = true;
        }
        println!("   {line}");
    };

    let earlier = top.join(EARLIER_SETTINGS);
    let settings = top.join("gitmoji.toml");
    if earlier.is_file() && staged(top, EARLIER_SETTINGS)? {
        say(format!(
            "{EARLIER_SETTINGS} has staged changes; left as it is. Commit or unstage it, then run `gitmoji init` again"
        ));
    } else if earlier.is_file() && !settings.exists() && !top.join(".config/gitmoji.toml").exists()
    {
        let was_tracked = tracked(top, EARLIER_SETTINGS);
        std::fs::rename(&earlier, &settings)
            .with_context(|| format!("cannot rename {}", earlier.display()))?;
        changed.push("gitmoji.toml".to_string());
        if was_tracked {
            changed.push(EARLIER_SETTINGS.to_string());
        }
        say(format!("renamed {EARLIER_SETTINGS} to gitmoji.toml"));
    }

    let current_dir = paths.resolve(SKILL_DIR)?;
    let skill = top.join(EARLIER_SKILL_DIR);
    // The old installer shipped a real directory. An alias may point to a shared skill.
    if skill.is_dir() && !skill.is_symlink() {
        let earlier_dir = paths.resolve(EARLIER_SKILL_DIR)?;
        for name in EARLIER_SKILL_FILES.iter().copied() {
            if skill.join(name).is_symlink() {
                // The old installer shipped regular files. A link belongs to the project.
                continue;
            }
            let path = paths.resolve(&format!("{earlier_dir}/{name}"))?;
            let file = top.join(&path);
            if !file.is_file() {
                continue;
            }
            if staged(top, &path)? {
                say(format!(
                    "{path} has staged changes; left as it is. Commit or unstage it, then run `gitmoji init` again"
                ));
                continue;
            }
            let was_tracked = tracked(top, &path);
            // A file git can give back goes. Anything else may hold someone's edits,
            // so it moves beside the new file.
            if was_tracked && same_as_committed(top, &path) {
                std::fs::remove_file(&file)
                    .with_context(|| format!("cannot remove {}", file.display()))?;
            } else {
                let backup = paths.resolve(&format!("{current_dir}/{name}.before-update"))?;
                let kept = top.join(&backup);
                if kept.exists() || staged(top, &backup)? {
                    say(format!(
                        "{path} was not as committed, and {backup} is already there; both left as they are. Compare them, delete the copy, then run `gitmoji init` again"
                    ));
                    continue;
                }
                if let Some(folder) = kept.parent() {
                    std::fs::create_dir_all(folder)
                        .with_context(|| format!("cannot create {}", folder.display()))?;
                }
                std::fs::rename(&file, &kept)
                    .with_context(|| format!("cannot move {}", file.display()))?;
                say(format!("{path} was not as committed; kept as {backup}"));
            }
            if was_tracked {
                changed.push(path);
            }
        }
        // Folders go only when nothing of yours is left in them.
        let _ = std::fs::remove_dir(skill.join("harness"));
        if std::fs::remove_dir(&skill).is_ok() {
            say(format!(
                "removed {earlier_dir}; the skill is now {current_dir}"
            ));
        } else {
            say(format!(
                "removed the shipped files from {earlier_dir}; your own files there stay"
            ));
        }
    }

    // Releases up to 0.9 kept the scripts in `harness/` and the reference beside
    // SKILL.md, and releases up to 0.16 shipped Python helpers under `scripts/`.
    // Those that git can give back go (for `scripts/`, only exact released content);
    // anything else is left and named.
    for (name, reason, verify_digest) in EARLIER_SKILL_FILES
        .iter()
        .copied()
        .filter(|name| *name != "SKILL.md")
        .map(|name| {
            (
                name,
                "the skill keeps its reference under references/ now",
                false,
            )
        })
        .chain(
            PREVIOUS_SKILL_SCRIPTS
                .iter()
                .copied()
                .map(|name| (name, "`gitmoji` provides these commands directly now", true)),
        )
    {
        if top.join(&current_dir).join(name).is_symlink() {
            continue;
        }
        let path = paths.resolve(&format!("{SKILL_DIR}/{name}"))?;
        let file = top.join(&path);
        if !file.is_file() {
            continue;
        }
        if staged(top, &path)? {
            say(format!(
                "{path} has staged changes; left as it is. Commit or unstage it, then run `gitmoji init` again"
            ));
        } else if tracked(top, &path) && same_as_committed(top, &path) {
            if verify_digest {
                let bytes = std::fs::read(&file)
                    .with_context(|| format!("cannot read {}", file.display()))?;
                if !is_released_script(name, &bytes) {
                    say(format!(
                        "{path} does not match a released gitmoji script; left as it is. Delete it when you no longer need it"
                    ));
                    continue;
                }
            }
            std::fs::remove_file(&file)
                .with_context(|| format!("cannot remove {}", file.display()))?;
            changed.push(path.clone());
            say(format!("removed {path}; {reason}"));
        } else {
            say(format!(
                "{path} is from an earlier layout and is not as committed; left as it is. Delete it when you no longer need it"
            ));
        }
    }
    let _ = std::fs::remove_dir(top.join(&current_dir).join("harness"));
    let _ = std::fs::remove_dir(top.join(&current_dir).join("scripts"));

    for name in LEFTHOOK_FILES.iter().copied() {
        let file = top.join(name);
        let Some(old) = read_if_present(&file)? else {
            continue;
        };
        let new = old
            .replace("triangle staged", "gitmoji staged")
            .replace("triangle check", "gitmoji check")
            .replace("triangle-staged", "gitmoji-staged")
            .replace("triangle-check", "gitmoji-check")
            .replace("`triangle` must be", "`gitmoji` must be");
        if new != old {
            write(&file, &new)?;
            changed.push(name.to_string());
            say(format!("{name} now runs gitmoji"));
        }
    }

    if let Ok(folder) = git(top, &["rev-parse", "--git-path", "hooks"]) {
        let hooks = top.join(folder);
        for (hook, content) in [
            ("commit-msg", EARLIER_COMMIT_MSG_HOOK),
            ("pre-commit", EARLIER_PRE_COMMIT_HOOK),
        ] {
            let file = hooks.join(hook);
            if read_if_present(&file)?.as_deref() == Some(content) {
                std::fs::remove_file(&file)
                    .with_context(|| format!("cannot remove {}", file.display()))?;
                say(format!("removed the plain {hook} hook that ran triangle"));
            }
        }
    }
    Ok(())
}

fn read_if_present(path: &Path) -> Result<Option<String>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(String::from_utf8_lossy(&bytes).into_owned())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("cannot read {}", path.display())),
    }
}

fn write(path: &Path, content: &str) -> Result<()> {
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder)
            .with_context(|| format!("cannot create {}", folder.display()))?;
    }
    std::fs::write(path, content).with_context(|| format!("cannot write {}", path.display()))
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
        .with_context(|| format!("cannot make {} executable", path.display()))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<()> {
    Ok(())
}

pub(super) fn skill_files(reference: &str) -> impl Iterator<Item = (&str, &str)> {
    SKILL
        .iter()
        .copied()
        .chain(std::iter::once((REFERENCE, reference)))
}

/// Write the skill. A file someone added stays; a shipped file someone changed is kept
/// beside the new one as `<name>.before-update`.
fn write_skill(
    top: &Path,
    paths: &SkillPaths,
    vocab: &Vocabulary,
    changed: &mut Vec<String>,
) -> Result<()> {
    let reference = guide::all(vocab);
    for (name, content) in skill_files(&reference) {
        let path = paths.resolve(&format!("{SKILL_DIR}/{name}"))?;
        let to = top.join(&path);
        // What is staged is someone's work in progress, an edit or a deletion, and
        // `git add` would replace it. This comes before anything is written.
        if staged(top, &path)? {
            println!(
                "   {path} has staged changes; left as it is. Commit or unstage it, then run `gitmoji init` again"
            );
            continue;
        }
        let Some(old) = read_if_present(&to)? else {
            write(&to, content)?;
            changed.push(path);
            continue;
        };
        if old == content {
            continue;
        }
        // An older version that was committed as it is can be had back from git.
        if !(tracked(top, &path) && same_as_committed(top, &path)) {
            let backup = format!("{path}.before-update");
            let backup_file = top.join(&backup);
            if backup_file.symlink_metadata().is_ok() || staged(top, &backup)? {
                println!(
                    "   {path} has changes that were never committed, and {backup} from an earlier update is still there; both left as they are. Compare them, delete the copy, then run `gitmoji init` again"
                );
                continue;
            }
            write(&backup_file, &old)?;
            println!(
                "   {path} had changes that were never committed; they are kept in {backup}. Compare the two, then delete that file"
            );
        }
        write(&to, content)?;
        changed.push(path);
    }
    Ok(())
}

pub(super) struct Workflow {
    pub(super) path: &'static str,
    pub(super) legacy_path: &'static str,
    legacy_job: &'static str,
    pub(super) job: &'static str,
    pub(super) template: &'static str,
}

pub(super) const WORKFLOWS: &[Workflow] = &[
    Workflow {
        path: ".github/workflows/gitmoji-pr-title.yml",
        legacy_path: ".github/workflows/pr-title.yml",
        legacy_job: "title",
        job: "gitmoji-pr-title",
        template: include_str!("../../.github/workflows/gitmoji-pr-title.yml"),
    },
    Workflow {
        path: ".github/workflows/gitmoji-commits.yml",
        legacy_path: ".github/workflows/check-commits.yml",
        legacy_job: "check",
        job: "gitmoji-commits",
        template: include_str!("../../.github/workflows/gitmoji-commits.yml"),
    },
];
pub(super) const WORKFLOW_MARKER: &str = "# Managed by gitmoji init --github-workflows";
pub(super) const WORKFLOW_PIN: &str = "  GITMOJI_VERSION: ";

/// Parse a strict `major.minor.patch` release version without leading zeros.
fn parse_release_version(version: &str) -> Option<(u64, u64, u64)> {
    let numbers: Vec<_> = version
        .split('.')
        .map(|part| {
            if part.is_empty()
                || !part.bytes().all(|c| c.is_ascii_digit())
                || (part.len() > 1 && part.starts_with('0'))
            {
                return None;
            }
            part.parse::<u64>().ok()
        })
        .collect();
    match numbers.as_slice() {
        [Some(major), Some(minor), Some(patch)] => Some((*major, *minor, *patch)),
        _ => None,
    }
}

/// Return the pinned version when the repository already pins a release older than 0.17.0.
fn older_pinned_version(top: &Path) -> Result<Option<String>> {
    let Some(path) = locate(top) else {
        return Ok(None);
    };
    let Some(version) = Config::load(&path)?.vocabulary_version else {
        return Ok(None);
    };
    Ok(match parse_release_version(&version) {
        Some((major, minor, _)) if (major, minor) < (0, 17) => Some(version),
        _ => None,
    })
}

/// Only numbered release tags from the public Gitmoji repository may supply executable code.
fn workflow_version(top: &Path) -> Result<String> {
    let path =
        locate(top).context("GitHub workflows need a gitmoji.toml with [vocabulary].version")?;
    let version = Config::load(&path)?.vocabulary_version.with_context(|| {
        format!(
            "set [vocabulary].version in {} before installing the GitHub workflows",
            path.display()
        )
    })?;
    match parse_release_version(&version) {
        Some((major, minor, _)) if (major, minor) >= (0, 17) => Ok(version),
        _ => bail!(
            "the GitHub workflows need a numbered Gitmoji release at least 0.17.0 in {} (found {version:?})",
            path.display()
        ),
    }
}

/// Only the release pin varies between the shipped workflow and the installed copy.
pub(super) fn pin_workflow(template: &str, version: &str) -> String {
    let mut result = String::new();
    for line in template.lines() {
        if line.starts_with(WORKFLOW_PIN) {
            result.push_str(&format!("{WORKFLOW_PIN}'{version}'"));
        } else {
            result.push_str(line);
        }
        result.push('\n');
    }
    result
}

/// A workflow belongs to the installer only while it carries the ownership line.
/// Links, directories and files without the line are always project-owned.
pub(super) enum WorkflowFile {
    Missing,
    Managed(String),
    ProjectOwned,
}

pub(super) fn workflow_file(path: &Path) -> Result<WorkflowFile> {
    match path.symlink_metadata() {
        Ok(meta) if !meta.is_file() => return Ok(WorkflowFile::ProjectOwned),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(WorkflowFile::Missing);
        }
        Err(error) => {
            return Err(error).with_context(|| format!("cannot inspect {}", path.display()));
        }
        Ok(_) => {}
    }
    let bytes = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    let header = bytes.split(|&b| b == b'\n').next().unwrap_or_default();
    if header.strip_suffix(b"\r").unwrap_or(header) != WORKFLOW_MARKER.as_bytes() {
        return Ok(WorkflowFile::ProjectOwned);
    }
    Ok(WorkflowFile::Managed(
        String::from_utf8(bytes)
            .with_context(|| format!("managed workflow {} is not UTF-8", path.display()))?,
    ))
}

/// Claim a fresh workflow or backup name without replacing another writer's file.
fn create_workflow_file(path: &Path, content: &str) -> Result<bool> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match std::fs::File::create_new(path) {
        Ok(mut file) => {
            file.write_all(content.as_bytes())
                .with_context(|| format!("cannot write {}", path.display()))?;
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(error) => Err(error).with_context(|| format!("cannot create {}", path.display())),
    }
}

fn backup_workflow(top: &Path, name: &str, old: &str) -> Result<bool> {
    // These released PR-title templates used the Python helper. They differ from
    // the native template even when the project has never edited the workflow.
    const RELEASED: &[(&str, &str)] = &[
        (
            ".github/workflows/pr-title.yml",
            "9b453f5dd6a6ef6bb788b6b8f1a005da78def680be1e0e9bbe3df476dba98676",
        ), // 0.15.0
        (
            ".github/workflows/gitmoji-pr-title.yml",
            "5d1151655fe981b30615b916cc2bb1162bbac6395b737ecc5a06dc57804ec171",
        ), // 0.15.1
        (
            ".github/workflows/gitmoji-pr-title.yml",
            "a660fa96ddce35e12dca9da60622798b25761a0619dd7dbf760fa6f9bd81c7a8",
        ), // 0.16.0
    ];
    let digest = format!("{:x}", Sha256::digest(old.replace("\r\n", "\n").as_bytes()));
    if RELEASED.contains(&(name, digest.as_str())) {
        return Ok(true);
    }

    let backup = format!("{name}.before-update");
    let path = top.join(&backup);
    if !staged(top, &backup)? {
        if create_workflow_file(&path, old)? {
            println!("   kept edits to {name} in {backup}. Compare the files before committing");
            return Ok(true);
        }
        // A previous run may have backed up this file before encountering a
        // conflicting destination. Reuse an identical regular file on retry.
        if path.symlink_metadata()?.is_file() && std::fs::read(&path)? == old.as_bytes() {
            return Ok(true);
        }
    }
    println!(
        "   {name} has edits and {backup} is already in use. Review the backup before updating"
    );
    Ok(false)
}

fn write_workflows(
    top: &Path,
    paths: &SkillPaths,
    requested: bool,
    changed: &mut Vec<String>,
) -> Result<()> {
    if [".github", ".github/workflows"]
        .iter()
        .any(|name| top.join(name).is_symlink())
    {
        if requested {
            bail!("cannot install Gitmoji workflows through a symbolic link");
        }
        return Ok(());
    }
    for workflow in WORKFLOWS {
        let name = workflow.path;
        let legacy_name = workflow.legacy_path;
        let path = top.join(name);
        let legacy_path = top.join(legacy_name);
        let current = workflow_file(&path)?;
        let legacy = workflow_file(&legacy_path)?;
        if !requested
            && !matches!(current, WorkflowFile::Managed(_))
            && !matches!(legacy, WorkflowFile::Managed(_))
        {
            continue;
        }
        if matches!(current, WorkflowFile::ProjectOwned) {
            println!(
                "   {name} belongs to the project. Choose a different path for it before installing the Gitmoji workflow"
            );
            continue;
        }
        paths.resolve(name)?;
        if staged(top, name)?
            || (matches!(legacy, WorkflowFile::Managed(_)) && staged(top, legacy_name)?)
        {
            println!(
                "   the workflow has staged changes; commit or unstage them before updating {name}"
            );
            continue;
        }
        let version = workflow_version(top)?;
        let content = pin_workflow(workflow.template, &version);
        let current_text = match current {
            WorkflowFile::Managed(ref text) => Some(text),
            _ => None,
        };
        if let WorkflowFile::Managed(ref text) = legacy {
            // The 0.15.0 templates used generic job IDs. Preserve any edits
            // beyond that rename and the routine pin change before retiring them.
            let previous_template = content.replace(
                &format!("\njobs:\n  {}:\n", workflow.job),
                &format!("\njobs:\n  {}:\n", workflow.legacy_job),
            );
            if pin_workflow(text, &version) != previous_template
                && !backup_workflow(top, legacy_name, text)?
            {
                continue;
            }
        }
        if let Some(old) = current_text
            && pin_workflow(old, &version) != content
            && !backup_workflow(top, name, old)?
        {
            continue;
        }
        if current_text.is_none_or(|old| old.replace("\r\n", "\n") != content) {
            if current_text.is_some() {
                write(&path, &content)?;
            } else if !create_workflow_file(&path, &content)? {
                println!("   {name} appeared during installation. Inspect it and rerun init");
                continue;
            }
            changed.push(name.to_owned());
            println!("   wrote {name} for Gitmoji {version}");
        }
        if matches!(legacy, WorkflowFile::Managed(_)) {
            std::fs::remove_file(&legacy_path)
                .with_context(|| format!("cannot retire {legacy_name}"))?;
            if tracked(top, legacy_name) {
                changed.push(legacy_name.to_owned());
            }
            println!(
                "   migrated {legacy_name} to {name}. If you required Gitmoji's `{}` check, switch the requirement to `{}` after its first run. Keep any required project check of the same name.",
                workflow.legacy_job, workflow.job
            );
        }
    }
    Ok(())
}

/// Write `gitmoji.toml` when there is none. Says whether this run adopted the convention.
fn write_settings(
    top: &Path,
    vocab: &Vocabulary,
    tickets: Tickets,
    changed: &mut Vec<String>,
) -> Result<bool> {
    if let Some(found) = ["gitmoji.toml", ".config/gitmoji.toml"]
        .into_iter()
        .find(|name| top.join(name).exists())
    {
        println!("   {found} exists; left as it is");
        return Ok(false);
    }
    // The settings from before 0.7.0 are still waiting to be renamed, because they
    // have staged changes. Writing defaults now would leave them stranded.
    if top.join(EARLIER_SETTINGS).is_file() {
        println!(
            "   {EARLIER_SETTINGS} is still to be renamed; no new settings are written until then"
        );
        return Ok(false);
    }
    let file = settings_home(top);
    let path = top.join(file);
    let (example, name) = match tickets {
        Tickets::Github => (SETTINGS_GITHUB, "GitHub"),
        Tickets::Backlog => (SETTINGS_BACKLOG, "Backlog.md"),
    };
    let content = format!(
        "{example}\n# The vocabulary version this repository has adopted. Change it with a 🔺 commit.\n[vocabulary]\nversion = \"{}\"\n",
        vocab.version
    );
    write(&path, &content)?;
    println!(
        "   wrote {file} ({name} tickets, gitmoji {})",
        vocab.version
    );
    changed.push(file.to_string());
    Ok(true)
}

/// Set the hooks up, through lefthook when it is installed. Says whether lefthook is missing.
fn write_hooks(top: &Path, changed: &mut Vec<String>) -> Result<bool> {
    let hooks = {
        let folder = PathBuf::from(git(top, &["rev-parse", "--git-path", "hooks"])?);
        if folder.is_absolute() {
            folder
        } else {
            top.join(folder)
        }
    };
    let plain = [
        ("commit-msg", COMMIT_MSG_HOOK, "gitmoji check --fix \"$1\""),
        ("pre-commit", PRE_COMMIT_HOOK, "gitmoji staged"),
        (
            "prepare-commit-msg",
            PREPARE_COMMIT_MSG_HOOK,
            r#"gitmoji merge-message "$1" "${2:-}""#,
        ),
    ];
    let existing = LEFTHOOK_FILES
        .iter()
        .copied()
        .find(|name| top.join(name).exists());
    if let Some(name) = existing {
        let path = top.join(name);
        let text = read_if_present(&path)?.unwrap_or_default();
        if text == PREVIOUS_LEFTHOOK {
            write(&path, LEFTHOOK)?;
            changed.push(name.to_string());
            println!("   added prepare-commit-msg to {name}");
        } else if !gitmoji_commands(&text).any(|words| words.contains(&"check")) {
            println!("   {name} exists without the gitmoji commands; left as it is.");
            println!("   Add these to it, then run `lefthook install`:");
            for line in LEFTHOOK.lines().filter(|line| !line.starts_with('#')) {
                println!("     {line}");
            }
            return Ok(false);
        } else if !gitmoji_commands(&text).any(|words| words.contains(&"merge-message")) {
            println!(
                "   {name} is project-managed. Add this command under prepare-commit-msg, then run `lefthook install`:"
            );
            for line in LEFTHOOK
                .lines()
                .skip_while(|line| *line != "prepare-commit-msg:")
            {
                println!("     {line}");
            }
        }
    }
    let lefthook = Command::new("lefthook").arg("version").output().is_ok();
    if lefthook {
        let name = match existing {
            Some(name) => name.to_string(),
            None => {
                write(&top.join("lefthook.yml"), LEFTHOOK)?;
                changed.push("lefthook.yml".to_string());
                println!("   wrote lefthook.yml");
                "lefthook.yml".to_string()
            }
        };
        // A plain hook this command wrote earlier, unchanged since, makes way for lefthook.
        for (hook, content, _) in plain {
            let path = hooks.join(hook);
            if read_if_present(&path)?.as_deref() == Some(content) {
                std::fs::remove_file(&path)
                    .with_context(|| format!("cannot remove {}", path.display()))?;
            }
        }
        let status = Command::new("lefthook")
            .arg("install")
            .current_dir(top)
            .output()
            .context("cannot run lefthook")?;
        if !status.status.success() {
            bail!(
                "lefthook install failed: {}",
                String::from_utf8_lossy(&status.stderr).trim()
            );
        }
        println!("   installed the hooks from {name} with lefthook");
        return Ok(false);
    }
    for (hook, content, command) in plain {
        let path = hooks.join(hook);
        match read_if_present(&path)? {
            Some(old) if old == content => println!("   {hook} is already the plain gitmoji hook"),
            Some(_) => println!("   a {hook} hook exists; left as it is. It must run: {command}"),
            None => {
                write(&path, content)?;
                make_executable(&path)?;
                println!("   wrote a plain {hook} hook (lefthook is not installed)");
            }
        }
    }
    Ok(true)
}

/// The text of CLAUDE.md with the Commits section added or brought up to date.
fn with_commits_section(old: &str) -> String {
    // The file's own line endings, so that a checkout with CRLF stays clean.
    let eol = if old.contains("\r\n") { "\r\n" } else { "\n" };
    let inner = format!("{BLOCK_START}\n{BLOCK_BODY}{BLOCK_END}").replace('\n', eol);
    match (old.find(BLOCK_START), old.find(BLOCK_END)) {
        // Only the block itself is replaced, up to its end marker. What follows it, blank
        // lines and line endings included, is the repository's and stays byte for byte.
        (Some(start), Some(end)) if start < end => {
            format!("{}{inner}{}", &old[..start], &old[end + BLOCK_END.len()..])
        }
        _ if old.trim().is_empty() => format!("{inner}{eol}"),
        _ => format!(
            "{}{eol}{eol}{inner}{eol}",
            old.trim_end_matches(['\r', '\n'])
        ),
    }
}

/// The files Claude Code reads its instructions from, and `AGENTS.md`, which some
/// repositories load through one of them.
const INSTRUCTION_FILES: [&str; 3] = ["CLAUDE.md", ".claude/CLAUDE.md", "AGENTS.md"];

/// Which file takes the Commits section: the one that already has it, otherwise the
/// `CLAUDE.md` the repository already keeps, otherwise a new one at the root.
fn instructions_home(top: &Path) -> Result<&'static str> {
    for name in INSTRUCTION_FILES {
        if read_if_present(&top.join(name))?.is_some_and(|text| text.contains(BLOCK_START)) {
            return Ok(name);
        }
    }
    Ok(INSTRUCTION_FILES[..2]
        .iter()
        .copied()
        .find(|name| top.join(name).is_file())
        .unwrap_or("CLAUDE.md"))
}

/// The file a name stands for, relative to the repository: itself, or the file a link
/// inside the repository points to. `None` for a link that leads nowhere or outside.
fn behind_a_link(top: &Path, name: &str) -> Option<String> {
    let path = top.join(name);
    let is_link = std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_symlink());
    if !is_link {
        return Some(name.to_string());
    }
    let target = path.canonicalize().ok()?;
    let root = top.canonicalize().ok()?;
    target
        .strip_prefix(&root)
        .ok()
        .map(|inside| inside.to_string_lossy().replace('\\', "/"))
}

/// CLAUDE.md is loaded at the start of every Claude Code session.
fn write_claude_md(top: &Path, changed: &mut Vec<String>) -> Result<()> {
    let home = instructions_home(top)?;
    // Git records a link, not what it points to, so the file to stage is the target.
    let Some(name) = behind_a_link(top, home) else {
        println!(
            "   {home} is a link to a file outside this repository; left as it is. Add the Commits section to that file yourself"
        );
        return Ok(());
    };
    let path = top.join(&name);
    let old = read_if_present(&path)?.unwrap_or_default();
    let new = with_commits_section(&old);
    if new == old {
        println!("   {name} already has the Commits section");
    } else {
        write(&path, &new)?;
        println!("   wrote the Commits section in {name}");
        changed.push(name);
    }
    Ok(())
}

/// What became of the Claude Code settings.
#[derive(Debug, PartialEq)]
enum Attribution {
    /// The new text of the file, with the co-author line turned off.
    TurnedOff(String),
    /// The file already says something about attribution.
    AlreadySet,
    /// The file is not a JSON object.
    Unreadable,
}

/// Claude Code adds a co-author line to commits by default, and the hook refuses it.
fn without_coauthor(old: Option<&str>) -> Attribution {
    let mut settings = match old {
        None => Map::new(),
        Some(text) => match serde_json::from_str::<Value>(text) {
            Ok(Value::Object(map)) => map,
            _ => return Attribution::Unreadable,
        },
    };
    if settings.contains_key("attribution") || settings.contains_key("includeCoAuthoredBy") {
        return Attribution::AlreadySet;
    }
    settings.insert(
        "attribution".to_string(),
        json!({ "commit": "", "sessionUrl": false }),
    );
    match serde_json::to_string_pretty(&Value::Object(settings)) {
        Ok(text) => Attribution::TurnedOff(format!("{text}\n")),
        Err(_) => Attribution::Unreadable,
    }
}

fn write_claude_settings(top: &Path, changed: &mut Vec<String>) -> Result<()> {
    let path = top.join(".claude").join("settings.json");
    match without_coauthor(read_if_present(&path)?.as_deref()) {
        Attribution::TurnedOff(text) => {
            write(&path, &text)?;
            changed.push(".claude/settings.json".to_string());
            println!("   turned off Claude Code's co-author line in .claude/settings.json");
        }
        Attribution::AlreadySet => {
            println!("   .claude/settings.json already sets commit attribution; left as it is");
        }
        Attribution::Unreadable => println!(
            "   .claude/settings.json is not a JSON object; left as it is. Set attribution.commit to an empty string yourself"
        ),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("gitmoji-init-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn merge_hook_upgrades_the_generated_template_and_preserves_project_commands() {
        let top = scratch("upgrade-merge-hook");
        git(&top, &["init", "-q"]).unwrap();
        let config = top.join("lefthook.yml");
        write(&config, PREVIOUS_LEFTHOOK).unwrap();
        let mut changed = Vec::new();
        write_hooks(&top, &mut changed).unwrap();
        assert_eq!(std::fs::read_to_string(&config).unwrap(), LEFTHOOK);
        assert_eq!(changed, ["lefthook.yml"]);

        changed.clear();
        write_hooks(&top, &mut changed).unwrap();
        assert!(changed.is_empty());

        let custom = format!("{PREVIOUS_LEFTHOOK}# This project also runs its own checks.\n");
        write(&config, &custom).unwrap();
        write_hooks(&top, &mut changed).unwrap();
        assert_eq!(std::fs::read_to_string(&config).unwrap(), custom);
        assert!(changed.is_empty());
        std::fs::remove_dir_all(top).unwrap();
    }

    #[test]
    fn the_skill_is_written_once_and_keeps_what_others_put_there() {
        let top = scratch("skill");
        git(&top, &["init", "-q"]).unwrap();
        let paths = SkillPaths::new(&top).unwrap();
        let vocab = Vocabulary::builtin().unwrap();
        let mut changed = Vec::new();
        write_skill(&top, &paths, &vocab, &mut changed).unwrap();
        assert_eq!(changed.len(), SKILL.len() + 1);
        assert!(top.join(SKILL_DIR).join(REFERENCE).is_file());
        assert!(!top.join(SKILL_DIR).join("scripts").exists());

        let mut again = Vec::new();
        write_skill(&top, &paths, &vocab, &mut again).unwrap();
        assert!(again.is_empty());

        let skill = top.join(SKILL_DIR).join("SKILL.md");
        let note = top.join(SKILL_DIR).join("notes.txt");
        std::fs::write(&skill, "mine").unwrap();
        std::fs::write(&note, "mine too").unwrap();
        let mut third = Vec::new();
        write_skill(&top, &paths, &vocab, &mut third).unwrap();
        assert_eq!(third, vec![format!("{SKILL_DIR}/SKILL.md")]);
        let backup = top.join(SKILL_DIR).join("SKILL.md.before-update");
        assert_eq!(std::fs::read_to_string(backup).unwrap(), "mine");
        assert_eq!(std::fs::read_to_string(note).unwrap(), "mine too");
        std::fs::remove_dir_all(top).unwrap();
    }

    const RELEASED_SCRIPTS_README: &str = "# Skill scripts

Use the [PR guide](../references/pull-requests.md) for `github-merge.py` and the
[history upgrade guide](../references/upgrade-history.md) for `gitmoji-upgrade-history.py`.

## Signature scripts, one per harness

A commit made by an agent ends with a signature that says what went into it:

```
_
<model> (<effort>), <agent name, only if a person gave one>
<harness> (<session id, if there is one>)
Sub-agents: <count> × <model> (<effort>), ...      only if sub-agents did part of the work
Team: <other agents>                               only if it exchanged messages with others
Research: <n> searches, <n> pages, <n> doc lookups only if it looked something up
```

The scripts print the underscore. When the commit is made, the hook replaces it by `△` and,
between backticks, the version of the vocabulary the message was checked against.

Every value comes from the harness's own record of the session, never from the agent's
memory. An agent can be wrong about its own model and effort, and a sub-agent can run on
a different model than the agent that started it. The last three lines cover the time
since the session's last commit in the repository. On its first commit here, that
window can include earlier tasks or work in another checkout. Check optional counts
against the work in this commit.

Each signature script prints that block for one harness. Run yours and paste the output at the
end of the commit body:

| Harness | Script | Where the values come from |
|---|---|---|
| Claude Code | `claude-code.py` | `CLAUDE_EFFORT`, `CLAUDE_CODE_SESSION_ID`, and the session log under `~/.claude/projects/`: model per reply, `custom-title.json`, `subagents/`, and tool calls for team and research. A shell command such as `curl` is not counted as a page read |
| Codex | `codex.py` | `CODEX_THREAD_ID` (or `CODEX_SESSION_ID`) selects the rollout in `$CODEX_HOME` or `~/.codex`. Turn contexts give model and effort, a naming message gives the name, and spawn records identify direct helpers. Native sends and agmsg receipts give teammates. Completed web actions, Context7 docs commands and documentation tool calls give research counts for the parent and helpers. |
| Antigravity | `antigravity.py` | `ANTIGRAVITY_CONVERSATION_ID`, `ANTIGRAVITY_APP_DATA_DIR` (`transcript_full.jsonl` model/effort/team/research, `history.jsonl` `/rename`, and `conversation_summaries.db` sub-agents) |

## If your harness is not listed

Work it out once, then add a script here so the next agent does not have to.

1. Find where your harness records, for this session: the model of each reply, the
   effort level, the session id, the name a person gave you, and each sub-agent you
   started with its model and effort. Look at environment variables, the session or
   transcript log, and per-turn context records.
2. Write a script that prints the block above from those records. It must not take the
   values as arguments or guess a default. When a value cannot be found, it prints
   `unknown` in its place.
3. Cover the time since your session's last commit in this repository. Count the
   sub-agents that ran, grouped by model and effort. List the other agents you sent
   messages to. Count web searches, web pages read and documentation lookups, yours and
   your sub-agents'. Leave out a line when there is nothing to report.
4. Add a row to the table, and say in one line where each value comes from.
";

    #[test]
    fn upgrade_retires_shipped_scripts_and_preserves_modified_or_user_files() {
        let top = scratch("retire-scripts");
        git(&top, &["init", "-q"]).unwrap();
        let skill = top.join(SKILL_DIR);
        let readme_crlf = RELEASED_SCRIPTS_README.replace('\n', "\r\n");
        assert!(is_released_script(
            "scripts/README.md",
            readme_crlf.as_bytes()
        ));
        assert!(!is_released_script(
            "scripts/README.md",
            b"# custom downstream README\n"
        ));

        write(&skill.join("scripts/README.md"), &readme_crlf).unwrap();
        write(
            &skill.join("scripts/github-merge.py"),
            "# committed downstream customization\n",
        )
        .unwrap();
        write(
            &skill.join("scripts/codex.py"),
            "# uncommitted worktree edit target\n",
        )
        .unwrap();
        write(
            &skill.join("scripts/claude-code.py"),
            "# staged edit target\n",
        )
        .unwrap();
        write(
            &skill.join("scripts/antigravity.py"),
            "# untracked target\n",
        )
        .unwrap();
        write(&skill.join("scripts/custom.py"), "user helper\n").unwrap();
        git(&top, &["add", SKILL_DIR]).unwrap();
        git(
            &top,
            &[
                "-c",
                "user.name=T",
                "-c",
                "user.email=t@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-qm",
                "previous layout",
            ],
        )
        .unwrap();

        // Modified tracked, staged, and untracked scripts stay untouched.
        let modified = "scripts/codex.py";
        let staged_script = "scripts/claude-code.py";
        let untracked = "scripts/antigravity.py";
        let customized = "scripts/github-merge.py";
        write(&skill.join(modified), "local edit\n").unwrap();
        write(&skill.join(staged_script), "staged edit\n").unwrap();
        git(&top, &["add", &format!("{SKILL_DIR}/{staged_script}")]).unwrap();
        git(
            &top,
            &["rm", "-q", "--cached", &format!("{SKILL_DIR}/{untracked}")],
        )
        .unwrap();

        let paths = SkillPaths::new(&top).unwrap();
        let mut changed = Vec::new();
        rename_earlier(&top, &paths, &mut changed).unwrap();

        assert!(!skill.join("scripts/README.md").exists());
        assert_eq!(changed, vec![format!("{SKILL_DIR}/scripts/README.md")]);
        assert_eq!(
            std::fs::read_to_string(skill.join(customized)).unwrap(),
            "# committed downstream customization\n"
        );
        assert_eq!(
            std::fs::read_to_string(skill.join("scripts/custom.py")).unwrap(),
            "user helper\n"
        );
        assert_eq!(
            std::fs::read_to_string(skill.join(modified)).unwrap(),
            "local edit\n"
        );
        assert_eq!(
            std::fs::read_to_string(skill.join(staged_script)).unwrap(),
            "staged edit\n"
        );
        assert_eq!(
            std::fs::read_to_string(skill.join(untracked)).unwrap(),
            "# untracked target\n"
        );
        assert!(skill.join("scripts").is_dir());

        // When only released scripts remain in an empty folder, the folder is removed too.
        let clean_only = scratch("retire-scripts-clean");
        git(&clean_only, &["init", "-q"]).unwrap();
        write(
            &clean_only.join(SKILL_DIR).join("scripts/README.md"),
            RELEASED_SCRIPTS_README,
        )
        .unwrap();
        git(&clean_only, &["add", SKILL_DIR]).unwrap();
        git(
            &clean_only,
            &[
                "-c",
                "user.name=T",
                "-c",
                "user.email=t@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-qm",
                "clean released script",
            ],
        )
        .unwrap();
        let mut clean_changed = Vec::new();
        let clean_paths = SkillPaths::new(&clean_only).unwrap();
        rename_earlier(&clean_only, &clean_paths, &mut clean_changed).unwrap();
        assert!(!clean_only.join(SKILL_DIR).join("scripts").exists());
        assert_eq!(
            clean_changed,
            vec![format!("{SKILL_DIR}/scripts/README.md")]
        );
        std::fs::remove_dir_all(clean_only).unwrap();
        std::fs::remove_dir_all(top).unwrap();
    }

    #[test]
    fn the_commits_section_goes_where_the_repository_keeps_its_instructions() {
        let top = scratch("instructions");
        assert_eq!(instructions_home(&top).unwrap(), "CLAUDE.md");
        std::fs::create_dir_all(top.join(".claude")).unwrap();
        std::fs::write(top.join(".claude/CLAUDE.md"), "# Project\n").unwrap();
        assert_eq!(instructions_home(&top).unwrap(), ".claude/CLAUDE.md");
        let mut changed = Vec::new();
        write_claude_md(&top, &mut changed).unwrap();
        assert_eq!(changed, vec![".claude/CLAUDE.md".to_string()]);
        assert!(!top.join("CLAUDE.md").exists());
        // The section is found again wherever it was put, also in AGENTS.md.
        std::fs::write(top.join("CLAUDE.md"), "@AGENTS.md\n").unwrap();
        assert_eq!(instructions_home(&top).unwrap(), ".claude/CLAUDE.md");
        std::fs::remove_file(top.join(".claude/CLAUDE.md")).unwrap();
        std::fs::write(
            top.join("AGENTS.md"),
            format!("# Agents\n\n{BLOCK_START}\nold\n{BLOCK_END}\n"),
        )
        .unwrap();
        assert_eq!(instructions_home(&top).unwrap(), "AGENTS.md");
        std::fs::remove_dir_all(top).unwrap();
    }

    #[test]
    fn leftovers_of_an_earlier_version_are_pointed_out() {
        let top = scratch("leftovers");
        assert!(leftovers(&top).unwrap().is_empty());
        std::fs::write(
            top.join("gitmoji.toml"),
            "[rules]\nsubject_max = 80   # characters\n",
        )
        .unwrap();
        std::fs::write(
            top.join("lefthook.yml"),
            "commit-msg:\n  commands:\n    gitmoji-check:\n      run: gitmoji --config .config/gitmoji.toml check --fix {1}\n",
        )
        .unwrap();
        std::fs::create_dir_all(top.join(".config")).unwrap();
        std::fs::write(top.join(".config/gitmoji.toml"), "").unwrap();
        let found = leftovers(&top).unwrap();
        // The root file is the one gitmoji finds, so naming another is on purpose.
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].starts_with("gitmoji.toml sets subject_max = 80"));
        std::fs::remove_file(top.join("gitmoji.toml")).unwrap();
        let found = leftovers(&top).unwrap();
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].starts_with("lefthook.yml passes --config .config/gitmoji.toml"));
        // A file of the project's own choosing, and a comment, are left without advice.
        for run in [
            "      run: gitmoji --config tools/company.toml check --fix {1}\n",
            "      # run: gitmoji --config .config/gitmoji.toml check --fix {1}\n      run: gitmoji check --fix {1}\n",
        ] {
            std::fs::write(top.join("lefthook.yml"), run).unwrap();
            assert!(leftovers(&top).unwrap().is_empty(), "{run}");
        }
        assert!(
            gitmoji_commands("run: gitmoji --config tools/company.toml check --fix {1}")
                .any(|words| words.contains(&"check"))
        );
        assert!(!gitmoji_commands("# run: gitmoji check").any(|words| words.contains(&"check")));
        std::fs::write(
            top.join("lefthook.yml"),
            "      run: gitmoji check --fix {1}  # was: gitmoji --config .config/gitmoji.toml check\n",
        )
        .unwrap();
        assert!(leftovers(&top).unwrap().is_empty());
        std::fs::remove_dir_all(top).unwrap();
    }

    #[test]
    fn the_reference_in_the_skill_folder_is_the_one_this_command_writes() {
        let vocab = Vocabulary::builtin().unwrap();
        assert_eq!(
            include_str!("../../skills/gitmoji/references/vocabulary.txt"),
            guide::all(&vocab)
        );
    }

    #[test]
    fn workflow_install_is_optional_and_refreshes_both_pins_from_repository_settings() {
        let top = scratch("workflows");
        git(&top, &["init", "-q"]).unwrap();
        let paths = SkillPaths::new(&top).unwrap();
        let mut changed = Vec::new();
        write_workflows(&top, &paths, false, &mut changed).unwrap();
        assert!(!top.join(".github").exists());

        let title = WORKFLOWS[0].path;
        let previous = "name: Project title workflow\n";
        let generic = WORKFLOWS[0].legacy_path;
        write(&top.join(generic), previous).unwrap();
        write(
            &top.join(".config/gitmoji.toml"),
            "[vocabulary]\nversion='0.17.0'",
        )
        .unwrap();
        write_workflows(&top, &paths, true, &mut changed).unwrap();
        assert_eq!(changed, [title, WORKFLOWS[1].path]);
        assert_eq!(
            std::fs::read_to_string(top.join(generic)).unwrap(),
            previous
        );
        for workflow in WORKFLOWS {
            assert_eq!(
                std::fs::read_to_string(top.join(workflow.path)).unwrap(),
                pin_workflow(workflow.template, "0.17.0")
            );
        }

        changed.clear();
        write(&top.join("gitmoji.toml"), "[vocabulary]\nversion='1.0.1'").unwrap();
        write_workflows(&top, &paths, false, &mut changed).unwrap();
        assert_eq!(changed.len(), 2);
        for workflow in WORKFLOWS {
            assert!(
                std::fs::read_to_string(top.join(workflow.path))
                    .unwrap()
                    .contains("GITMOJI_VERSION: '1.0.1'")
            );
        }
        changed.clear();
        write_workflows(&top, &paths, false, &mut changed).unwrap();
        assert!(changed.is_empty());
        std::fs::remove_dir_all(top).unwrap();
    }

    #[test]
    fn plain_init_retires_only_enrolled_legacy_workflows_and_preserves_edits() {
        let top = scratch("workflow-rename");
        git(&top, &["init", "-q"]).unwrap();
        write(&top.join("gitmoji.toml"), "[vocabulary]\nversion='0.17.1'").unwrap();
        let title = &WORKFLOWS[0];
        let check = &WORKFLOWS[1];
        let customized = format!("{WORKFLOW_MARKER}\nname: extra project steps\n");
        write(&top.join(title.legacy_path), &customized).unwrap();
        let old_check = pin_workflow(check.template, "0.17.0")
            .replace("\n  gitmoji-commits:\n", "\n  check:\n");
        write(&top.join(check.legacy_path), &old_check).unwrap();
        git(&top, &["add", ".github"]).unwrap();
        git(
            &top,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-qm",
                "old workflows",
            ],
        )
        .unwrap();

        let paths = SkillPaths::new(&top).unwrap();
        let mut changed = Vec::new();
        write_workflows(&top, &paths, false, &mut changed).unwrap();
        assert_eq!(
            changed,
            [title.path, title.legacy_path, check.path, check.legacy_path]
        );
        for workflow in WORKFLOWS {
            let installed = std::fs::read_to_string(top.join(workflow.path)).unwrap();
            assert!(installed.contains(&format!("\njobs:\n  {}:\n", workflow.job)));
            assert!(installed.contains("GITMOJI_VERSION: '0.17.1'"));
            assert!(!top.join(workflow.legacy_path).exists());
        }
        assert_eq!(
            std::fs::read_to_string(top.join(format!("{}.before-update", title.legacy_path)))
                .unwrap(),
            customized
        );
        assert!(
            !top.join(format!("{}.before-update", check.legacy_path))
                .exists()
        );
        changed.clear();
        write_workflows(&top, &paths, false, &mut changed).unwrap();
        assert!(changed.is_empty());
        std::fs::remove_dir_all(top).unwrap();
    }

    #[test]
    fn release_upgrade_backs_up_only_edits_to_the_previous_title_workflow() {
        let previous = include_str!("../tests/fixtures/pr-title-0.16.yml");
        let title = &WORKFLOWS[0];
        for (version, path, customized) in [
            ("0.15.0", title.legacy_path, false),
            ("0.15.1", title.path, false),
            ("0.16.0", title.path, false),
            ("0.16.0", title.path, true),
        ] {
            let top = scratch(&format!("release-backup-{version}-{customized}"));
            git(&top, &["init", "-q"]).unwrap();
            write(&top.join("gitmoji.toml"), "[vocabulary]\nversion='0.17.1'").unwrap();
            let mut old = pin_workflow(previous, version);
            if path == title.legacy_path {
                old = old.replace("\n  gitmoji-pr-title:\n", "\n  title:\n");
            }
            if customized {
                old.push_str("# project change\n");
            }
            let old = old.replace('\n', "\r\n");
            write(&top.join(path), &old).unwrap();
            git(&top, &["add", "."]).unwrap();
            git(
                &top,
                &[
                    "-c",
                    "user.name=Test",
                    "-c",
                    "user.email=test@example.com",
                    "-c",
                    "commit.gpgsign=false",
                    "commit",
                    "-qm",
                    "earlier release",
                ],
            )
            .unwrap();
            let paths = SkillPaths::new(&top).unwrap();
            let mut changed = Vec::new();
            write_workflows(&top, &paths, false, &mut changed).unwrap();
            assert!(changed.iter().any(|entry| entry == title.path));
            let backup = top.join(format!("{path}.before-update"));
            assert_eq!(backup.exists(), customized);
            if customized {
                assert_eq!(std::fs::read_to_string(&backup).unwrap(), old);
            }
            changed.clear();
            write_workflows(&top, &paths, false, &mut changed).unwrap();
            assert!(changed.is_empty());
            std::fs::remove_dir_all(top).unwrap();
        }
    }

    #[test]
    fn workflow_migration_respects_project_files_index_and_existing_backups() {
        let top = scratch("workflow-collisions");
        git(&top, &["init", "-q"]).unwrap();
        write(&top.join("gitmoji.toml"), "[vocabulary]\nversion='0.17.1'").unwrap();
        let title = &WORKFLOWS[0];
        let check = &WORKFLOWS[1];
        let customized = format!("{WORKFLOW_MARKER}\nname: custom\n");
        write(&top.join(title.legacy_path), &customized).unwrap();
        write(&top.join(check.legacy_path), &customized).unwrap();
        write(&top.join(title.path), "name: project owned\n").unwrap();
        git(&top, &["add", check.legacy_path]).unwrap();
        let paths = SkillPaths::new(&top).unwrap();
        let mut changed = Vec::new();
        for requested in [false, true] {
            write_workflows(&top, &paths, requested, &mut changed).unwrap();
            assert!(changed.is_empty());
            assert_eq!(
                std::fs::read_to_string(top.join(title.path)).unwrap(),
                "name: project owned\n"
            );
            assert!(!top.join(check.path).exists());
        }
        git(&top, &["rm", "--cached", check.legacy_path]).unwrap();
        let backup = top.join(format!("{}.before-update", check.legacy_path));
        write(&backup, "keep my backup").unwrap();
        write_workflows(&top, &paths, false, &mut changed).unwrap();
        assert_eq!(std::fs::read_to_string(backup).unwrap(), "keep my backup");
        for workflow in WORKFLOWS {
            assert_eq!(
                std::fs::read_to_string(top.join(workflow.legacy_path)).unwrap(),
                customized
            );
        }
        assert!(changed.is_empty());

        // A previous interrupted run left exactly the backup this update needs.
        write(
            &top.join(format!("{}.before-update", check.legacy_path)),
            &customized,
        )
        .unwrap();
        write_workflows(&top, &paths, false, &mut changed).unwrap();
        assert_eq!(changed, [check.path]);
        assert!(!top.join(check.legacy_path).exists());
        assert!(!create_workflow_file(&top.join(title.path), "replacement").unwrap());
        assert_eq!(
            std::fs::read_to_string(top.join(title.path)).unwrap(),
            "name: project owned\n"
        );
        std::fs::remove_dir_all(top).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn workflow_migration_leaves_symbolic_links_and_their_targets_alone() {
        use std::os::unix::fs::symlink;
        let top = scratch("workflow-symlinks");
        git(&top, &["init", "-q"]).unwrap();
        write(&top.join("gitmoji.toml"), "[vocabulary]\nversion='0.17.1'").unwrap();
        let paths = SkillPaths::new(&top).unwrap();
        let mut changed = Vec::new();
        let title = &WORKFLOWS[0];
        let target = top.join("target.txt");
        let saved = format!("{WORKFLOW_MARKER}\nname: saved\n");
        write(&target, &saved).unwrap();
        write(&top.join(title.legacy_path), &saved).unwrap();
        symlink(&target, top.join(title.path)).unwrap();
        write_workflows(&top, &paths, false, &mut changed).unwrap();
        assert!(top.join(title.legacy_path).exists());
        assert!(top.join(title.path).is_symlink());
        assert_eq!(std::fs::read_to_string(target).unwrap(), saved);
        assert!(changed.is_empty());
        std::fs::remove_dir_all(top).unwrap();
    }

    #[test]
    fn workflow_install_rejects_invalid_release_pins_and_respects_staged_files() {
        let top = scratch("workflow-conflicts");
        git(&top, &["init", "-q"]).unwrap();
        let title = WORKFLOWS[0].path;
        write(&top.join(title), "name: staged workflow\n").unwrap();
        git(&top, &["add", "--", title]).unwrap();
        let paths = SkillPaths::new(&top).unwrap();
        let mut changed = Vec::new();
        for bad in [
            "1.2.3'\nmangled",
            "trunk",
            "0.16.0",
            "0.13.3",
            "1.2",
            "+1.2.3",
        ] {
            write(
                &top.join("gitmoji.toml"),
                &format!("[vocabulary]\nversion={bad:?}"),
            )
            .unwrap();
            assert!(write_workflows(&top, &paths, true, &mut changed).is_err());
            assert!(!top.join(WORKFLOWS[1].path).exists());
        }
        write(&top.join("gitmoji.toml"), "[vocabulary]\nversion='0.17.0'").unwrap();
        write_workflows(&top, &paths, true, &mut changed).unwrap();
        assert_eq!(changed, [WORKFLOWS[1].path]);
        assert_eq!(
            std::fs::read_to_string(top.join(title)).unwrap(),
            "name: staged workflow\n"
        );
        std::fs::remove_dir_all(top).unwrap();
    }

    #[test]
    fn init_preserves_skill_and_workflows_when_repo_pins_older_version() {
        let top = scratch("older-pin");
        git(&top, &["init", "-q"]).unwrap();
        write(
            &top.join("gitmoji.toml"),
            "[vocabulary]\nversion = \"0.16.0\"\n",
        )
        .unwrap();
        let old_skill = "old 0.16 skill\n";
        let old_script = "old 0.16 script\n";
        let old_workflow = format!("{WORKFLOW_MARKER}\nname: old 0.16 workflow\n");
        write(&top.join(SKILL_DIR).join("SKILL.md"), old_skill).unwrap();
        write(
            &top.join(SKILL_DIR).join("scripts/claude-code.py"),
            old_script,
        )
        .unwrap();
        write(&top.join(WORKFLOWS[0].path), &old_workflow).unwrap();
        write(&top.join("lefthook.yml"), LEFTHOOK).unwrap();
        write(&top.join("CLAUDE.md"), &with_commits_section("")).unwrap();
        write(
            &top.join(".claude/settings.json"),
            "{ \"attribution\": { \"commit\": \"\" } }\n",
        )
        .unwrap();
        git(&top, &["add", "."]).unwrap();
        git(
            &top,
            &[
                "-c",
                "user.name=T",
                "-c",
                "user.email=t@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-qm",
                "pinned 0.16",
            ],
        )
        .unwrap();

        let vocab = Vocabulary::builtin().unwrap();
        run(&vocab, &top, Tickets::Github, false).unwrap();
        assert_eq!(
            std::fs::read_to_string(top.join(SKILL_DIR).join("SKILL.md")).unwrap(),
            old_skill
        );
        assert_eq!(
            std::fs::read_to_string(top.join(SKILL_DIR).join("scripts/claude-code.py")).unwrap(),
            old_script
        );
        assert_eq!(
            std::fs::read_to_string(top.join(WORKFLOWS[0].path)).unwrap(),
            old_workflow
        );
        assert!(run(&vocab, &top, Tickets::Github, true).is_err());

        // Explicit --github-workflows on an older pin fails before touching hooks or Claude files.
        let clean = scratch("older-pin-fail-early");
        git(&clean, &["init", "-q"]).unwrap();
        write(
            &clean.join("gitmoji.toml"),
            "[vocabulary]\nversion = \"0.16.0\"\n",
        )
        .unwrap();
        write(&clean.join(SKILL_DIR).join("SKILL.md"), old_skill).unwrap();
        assert!(run(&vocab, &clean, Tickets::Github, true).is_err());
        assert!(!clean.join("lefthook.yml").exists());
        assert!(!clean.join("CLAUDE.md").exists());
        assert!(!clean.join(".claude/settings.json").exists());
        assert!(!clean.join(".github").exists());
        assert_eq!(
            std::fs::read_to_string(clean.join(SKILL_DIR).join("SKILL.md")).unwrap(),
            old_skill
        );

        std::fs::remove_dir_all(clean).unwrap();
        std::fs::remove_dir_all(top).unwrap();
    }

    #[test]
    fn fresh_init_writes_scriptless_skill_and_current_pin() {
        let top = scratch("fresh-init");
        git(&top, &["init", "-q"]).unwrap();
        let vocab = Vocabulary::builtin().unwrap();
        run(&vocab, &top, Tickets::Github, false).unwrap();

        let settings = std::fs::read_to_string(top.join("gitmoji.toml")).unwrap();
        assert!(settings.contains(&format!("version = \"{}\"", vocab.version)));
        assert!(top.join(SKILL_DIR).join("SKILL.md").is_file());
        assert!(top.join(SKILL_DIR).join(REFERENCE).is_file());
        assert!(
            top.join(SKILL_DIR)
                .join("references/pull-requests.md")
                .is_file()
        );
        assert!(
            top.join(SKILL_DIR)
                .join("references/upgrade-history.md")
                .is_file()
        );
        assert!(
            top.join(SKILL_DIR)
                .join("references/dependabot.md")
                .is_file()
        );
        assert!(!top.join(SKILL_DIR).join("scripts").exists());
        std::fs::remove_dir_all(top).unwrap();
    }

    #[test]
    fn commit_suggestion_distinguishes_adoption_skill_import_and_config_changes() {
        let top = scratch("commit-suggestion");
        let vocab = Vocabulary::builtin().unwrap();

        let import = existing_repo_commit_guidance(
            &top,
            &vocab,
            &[".claude/skills/gitmoji/SKILL.md".to_string()],
            &[],
        )
        .join("\n");
        assert!(import.contains(&format!(
            "🤖📥 <ticket> (gitmoji): import the gitmoji skill from v{}",
            vocab.version
        )));
        assert!(import.contains(&format!(
            "Source: https://github.com/D1no/gitmoji/releases/tag/v{}",
            vocab.version
        )));
        assert!(!import.contains("🤖📝"));
        assert!(!import.contains("Project-owned configuration"));

        write(
            &top.join("gitmoji.toml"),
            "[scope]\nallowed = [\"repo\", \"ci\"]\n",
        )
        .unwrap();
        let scoped = existing_repo_commit_guidance(
            &top,
            &vocab,
            &[
                ".claude/skills/gitmoji/SKILL.md".to_string(),
                ".github/workflows/gitmoji-commits.yml".to_string(),
            ],
            &[],
        )
        .join("\n");
        assert!(scoped.contains("🤖📥 <ticket> (<scope>): import the gitmoji skill and workflows"));
        assert!(scoped.contains("allowed scopes in place of <scope>: repo, ci"));

        let config_only = existing_repo_commit_guidance(
            &top,
            &vocab,
            &[],
            &["CLAUDE.md".to_string(), "lefthook.yml".to_string()],
        )
        .join("\n");
        assert!(!config_only.contains("🤖📥"));
        assert!(!config_only.contains("🤖📝"));
        assert!(config_only.contains("CLAUDE.md"));
        assert!(config_only.contains("lefthook.yml"));
        assert!(config_only.contains("Pick the intent that fits those files"));

        let mixed = existing_repo_commit_guidance(
            &top,
            &vocab,
            &[".claude/skills/gitmoji/SKILL.md".to_string()],
            &["CLAUDE.md".to_string(), "lefthook.yml".to_string()],
        )
        .join("\n");
        let (first, second) = mixed
            .split_once("2. Project-owned configuration, hooks, or instructions:")
            .unwrap();
        assert!(first.contains(".claude/skills/gitmoji/SKILL.md"));
        assert!(!first.contains("CLAUDE.md"));
        assert!(!first.contains("lefthook.yml"));
        assert!(first.contains("🤖📥 <ticket> (<scope>): import the gitmoji skill"));
        assert!(second.contains("CLAUDE.md"));
        assert!(second.contains("lefthook.yml"));
        assert!(!second.contains(".claude/skills/gitmoji/SKILL.md"));
        assert!(!second.contains("🤖📥"));

        std::fs::remove_dir_all(top).unwrap();
    }

    #[test]
    fn settings_are_written_once_with_the_adopted_version() {
        let top = scratch("settings");
        let vocab = Vocabulary::builtin().unwrap();
        let mut changed = Vec::new();
        assert!(write_settings(&top, &vocab, Tickets::Backlog, &mut changed).unwrap());
        let text = std::fs::read_to_string(top.join("gitmoji.toml")).unwrap();
        assert!(text.contains("TASK-"));
        assert!(text.contains(&format!("version = \"{}\"", vocab.version)));
        gitmoji_triangle::config::Config::from_toml(&text).unwrap();
        assert!(!write_settings(&top, &vocab, Tickets::Github, &mut changed).unwrap());
        assert_eq!(changed, vec!["gitmoji.toml".to_string()]);
        std::fs::remove_dir_all(top).unwrap();
    }

    #[test]
    fn default_settings_are_not_written_over_a_pending_rename() {
        let top = scratch("pending");
        let vocab = Vocabulary::builtin().unwrap();
        std::fs::write(top.join(EARLIER_SETTINGS), "[rules]\nsubject_max = 96\n").unwrap();
        let mut changed = Vec::new();
        assert!(!write_settings(&top, &vocab, Tickets::Github, &mut changed).unwrap());
        assert!(changed.is_empty());
        assert!(!top.join("gitmoji.toml").exists());
        std::fs::remove_dir_all(top).unwrap();
    }

    #[test]
    fn the_commits_section_is_added_once_and_replaced_in_place() {
        let fresh = with_commits_section("");
        assert!(fresh.starts_with(BLOCK_START));
        assert_eq!(with_commits_section(&fresh), fresh);

        let added = with_commits_section("# Project\n\nNotes.\n");
        assert!(added.starts_with("# Project\n\nNotes.\n\n<!-- gitmoji-triangle:start -->"));
        assert_eq!(with_commits_section(&added), added);

        let stale = format!("# Project\n\n{BLOCK_START}\nold words\n{BLOCK_END}\n\n## After\n");
        let updated = with_commits_section(&stale);
        assert!(!updated.contains("old words"));
        assert!(updated.starts_with("# Project\n\n"));
        // The blank line between the block and the next heading stays.
        assert!(
            updated.ends_with(&format!("{BLOCK_END}\n\n## After\n")),
            "{updated}"
        );
        assert_eq!(with_commits_section(&updated), updated);
        // And so does its absence, and three of them, and a file that ends at the marker.
        for after in ["\n## After\n", "\n\n\n\n## After\n", ""] {
            let file = format!("# Project\n\n{BLOCK_START}\nold\n{BLOCK_END}{after}");
            let once = with_commits_section(&file);
            assert!(once.ends_with(&format!("{BLOCK_END}{after}")), "{after:?}");
            assert_eq!(with_commits_section(&once), once, "{after:?}");
        }
        // A file with Windows line endings keeps them, inside the block and after it.
        let crlf =
            format!("# Project\r\n\r\n{BLOCK_START}\r\nold\r\n{BLOCK_END}\r\n\r\n## After\r\n");
        let once = with_commits_section(&crlf);
        assert!(!once.replace("\r\n", "").contains('\n'), "{once:?}");
        assert!(once.ends_with(&format!("{BLOCK_END}\r\n\r\n## After\r\n")));
        assert_eq!(with_commits_section(&once), once);
        let added = with_commits_section("# Project\r\n");
        assert!(!added.replace("\r\n", "").contains('\n'), "{added:?}");
    }

    #[test]
    fn the_coauthor_line_is_turned_off_without_touching_other_settings() {
        let Attribution::TurnedOff(fresh) = without_coauthor(None) else {
            panic!("a missing file gets the setting");
        };
        assert!(fresh.contains("\"commit\": \"\""));

        let Attribution::TurnedOff(kept) =
            without_coauthor(Some("{\"zeta\": 1, \"alpha\": {\"x\": true}}"))
        else {
            panic!("other settings do not stand in the way");
        };
        assert!(kept.find("zeta").unwrap() < kept.find("alpha").unwrap());
        assert!(kept.contains("attribution"));

        assert_eq!(
            without_coauthor(Some("{\"attribution\": {\"commit\": \"x\"}}")),
            Attribution::AlreadySet
        );
        assert_eq!(
            without_coauthor(Some("{\"includeCoAuthoredBy\": true}")),
            Attribution::AlreadySet
        );
        assert_eq!(without_coauthor(Some("not json")), Attribution::Unreadable);
        assert_eq!(without_coauthor(Some("[1]")), Attribution::Unreadable);
    }
}
