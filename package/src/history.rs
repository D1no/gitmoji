//! `gitmoji history`: check commits that are already made.
//!
//! The hook checks a commit as it is made and writes into its message which version of
//! the vocabulary it was checked against, ``△ `<version>` ``. This reads finished
//! commits, checks them against the rules this command speaks today, and lists the
//! ones that would not pass.
//!
//! Each commit says for itself which version it is on, so nothing has to be worked out
//! from where a commit sits in the history. A commit that names an earlier version, or
//! none, was not checked against today's rules: it is not migrated.

use std::process::{Command, ExitCode};

use anyhow::{Context, Result, bail};

use gitmoji_triangle::check::{check_message, checked_against};
use gitmoji_triangle::config::{Config, Level};
use gitmoji_triangle::subject;
use gitmoji_triangle::vocabulary::Vocabulary;

/// One commit as git stores it.
struct Commit {
    hash: String,
    is_merge: bool,
    author: String,
    message: String,
}

impl Commit {
    fn subject(&self) -> &str {
        self.message.lines().next().unwrap_or_default()
    }

    fn short(&self) -> &str {
        &self.hash[..self.hash.len().min(9)]
    }
}

/// What a commit says about the version it was checked against.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Standing {
    /// It names the version it was checked against.
    On(String),
    /// It names no version: made before the hook wrote the line, or without the hook.
    Unsaid,
    /// Git records multiple parents, independently of who chose the title.
    Merge,
    /// The subject matches a recognized Git operation.
    Generated,
}

impl Standing {
    fn name(&self) -> String {
        match self {
            Self::On(version) => format!("checked against gitmoji {version}"),
            Self::Unsaid => {
                "without a version: made before the hook wrote one, or without the hook".to_string()
            }
            Self::Merge => "with multiple parents (merge), exempt from author checks".to_string(),
            Self::Generated => {
                "matching a recognized Git subject, exempt from author checks".to_string()
            }
        }
    }
}

fn git(args: &[&str]) -> Result<String> {
    // Captions and checks describe the objects a host will receive. Local
    // replacements and grafts can change both the subjects and their ancestry.
    let output = Command::new("git")
        .arg("--no-replace-objects")
        .env(
            "GIT_GRAFT_FILE",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .args(args)
        .output()
        .context("cannot run git")?;
    if !output.status.success() {
        // Git's own first line says what is wrong, such as a revision it does not know.
        let said = String::from_utf8_lossy(&output.stderr);
        let reason = said.lines().next().unwrap_or("git failed");
        bail!("{}", reason.trim_start_matches("fatal: "));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Return the authored subjects newly reachable on the PR's actual destination.
pub fn merge_subjects(base: &str, head: &str) -> Result<Vec<String>> {
    // Missing parents on either side can hide commits or make already-landed work
    // look new. An incomplete list would silently change the meaning of the caption.
    if git(&["rev-parse", "--is-shallow-repository"])?.trim() == "true" {
        bail!(
            "a merge caption needs complete commit history; run `git fetch --unshallow` then retry"
        );
    }
    let resolve = |revision: &str| {
        git(&[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{revision}^{{commit}}"),
        ])
        .with_context(|| {
            format!("cannot resolve commit `{revision}`; fetch the pull request's base and head")
        })
    };
    // Pin both tips and pass only verified object IDs to the revision walker.
    let base = resolve(base)?;
    let head = resolve(head)?;
    let range = format!("{}..{}", base.trim(), head.trim());
    let log = git(&[
        "log",
        "--topo-order",
        "--reverse",
        "--no-merges",
        "--no-patch",
        "--no-color",
        "--no-notes",
        "--no-show-signature",
        "--encoding=UTF-8",
        "-z",
        "--format=%s",
        &range,
        "--",
    ])?;
    Ok(log.split_terminator('\0').map(str::to_owned).collect())
}

/// The commits of a range, oldest first.
fn commits(range: &str) -> Result<Vec<Commit>> {
    // Copy the full message, including blank lines, without interpreting its contents.
    let log = git(&[
        "log",
        "--reverse",
        "-z",
        "--no-show-signature",
        "--format=%H%x00%P%x00%an <%ae>%x00%B",
        "--end-of-options",
        range,
        // What comes before this is a revision, even where a file has the same name.
        "--",
    ])
    .with_context(|| format!("cannot read the commits of `{range}`"))?;
    let mut fields = log.split_terminator('\0');
    let mut listed = Vec::new();
    while let Some(hash) = fields.next() {
        let parents = fields.next().context("missing parents in git log")?;
        let author = fields.next().context("missing author in git log")?;
        let message = fields.next().context("missing message in git log")?;
        // Git log omits the parents of a shallow boundary commit. Its own object
        // still records them, even when neither parent is available locally.
        let is_merge = if parents.is_empty() {
            git(&["cat-file", "commit", hash])?
                .lines()
                .take_while(|line| !line.is_empty())
                .filter(|line| line.starts_with("parent "))
                .nth(1)
                .is_some()
        } else {
            parents.split_ascii_whitespace().nth(1).is_some()
        };
        listed.push(Commit {
            hash: hash.to_owned(),
            is_merge,
            author: author.to_owned(),
            message: message.to_owned(),
        });
    }
    Ok(listed)
}

fn standing(config: &Config, commit: &Commit) -> Standing {
    if commit.is_merge {
        return Standing::Merge;
    }
    if subject::is_generated(commit.subject()) {
        return Standing::Generated;
    }
    match checked_against(config, &commit.message) {
        Some(version) => Standing::On(version),
        None => Standing::Unsaid,
    }
}

fn plural(count: usize) -> String {
    if count == 1 {
        "1 commit".to_string()
    } else {
        format!("{count} commits")
    }
}

/// Checks finished commits against today's rules and lists the ones that do not pass.
///
/// The commits of `range`, or every commit. With `unmigrated`, only those among them
/// that do not say they were checked against the version this command speaks.
pub fn run(
    vocab: &Vocabulary,
    config: Config,
    range: Option<&str>,
    unmigrated: bool,
) -> Result<ExitCode> {
    // A stored message has no comment lines left in it: what starts with `#` was kept
    // by git and is part of the message.
    let mut config = config;
    config.keeps_comments = true;
    // A permissive hook can admit older subjects; history is where authors find them.
    config.untagged = Level::Error;

    git(&["rev-parse", "--git-dir"]).context("gitmoji history reads a git repository")?;
    let has_commits = Command::new("git")
        .args(["rev-parse", "--verify", "--quiet", "HEAD"])
        .output()
        .is_ok_and(|output| output.status.success());
    if range.is_none() && !has_commits {
        println!("There are no commits yet.");
        return Ok(ExitCode::SUCCESS);
    }
    let listed = commits(range.unwrap_or("HEAD"))?;
    let standings: Vec<Standing> = listed
        .iter()
        .map(|commit| standing(&config, commit))
        .collect();
    let current = Standing::On(vocab.version.clone());
    // Integration commits and recognized Git messages have no authoring version.
    let wanted = |at: usize| {
        !matches!(standings[at], Standing::Merge | Standing::Generated)
            && (!unmigrated || standings[at] != current)
    };

    let mut checked = 0;
    let mut failing = 0;
    for (at, commit) in listed.iter().enumerate() {
        if !wanted(at) {
            continue;
        }
        checked += 1;
        let report = check_message(vocab, &config, &commit.message, Some(&commit.author));
        if report.passed() {
            continue;
        }
        failing += 1;
        println!("{} {}", commit.short(), commit.subject());
        for error in &report.errors {
            println!("    {error}");
        }
    }

    let place = range.map_or("in the whole history".to_string(), |range| {
        format!("in {range}")
    });
    let scope = if unmigrated {
        format!("{place} not yet checked against gitmoji {}", vocab.version)
    } else {
        place
    };
    if failing == 0 {
        println!(
            "Every commit {scope} passes the rules of gitmoji {} ({checked} checked).",
            vocab.version
        );
    } else {
        println!();
        println!(
            "{failing} of the {} {scope} would not pass the rules of gitmoji {}, which this command speaks.",
            plural(checked),
            vocab.version
        );
    }

    // What the commits say for themselves, in the order they are met from the newest.
    let mut groups: Vec<(Standing, usize)> = Vec::new();
    for standing in standings.iter().rev() {
        match groups.iter_mut().find(|(known, _)| known == standing) {
            Some((_, count)) => *count += 1,
            None => groups.push((standing.clone(), 1)),
        }
    }
    if groups.iter().any(|(standing, _)| *standing != current) {
        println!();
        println!("What the commits say about themselves:");
        for (standing, count) in &groups {
            println!("  {} {}", plural(*count), standing.name());
        }
        if !unmigrated {
            println!(
                "A commit made under earlier rules may stay as it is. `gitmoji history --unmigrated` checks only those not on gitmoji {}.",
                vocab.version
            );
        }
    }
    Ok(if failing == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn standing_of(message: &str) -> Standing {
        let config = Config::standard().unwrap();
        let commit = Commit {
            hash: "0".repeat(40),
            is_merge: false,
            author: "Ada <ada@example.com>".to_string(),
            message: message.to_string(),
        };
        standing(&config, &commit)
    }

    #[test]
    fn a_commit_says_for_itself_which_version_it_is_on() {
        assert_eq!(
            standing_of("🧩👔 #1 (a): x\n\nWhy.\n\n△ `2.0.0-draft.20`\n"),
            Standing::On("2.0.0-draft.20".to_string())
        );
        assert_eq!(standing_of("🧩👔 #1 (a): x\n\nWhy.\n"), Standing::Unsaid);
        assert_eq!(standing_of("old style commit\n"), Standing::Unsaid);
        assert_eq!(standing_of("Merge branch 'side'\n"), Standing::Generated);
        // At the head of an agent's signature, with git's trailers after it.
        assert_eq!(
            standing_of(
                "🧩👔 #1 (a): x\n\nWhy.\n\n△ `1.0.0`\nmodel (high)\nHarness\n\nSigned-off-by: Ada <a@example.com>\n"
            ),
            Standing::On("1.0.0".to_string())
        );
        // A signature from before the hook wrote the line names no version.
        assert_eq!(
            standing_of("🧩👔 #1 (a): x\n\nWhy.\n\n_\nmodel (high)\nHarness\n"),
            Standing::Unsaid
        );
        // A △ in the description, or in an example, is not the closing block.
        assert_eq!(
            standing_of(
                "old style commit\n\n△ `2.0.0-draft.20` is what new commits say.\nBut this is prose.\n"
            ),
            Standing::Unsaid
        );
    }
}
