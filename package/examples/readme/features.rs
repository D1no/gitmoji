//! Discover the commands and descriptions from the CLI built from this checkout.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::process::Command;

use anyhow::{Context, Result, ensure};

pub(super) fn render() -> Result<String> {
    let output = Command::new(env!("CARGO"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "run", "--locked", "--quiet", "--bin", "gitmoji", "--", "--help",
        ])
        .env("NO_COLOR", "1")
        .output()
        .context("cannot run the project's gitmoji --help")?;
    ensure!(
        output.status.success(),
        "cannot read gitmoji --help: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    from_help(std::str::from_utf8(&output.stdout)?)
}

fn from_help(help: &str) -> Result<String> {
    let mut lines = help.lines();
    lines
        .find(|line| *line == "Commands:")
        .context("gitmoji --help is missing its Commands section")?;

    let mut commands = BTreeMap::new();
    for line in lines.take_while(|line| !line.trim().is_empty()) {
        let (name, description) = line
            .trim()
            .split_once(char::is_whitespace)
            .with_context(|| format!("missing command description in gitmoji --help: {line}"))?;
        ensure!(
            commands.insert(name, description.trim_start()).is_none(),
            "duplicate command in gitmoji --help: {name}"
        );
    }
    ensure!(!commands.is_empty(), "gitmoji --help lists no commands");

    let mut markdown = String::from("## What Gitmoji can do\n\n");
    for (workflow, command_names) in [
        ("Set up a project", &["init", "doctor", "pin"][..]),
        (
            "Plan and write commits",
            &["guide", "staged", "check", "signature"][..],
        ),
        (
            "Review and merge",
            &[
                "pr-title",
                "merge-caption",
                "merge-message",
                "provisional-pr",
                "defer-findings",
                "github-merge",
            ][..],
        ),
        (
            "Read history and prepare a release",
            &["explain", "log", "history", "open", "changelog"][..],
        ),
        (
            "Use the vocabulary in other tools",
            &["vocabulary", "export", "site", "help"][..],
        ),
    ] {
        let members = command_names
            .iter()
            .filter_map(|&name| commands.remove(name).map(|description| (name, description)));
        write_group(&mut markdown, workflow, members)?;
    }
    write_group(&mut markdown, "More commands", commands.into_iter())?;
    Ok(markdown)
}

fn write_group<'a>(
    markdown: &mut String,
    workflow: &str,
    commands: impl Iterator<Item = (&'a str, &'a str)>,
) -> Result<()> {
    let mut commands = commands.peekable();
    if commands.peek().is_some() {
        writeln!(markdown, "**{workflow}**\n")?;
        for (name, description) in commands {
            writeln!(markdown, "- `gitmoji {name}` — {description}")?;
        }
        markdown.push('\n');
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newly_added_commands_appear_without_assigning_a_workflow() -> Result<()> {
        let help =
            "Commands:\n  init    Set up a project\n  preview  Preview a change\n\nOptions:\n";
        let markdown = from_help(help)?;
        ensure!(markdown.contains("**More commands**\n\n- `gitmoji preview` — Preview a change"));
        ensure!(markdown.matches("`gitmoji init`").count() == 1);
        Ok(())
    }
}
