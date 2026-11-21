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

const SKILL_DIR: &str = ".claude/skills/gitmoji";

/// The reference inside the skill, written from the vocabulary so that it cannot differ
/// from what the command says.
const REFERENCE: &str = "references/vocabulary.txt";

/// The skill's other files, as they stand in `skills/gitmoji` of this repository.
const SKILL: &[(&str, &str)] = &[
    ("SKILL.md", include_str!("../../skills/gitmoji/SKILL.md")),
    (
        "scripts/gitmoji-upgrade-history.py",
        include_str!("../../skills/gitmoji/scripts/gitmoji-upgrade-history.py"),
    ),
    (
        "scripts/github-pr-title.py",
        include_str!("../../skills/gitmoji/scripts/github-pr-title.py"),
    ),
    (
        "scripts/github-merge.py",
        include_str!("../../skills/gitmoji/scripts/github-merge.py"),
    ),
    (
        "references/upgrade-history.md",
        include_str!("../../skills/gitmoji/references/upgrade-history.md"),
    ),
    (
        "references/pull-requests.md",
        include_str!("../../skills/gitmoji/references/pull-requests.md"),
    ),
    (
        "scripts/README.md",
        include_str!("../../skills/gitmoji/scripts/README.md"),
    ),
    (
        "scripts/antigravity.py",
        include_str!("../../skills/gitmoji/scripts/antigravity.py"),
    ),
    (
        "scripts/claude-code.py",
        include_str!("../../skills/gitmoji/scripts/claude-code.py"),
    ),
    (
        "scripts/codex.py",
        include_str!("../../skills/gitmoji/scripts/codex.py"),
    ),
];

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
pub fn run(vocab: &Vocabulary, path: &Path, tickets: Tickets) -> Result<()> {
    let top = repository_root(path)?;
    // Every file this run wrote or changed, relative to the repository. Only these are
    // offered for the commit: never a backup, a file someone added, or a file left alone.
    let mut changed = Vec::new();

    let paths = SkillPaths::new(&top)?;
    rename_earlier(&top, &paths, &mut changed)?;

    println!("1. The skill, in {SKILL_DIR}");
    write_skill(&top, &paths, vocab, &mut changed)?;

    println!("2. Settings");
    let adopted = write_settings(&top, vocab, tickets, &mut changed)?;

    println!("3. Git hooks");
    let lefthook_missing = write_hooks(&top, &mut changed)?;

    println!("4. Claude Code");
    write_claude_md(&top, &mut changed)?;
    write_claude_settings(&top, &mut changed)?;

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
    } else {
        println!("Done. This run wrote or changed these files. Commit exactly these:");
        println!();
        println!("  cd {}", shell_path(&top.to_string_lossy()));
        println!("  git --literal-pathspecs add -- \\");
        for (index, file) in changed.iter().enumerate() {
            let more = if index + 1 < changed.len() { " \\" } else { "" };
            println!("    {}{more}", shell_path(file));
        }
        if adopted {
            println!(
                "  git commit -m '🔺 gitmoji `{}`: adopt the Triangle convention'",
                vocab.version
            );
        } else {
            println!(
                "  git commit -m '🤖📝 <ticket> (gitmoji): update the gitmoji skill and settings' \\"
            );
            println!(
                "    -m 'Brings them in line with gitmoji {}.'",
                vocab.version
            );
            println!();
            println!(
                "Put the ticket this update belongs to in place of <ticket>, written like {}.",
                ticket_example(&top)
            );
            println!(
                "What changed in this version: {}",
                gitmoji_triangle::CHANGELOG
            );
            println!(
                "To adopt this release, set its version in gitmoji.toml and commit that with:"
            );
            println!(
                "  🔺 gitmoji `{}`: <what changes for this repository>",
                vocab.version
            );
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
            ["commit-msg", "pre-commit"].map(|hook| (format!("the {hook} hook"), folder.join(hook)))
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
struct SkillPaths {
    root: PathBuf,
    staged_entries: HashSet<Vec<u8>>,
}

impl SkillPaths {
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
    fn resolve(&self, name: &str) -> Result<String> {
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
    // SKILL.md. Those that git can give back go; anything else is left and named.
    for name in EARLIER_SKILL_FILES
        .iter()
        .copied()
        .filter(|name| *name != "SKILL.md")
    {
        if top.join(&current_dir).join(name).is_symlink() {
            continue;
        }
        let path = paths.resolve(&format!("{SKILL_DIR}/{name}"))?;
        let file = top.join(&path);
        if !file.is_file() {
            continue;
        }
        if tracked(top, &path) && !staged(top, &path)? && same_as_committed(top, &path) {
            std::fs::remove_file(&file)
                .with_context(|| format!("cannot remove {}", file.display()))?;
            changed.push(path.clone());
            say(format!(
                "removed {path}; the skill keeps it under scripts/ or references/ now"
            ));
        } else {
            say(format!(
                "{path} is from an earlier layout and is not as committed; left as it is. Delete it when you no longer need it"
            ));
        }
    }
    let _ = std::fs::remove_dir(top.join(&current_dir).join("harness"));

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

fn write_skill_file(to: &Path, name: &str, content: &str) -> Result<()> {
    write(to, content)?;
    if name.ends_with(".py") {
        make_executable(to)?;
    }
    Ok(())
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
    let files = SKILL
        .iter()
        .copied()
        .chain(std::iter::once((REFERENCE, reference.as_str())));
    for (name, content) in files {
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
            write_skill_file(&to, name, content)?;
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
        write_skill_file(&to, name, content)?;
        changed.push(path);
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
    ];
    let existing = LEFTHOOK_FILES
        .iter()
        .copied()
        .find(|name| top.join(name).exists());
    if let Some(name) = existing {
        let text = read_if_present(&top.join(name))?.unwrap_or_default();
        if !gitmoji_commands(&text).any(|words| words.contains(&"check")) {
            println!("   {name} exists without the gitmoji commands; left as it is.");
            println!("   Add these to it, then run `lefthook install`:");
            for line in LEFTHOOK.lines().filter(|line| !line.starts_with('#')) {
                println!("     {line}");
            }
            return Ok(false);
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
    fn the_skill_is_written_once_and_keeps_what_others_put_there() {
        let top = scratch("skill");
        git(&top, &["init", "-q"]).unwrap();
        let paths = SkillPaths::new(&top).unwrap();
        let vocab = Vocabulary::builtin().unwrap();
        let mut changed = Vec::new();
        write_skill(&top, &paths, &vocab, &mut changed).unwrap();
        assert_eq!(changed.len(), SKILL.len() + 1);
        assert!(top.join(SKILL_DIR).join(REFERENCE).is_file());

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
