//! Read the policy and changed paths from the tree Git is about to commit.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use gitmoji_triangle::config::{Config, SETTINGS_FILES};

use crate::history::git;

/// Capture the active index, including the temporary index Git uses for partial commits.
pub(crate) fn tree() -> Result<String> {
    Ok(git(&["write-tree"])
        .context("cannot read the staged tree")?
        .trim_end()
        .to_owned())
}

pub(crate) fn config(tree: &str) -> Result<Config> {
    // Let Git resolve symlinks inside the staged tree. A symlink's blob alone
    // contains its target name, not the settings.
    let mut child = Command::new("git")
        .args([
            "--no-replace-objects",
            "cat-file",
            "--batch-check",
            "--follow-symlinks",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("cannot inspect the staged settings")?;
    let mut input = child.stdin.take().context("cannot query the staged tree")?;
    for path in SETTINGS_FILES {
        writeln!(input, "{tree}:{path}")?;
    }
    drop(input);
    let output = child.wait_with_output()?;
    if !output.status.success() {
        bail!(
            "cannot inspect staged settings: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let mut lines = output.stdout.split(|byte| *byte == b'\n');
    for path in SETTINGS_FILES {
        let header = std::str::from_utf8(lines.next().context("missing staged settings lookup")?)?;
        if header.ends_with(" missing") {
            continue;
        }
        let mut fields = header.split(' ');
        let oid = fields.next().context("missing staged settings object")?;
        match fields.next() {
            Some("blob") => {
                let output = Command::new("git")
                    .args(["--no-replace-objects", "cat-file", "blob", oid])
                    .output()
                    .context("cannot read the staged settings")?;
                if !output.status.success() {
                    bail!(
                        "cannot read staged {path}: {}",
                        String::from_utf8_lossy(&output.stderr).trim()
                    );
                }
                let source = std::str::from_utf8(&output.stdout)
                    .with_context(|| format!("staged {path} must be UTF-8"))?;
                if path == "triangle.toml" {
                    eprintln!(
                        "gitmoji: reading staged triangle.toml; run `gitmoji init` to rename it to gitmoji.toml"
                    );
                }
                return Config::from_toml(source)
                    .with_context(|| format!("cannot read staged {path}"));
            }
            Some("tree" | "commit") => continue,
            // Git follows links inside the tree. It returns a separate record
            // for broken or external links, which cannot supply a staged policy.
            _ if matches!(oid, "dangling" | "notdir") => {
                lines.next().context("missing staged symlink target")?;
            }
            _ => bail!("cannot resolve staged {path} to a file inside the repository"),
        }
    }
    // A missing policy in the proposed tree always selects defaults. Merely
    // report a working copy if one exists, without opening or parsing it.
    if let Ok(location) = git(&["rev-parse", "--is-inside-work-tree", "--show-cdup"])
        && let Some(up) = location.strip_prefix("true\n")
        && let Some(path) = SETTINGS_FILES
            .into_iter()
            .find(|path| Path::new(up.trim_end()).join(path).is_file())
    {
        eprintln!(
            "gitmoji: using built-in defaults. Stage {path} to apply its settings to this commit."
        );
    }
    Ok(Config::standard()?)
}

/// Compare the captured tree to HEAD; before the first commit every path is new.
pub(crate) fn files(tree: &str) -> Result<Vec<String>> {
    let head = Command::new("git")
        .args([
            "--no-replace-objects",
            "rev-parse",
            "--verify",
            "--quiet",
            "HEAD^{tree}",
        ])
        .output()
        .context("cannot find HEAD")?;
    let paths = match head.status.code() {
        Some(0) => git(&[
            "diff",
            "--name-only",
            "-z",
            "--no-relative",
            std::str::from_utf8(&head.stdout)?.trim_end(),
            tree,
            "--",
        ])?,
        Some(1) => git(&["ls-tree", "-r", "-z", "--name-only", "--full-tree", tree])?,
        _ => bail!(
            "cannot find HEAD: {}",
            String::from_utf8_lossy(&head.stderr).trim()
        ),
    };
    Ok(paths.split_terminator('\0').map(str::to_owned).collect())
}
