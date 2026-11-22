//! The `gitmoji` command: check a commit message, read the guide, look at staged files.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};

use gitmoji_triangle::caption;
use gitmoji_triangle::check::{check_branch_message, check_message};
use gitmoji_triangle::config::{Config, Level, locate};
use gitmoji_triangle::fix::fix_message;
use gitmoji_triangle::guide;
use gitmoji_triangle::staged::Staged;
use gitmoji_triangle::vocabulary::Vocabulary;

mod doctor;
mod history;
mod hosting;
mod init;
mod local_merge;
mod signature;
mod upgrade;

#[derive(Parser)]
#[command(
    name = "gitmoji",
    // The package name tells this command apart from other tools called gitmoji.
    version = concat!(env!("CARGO_PKG_VERSION"), " (gitmoji-triangle)"),
    about = "Gitmoji Triangle: check and explain commit subjects"
)]
struct Cli {
    /// Settings file. Defaults to gitmoji.toml, or .config/gitmoji.toml, at the root of the repository.
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Action,
}

#[derive(Subcommand)]
enum Action {
    /// Check the installation in this working tree without changing it.
    Doctor {
        /// Print an array of component results for scripts.
        #[arg(long)]
        json: bool,
    },
    /// Print the release pinned in the project configuration.
    Pin,
    /// Print a commit signature from the active agent session.
    Signature {
        /// Select a harness explicitly when session discovery is ambiguous.
        #[arg(long, value_enum)]
        harness: Option<signature::Harness>,
    },
    /// Synchronize a GitHub pull request title from its incoming commits.
    PrTitle(hosting::TitleArgs),
    /// Preview or submit a verified direct merge on GitHub.
    GithubMerge(hosting::MergeArgs),
    /// Check a commit message. As a commit-msg hook: `gitmoji check --fix {1}`.
    Check {
        /// The file git wrote the message to.
        file: Option<PathBuf>,
        /// Check this text and not a file.
        #[arg(long, short)]
        message: Option<String>,
        /// First fix the file: write emojis for typed shortcodes, and close the message with
        /// the line that names the vocabulary version it was checked against.
        #[arg(long)]
        fix: bool,
    },
    /// Read the guide: no topic for the start, or an emoji, a name, a group or a topic.
    Guide {
        /// An emoji, a name, a group such as "protection", or one of: areas, behaviours,
        /// intents, modifiers, markers, groups, rules, message, scopes, all.
        topic: Option<String>,
    },
    /// Read a subject or a run of emojis back in words.
    Explain {
        /// A subject line, emojis such as 🧩👔💥, or a revision with --commit.
        text: String,
        /// Read the stored commit and its parents to explain a merge caption.
        #[arg(long)]
        commit: bool,
    },
    /// Look at the staged files and say when they look like more than one commit.
    Staged,
    /// Check commits that are already made, and list the ones that would not pass.
    History {
        /// The commits to check, as git names a range: `main..HEAD`. Without it, every
        /// commit.
        #[arg(conflicts_with = "upgrade")]
        range: Option<String>,
        /// Preview or apply a candidate history, or update verified hosted titles.
        #[arg(long, conflicts_with = "unmigrated")]
        upgrade: bool,
        #[command(flatten)]
        options: upgrade::Options,
        /// Check only the commits that do not say they were checked against the version
        /// this command speaks: those from an earlier version, and those without the line.
        #[arg(long)]
        unmigrated: bool,
    },
    /// Caption Git’s generated local branch merge message, or preview a stored merge.
    MergeMessage {
        /// Git’s message file (first prepare-commit-msg argument).
        #[arg(required_unless_present = "historical")]
        file: Option<PathBuf>,
        /// Git’s message source (second prepare-commit-msg argument).
        source: Option<String>,
        /// Preview the generated subject and inventory from a stored merge with known direction.
        #[arg(long, conflicts_with_all = ["file", "source"])]
        historical: Option<String>,
    },
    /// Print the synchronized PR title for the authored commits arriving on its base.
    MergeCaption {
        /// Print JSON with the PR title, landed subject, and recognized and total commit counts.
        #[arg(long)]
        json: bool,
        /// The pull request number.
        #[arg(long)]
        pr: u64,
        /// The pull request's full, single-line title.
        #[arg(long, allow_hyphen_values = true)]
        title: String,
        /// The pull request's actual target revision before the merge.
        base: String,
        /// The pull request's source revision before the merge.
        head: String,
    },
    /// Set a git repository up: the agent skill, gitmoji.toml, the hooks, CLAUDE.md.
    Init {
        /// A folder inside the repository. Defaults to the current folder.
        path: Option<PathBuf>,
        /// How the repository names its tickets.
        #[arg(long, value_enum, default_value = "github")]
        tickets: init::Tickets,
        /// Install or update the GitHub workflows for PR titles and commit checks.
        #[arg(long)]
        github_workflows: bool,
    },
    /// Print the version of the vocabulary this command speaks.
    Vocabulary,
    /// Print the vocabulary for other tools.
    Export {
        /// The output format.
        #[arg(value_enum)]
        format: ExportFormat,
    },
    /// Build the website from a template.
    Site {
        /// The template, with a `/*__VOCABULARY__*/` marker.
        #[arg(long)]
        template: PathBuf,
        /// Where to write the page.
        #[arg(long)]
        out: PathBuf,
        /// A JSON file of worked examples, for a `/*__REPLAYS__*/` marker in the template.
        #[arg(long)]
        replays: Option<PathBuf>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum ExportFormat {
    /// The vocabulary as JSON.
    Json,
    /// The whole guide as plain text.
    Text,
}

/// Select the same project configuration from the repository root or a nested directory.
fn settings_path(path: Option<&Path>) -> Result<Option<PathBuf>> {
    if let Some(path) = path {
        return Ok(Some(path.to_owned()));
    }
    let top = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| PathBuf::from(String::from_utf8_lossy(&output.stdout).trim()));
    let folder = match top {
        Some(top) => top,
        None => std::env::current_dir().context("cannot tell the current folder")?,
    };
    Ok(locate(&folder))
}

/// Read the project's adopted version even if its other settings belong to another release.
fn pinned_release(path: Option<&Path>) -> Result<String> {
    let file = settings_path(path)?.context("No Gitmoji settings found in the project")?;
    read_release_pin(&file)?.context("Cannot read [vocabulary].version from project settings")
}

fn read_release_pin(file: &Path) -> Result<Option<String>> {
    let source =
        std::fs::read_to_string(file).with_context(|| format!("cannot read {}", file.display()))?;
    let settings = source
        .parse::<toml::Table>()
        .with_context(|| format!("Cannot read [vocabulary].version from {}", file.display()))?;
    let Some(value) = settings.get("vocabulary").and_then(|v| v.get("version")) else {
        return Ok(None);
    };
    let valid = |part: &str| {
        !part.is_empty()
            && part.bytes().all(|n| n.is_ascii_digit())
            && (part == "0" || !part.starts_with('0'))
    };
    let version = value
        .as_str()
        .filter(|v| v.split('.').count() == 3 && v.split('.').all(valid))
        .with_context(|| {
            format!(
                "Expected a release number such as 1.2.3 in {}",
                file.display()
            )
        })?;
    Ok(Some(version.to_owned()))
}

fn load_config(path: Option<&Path>) -> Result<Config> {
    match settings_path(path)? {
        Some(file) => {
            if file.ends_with("triangle.toml") {
                eprintln!(
                    "gitmoji: reading triangle.toml; run `gitmoji init` to rename it to gitmoji.toml"
                );
            }
            Ok(Config::load(&file)?)
        }
        None => Ok(Config::standard()?),
    }
}

/// The character git treats as the start of a comment line in a commit message, when
/// the repository sets one. `auto` and longer strings are left to the default.
fn comment_char() -> Option<char> {
    let output = Command::new("git")
        .args(["config", "--get", "core.commentChar"])
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    let value = String::from_utf8_lossy(&output.stdout);
    let mut chars = value.trim().chars();
    match (chars.next(), chars.next()) {
        (Some(only), None) => Some(only),
        _ => None,
    }
}

/// True when git is told to keep comment lines in every commit message.
fn keeps_comments() -> bool {
    Command::new("git")
        .args(["config", "--get", "commit.cleanup"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .is_some_and(|output| {
            matches!(
                String::from_utf8_lossy(&output.stdout).trim(),
                "whitespace" | "verbatim" | "scissors"
            )
        })
}

/// Git passes MERGE_MSG for an immediate merge and COMMIT_EDITMSG when a person
/// finishes one. Only exempt those files in the current worktree's Git directory:
/// an unrelated draft checked during a merge must still be validated.
fn is_pending_merge_message(file: &Path) -> bool {
    let Some(parent) = file.parent() else {
        return false;
    };
    if !matches!(
        file.file_name().and_then(|name| name.to_str()),
        Some("MERGE_MSG" | "COMMIT_EDITMSG")
    ) || !parent.join("MERGE_HEAD").is_file()
        || !file.is_file()
    {
        return false;
    }
    // Compare Git's own normalized paths, including linked worktrees and non-UTF-8 names.
    let args = ["rev-parse", "--absolute-git-dir"];
    let current = Command::new("git").args(args).output();
    let candidate = Command::new("git")
        .arg("--git-dir")
        .arg(parent)
        .args(args)
        .output();
    matches!((current, candidate), (Ok(current), Ok(candidate))
        if current.status.success() && candidate.status.success() && current.stdout == candidate.stdout)
}

/// The author git is about to record, or nothing when git cannot say.
fn author_identity() -> Option<String> {
    let output = Command::new("git")
        .args(["var", "GIT_AUTHOR_IDENT"])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn staged_files() -> Result<Vec<String>> {
    let output = Command::new("git")
        .args(["diff", "--cached", "--name-only", "--diff-filter=ACMRD"])
        .output()
        .context("cannot run git")?;
    if !output.status.success() {
        bail!("git could not list the staged files");
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_string)
        .collect())
}

fn run(cli: Cli) -> Result<ExitCode> {
    if let Action::Doctor { json } = cli.command {
        return doctor::run(cli.config.as_deref(), json);
    }
    if let Action::Pin = cli.command {
        println!("{}", pinned_release(cli.config.as_deref())?);
        return Ok(ExitCode::SUCCESS);
    }
    if let Action::Signature { harness } = cli.command {
        println!("{}", signature::render(harness)?);
        return Ok(ExitCode::SUCCESS);
    }
    if let Action::PrTitle(args) = cli.command {
        hosting::sync_title(args)?;
        return Ok(ExitCode::SUCCESS);
    }
    if let Action::GithubMerge(args) = cli.command {
        hosting::merge(args)?;
        return Ok(ExitCode::SUCCESS);
    }
    let vocab = Vocabulary::builtin()?;
    if let Action::History {
        upgrade: true,
        ref options,
        ..
    } = cli.command
    {
        upgrade::run(options, &vocab)?;
        return Ok(ExitCode::SUCCESS);
    }
    if let Action::Init {
        path,
        tickets,
        github_workflows,
    } = &cli.command
    {
        // Before the settings are read: a repository that has none yet is the usual case.
        init::run(
            &vocab,
            path.as_deref().unwrap_or(Path::new(".")),
            *tickets,
            *github_workflows,
        )?;
        return Ok(ExitCode::SUCCESS);
    }
    if let Action::MergeMessage {
        file,
        source,
        historical,
    } = &cli.command
    {
        if let Some(commit) = historical {
            if let Some(message) = local_merge::historical(commit, &vocab)? {
                println!("{message}");
            }
        } else if let (Some(file), Some("merge")) = (file, source.as_deref())
            && is_pending_merge_message(file)
        {
            local_merge::prepare(file, &vocab)?;
        }
        return Ok(ExitCode::SUCCESS);
    }
    let mut config = load_config(cli.config.as_deref())?;
    if let Some(comment) = comment_char() {
        config.comment_char = comment;
    }
    config.keeps_comments = keeps_comments();
    match cli.command {
        Action::Check { file, message, fix } => {
            let from_file = message.is_none();
            let mut branch_merge = false;
            let mut text = match (message, &file) {
                (Some(text), _) => text,
                (None, Some(file)) if is_pending_merge_message(file) => {
                    let bytes = std::fs::read(file)
                        .with_context(|| format!("cannot read {}", file.display()))?;
                    let Ok(raw) = String::from_utf8(bytes) else {
                        eprintln!("gitmoji: not checked: a merge commit");
                        return Ok(ExitCode::SUCCESS);
                    };
                    if !caption::is_branch_message(&raw, &vocab) {
                        eprintln!("gitmoji: not checked: a merge commit");
                        return Ok(ExitCode::SUCCESS);
                    }
                    branch_merge = true;
                    raw
                }
                (None, Some(file)) => std::fs::read_to_string(file)
                    .with_context(|| format!("cannot read {}", file.display()))?,
                (None, None) => bail!("give a message file, or --message"),
            };
            if !from_file
                && text.lines().count() == 1
                && caption::branch_sources(text.trim_end(), &vocab).is_some()
            {
                eprintln!(
                    "gitmoji: this looks like a branch merge caption. The subject alone cannot confirm the commit has multiple parents. Use `gitmoji explain --commit <sha>` to inspect this commit, or `gitmoji history <range>` to check stored commits."
                );
                return Ok(ExitCode::FAILURE);
            }
            if fix {
                let fixed = fix_message(&vocab, &config, &text);
                for change in &fixed.changes {
                    eprintln!("gitmoji: {change}");
                }
                // The file is written whenever fixing changed it, also by the version
                // line alone, which is not announced on every commit.
                if let (true, Some(file), true) = (from_file, &file, fixed.message != text) {
                    std::fs::write(file, &fixed.message)
                        .with_context(|| format!("cannot write {}", file.display()))?;
                }
                text = fixed.message;
            }
            // Only a real commit has an author to look at, and a description to ask for.
            let author = if from_file { author_identity() } else { None };
            let config = if from_file {
                config
            } else {
                config.for_a_subject_alone()
            };
            let report = if branch_merge {
                check_branch_message(&config, &text, false)
            } else {
                check_message(&vocab, &config, &text, author.as_deref())
            };
            for warning in &report.warnings {
                eprintln!("gitmoji: note: {warning}");
            }
            if report.passed() {
                if let Some(reading) = &report.reading {
                    eprintln!("gitmoji: {reading}");
                }
                return Ok(ExitCode::SUCCESS);
            }
            for error in &report.errors {
                eprintln!("gitmoji: {error}");
            }
            if !branch_merge {
                eprintln!("\n  {}", vocab.format.subject);
                eprintln!("  {}", vocab.format.example);
                eprintln!(
                    "\nRun `gitmoji guide` for the four questions to answer before a commit."
                );
            }
            Ok(ExitCode::FAILURE)
        }
        Action::Guide { topic } => {
            let wanted = topic.unwrap_or_default();
            if wanted.eq_ignore_ascii_case("scopes") || wanted.eq_ignore_ascii_case("scope") {
                // The scopes are the one part of the guide that belongs to the repository.
                let mut known: Vec<&String> = config
                    .scopes_allowed
                    .iter()
                    .chain(config.scope_paths.keys())
                    .collect();
                known.sort();
                known.dedup();
                if known.is_empty() {
                    println!(
                        "This repository lists no scopes in gitmoji.toml. Use the name of the package or top-level folder that owns the decision, one per commit."
                    );
                } else {
                    println!("Scopes of this repository, one per commit:");
                    for scope in known {
                        println!("  {scope}");
                    }
                }
                return Ok(ExitCode::SUCCESS);
            }
            match guide::topic(&vocab, &wanted) {
                Some(text) => {
                    print!("{text}");
                    Ok(ExitCode::SUCCESS)
                }
                None => {
                    eprintln!(
                        "gitmoji: nothing in the guide is called `{wanted}`. Try an emoji, a group such as \"protection\", or: areas, behaviours, intents, modifiers, markers, groups, rules, message, scopes, all."
                    );
                    Ok(ExitCode::FAILURE)
                }
            }
        }
        Action::Explain { text, commit } => {
            if commit {
                return history::explain_commit(&vocab, &config, &text);
            }
            // A subject has words after its emojis; a run of emojis alone does not.
            if text.trim().contains(char::is_whitespace) {
                let report = check_message(&vocab, &config.for_a_subject_alone(), &text, None);
                if let Some(reading) = &report.reading {
                    println!("{reading}");
                }
                for line in report.errors.iter().chain(&report.warnings) {
                    println!("{line}");
                }
                return Ok(if report.passed() {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::FAILURE
                });
            }
            match vocab.tokens(text.trim()) {
                Ok(tokens) => {
                    for token in tokens {
                        match token {
                            gitmoji_triangle::vocabulary::Token::Known(group, entry) => println!(
                                "{} {} ({}): {}",
                                entry.emoji,
                                entry.name,
                                group.slot.label(),
                                entry.meaning
                            ),
                            gitmoji_triangle::vocabulary::Token::Retired(retired) => println!(
                                "{} is no longer used; write {}. {}",
                                retired.emoji, retired.into, retired.note
                            ),
                        }
                    }
                    Ok(ExitCode::SUCCESS)
                }
                Err(rest) => {
                    eprintln!("gitmoji: `{rest}` is not in the vocabulary");
                    Ok(ExitCode::FAILURE)
                }
            }
        }
        Action::Staged => {
            let staged = Staged::analyse(&staged_files()?, &config);
            for line in staged.lines() {
                eprintln!("gitmoji: {line}");
            }
            if staged.spans_scopes() && config.one_scope == Level::Error {
                return Ok(ExitCode::FAILURE);
            }
            Ok(ExitCode::SUCCESS)
        }
        Action::Doctor { .. }
        | Action::Pin
        | Action::Signature { .. }
        | Action::PrTitle(_)
        | Action::GithubMerge(_)
        | Action::Init { .. }
        | Action::MergeMessage { .. } => Ok(ExitCode::SUCCESS),
        Action::History {
            range, unmigrated, ..
        } => history::run(&vocab, config, range.as_deref(), unmigrated),
        Action::MergeCaption {
            json,
            pr,
            title,
            base,
            head,
        } => {
            let incoming = history::incoming_commits(&base, &[&head], &vocab)?;
            let caption = incoming.summarize_pr(pr, &title, &vocab)?;
            if json {
                println!("{}", serde_json::to_string(&caption)?);
            } else {
                println!("{}", caption.title);
            }
            Ok(ExitCode::SUCCESS)
        }
        Action::Vocabulary => {
            println!("{}", vocab.version);
            Ok(ExitCode::SUCCESS)
        }
        Action::Export { format } => {
            match format {
                ExportFormat::Json => println!("{}", serde_json::to_string_pretty(&vocab)?),
                ExportFormat::Text => print!("{}", guide::all(&vocab)),
            }
            Ok(ExitCode::SUCCESS)
        }
        Action::Site {
            template,
            out,
            replays,
        } => {
            let page = std::fs::read_to_string(&template)
                .with_context(|| format!("cannot read {}", template.display()))?;
            let marker = "/*__VOCABULARY__*/";
            if !page.contains(marker) {
                bail!("{} has no {marker} marker", template.display());
            }
            let data = format!("const VOCAB = {};", serde_json::to_string(&vocab)?);
            let mut page = page.replace(marker, &data);
            if let Some(replays) = replays {
                let text = std::fs::read_to_string(&replays)
                    .with_context(|| format!("cannot read {}", replays.display()))?;
                // Parsed and written again, so a broken file fails here and not in the browser.
                let examples: serde_json::Value = serde_json::from_str(&text)
                    .with_context(|| format!("{} is not valid JSON", replays.display()))?;
                page = page.replace("/*__REPLAYS__*/", &format!("const REPLAYS = {examples};"));
            }
            std::fs::write(&out, page)
                .with_context(|| format!("cannot write {}", out.display()))?;
            eprintln!("gitmoji: wrote {}", out.display());
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("gitmoji: {error:#}");
            ExitCode::FAILURE
        }
    }
}
