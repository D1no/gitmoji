//! Per-repository settings, read from `gitmoji.toml`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde::Deserialize;

/// How strictly a rule is applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    /// Not checked.
    Off,
    /// Reported, and the commit goes through.
    #[default]
    Warn,
    /// Reported, and the commit is refused.
    Error,
}

/// Who should supply the model and harness lines in a commit message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SignatureRule {
    /// Accept unsigned messages.
    #[default]
    Off,
    /// Warn for any unsigned message.
    Warn,
    /// Refuse any unsigned message.
    Error,
    /// Refuse an unsigned authored draft from a recognized agent shell.
    Agents,
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawTicket {
    patterns: Option<Vec<String>>,
    example: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawScope {
    #[serde(default)]
    allowed: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawRules {
    untagged: Option<Level>,
    lone_second: Option<bool>,
    trailers: Option<Level>,
    one_scope: Option<Level>,
    unusual: Option<Level>,
    agent_author: Option<Level>,
    signature: Option<SignatureRule>,
    length: Option<Level>,
    description: Option<Level>,
    stamp: Option<bool>,
    subject_max: Option<usize>,
    body_max_lines: Option<usize>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawVocabulary {
    version: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawAgents {
    markers: Option<Vec<String>>,
}

/// An ordered release-notes audience with its Gitmoji area symbols.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Audience {
    /// Markdown section heading.
    pub name: String,
    /// Authored area symbols belonging to the audience.
    pub areas: Vec<String>,
}

/// Handling of open provisional work when drafting release notes.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProvisionalPolicy {
    /// List outstanding tickets at the release revision.
    #[default]
    Disclose,
    /// Refuse to produce a draft until all outstanding tickets close.
    Refuse,
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawChangelog {
    #[serde(default)]
    provisional: ProvisionalPolicy,
    groups: Option<Vec<Audience>>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    #[serde(default)]
    changelog: RawChangelog,
    #[serde(default)]
    ticket: RawTicket,
    #[serde(default)]
    scope: RawScope,
    #[serde(default)]
    rules: RawRules,
    #[serde(default)]
    agents: RawAgents,
    #[serde(default)]
    vocabulary: RawVocabulary,
    #[serde(default)]
    paths: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    hints: BTreeMap<String, String>,
}

/// The settings a check runs with.
#[derive(Debug, Clone)]
pub struct Config {
    /// Changelog sections in priority order. Omit to use the standard audience map.
    pub changelog_groups: Option<Vec<Audience>>,
    /// Whether open provisional work is disclosed or prevents a release draft.
    pub changelog_provisional: ProvisionalPolicy,
    ticket_patterns: Vec<Regex>,
    /// What a ticket looks like here, for messages: `#123`, `TASK-150`.
    pub ticket_example: String,
    /// Scopes a commit may name. Empty means any.
    pub scopes_allowed: Vec<String>,
    /// How a plain, untagged subject is treated at commit time. History still reports it.
    pub untagged: Level,
    /// Whether the area may be left out.
    pub lone_second: bool,
    /// How a missing trailer such as `Limitation:` is treated.
    pub trailers: Level,
    /// How staged files from more than one scope are treated.
    pub one_scope: Level,
    /// How a pair that rarely makes sense is treated, such as style in infrastructure.
    pub unusual: Level,
    /// How an agent named as author or co-author is treated.
    pub agent_author: Level,
    /// How a commit without an agent's signature block (model and harness lines) is treated.
    pub signature: SignatureRule,
    /// How a summary or a body that is too long is treated.
    pub length: Level,
    /// How a commit by a person or an agent with nothing under its first line is treated.
    pub description: Level,
    /// Whether the hook adds a line naming the vocabulary version a message was checked
    /// against.
    pub stamp: bool,
    /// The character that starts a comment line in a commit message, from git's
    /// `core.commentChar`. Not a setting of `gitmoji.toml`; the caller fills it in.
    pub comment_char: char,
    /// True when git is told to keep comment lines in every message (`commit.cleanup`
    /// set to whitespace, verbatim or scissors). Filled in by the caller as well.
    pub keeps_comments: bool,
    /// The longest first line, in characters, with emojis, ticket and scope included.
    pub subject_max: usize,
    /// The most lines of prose a body may have, at 72 characters a line. Blank lines,
    /// trailers and the signature do not count.
    pub body_max_lines: usize,
    /// Text that marks an identity as an agent's, compared without regard to case.
    pub agent_markers: Vec<String>,
    /// The vocabulary version this repository has adopted, when it records one.
    pub vocabulary_version: Option<String>,
    /// Path prefixes that belong to each scope.
    pub scope_paths: BTreeMap<String, Vec<String>>,
    /// File patterns and the area they suggest: `*.css` or a path prefix.
    pub hints: Vec<(String, String)>,
}

/// The settings could not be used.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// The file could not be read.
    #[error("cannot read {path}: {source}")]
    Read {
        /// The file that was tried.
        path: String,
        /// What went wrong.
        source: std::io::Error,
    },
    /// The file is not valid TOML or holds an unknown key.
    #[error("the settings are not valid: {0}")]
    Parse(#[from] toml::de::Error),
    /// A ticket pattern is not a valid regular expression.
    #[error("ticket pattern `{pattern}` is not a valid regular expression: {source}")]
    Pattern {
        /// The pattern as written.
        pattern: String,
        /// What went wrong.
        source: regex::Error,
    },
}

/// The names a settings file may have inside a folder, in the order they are tried.
/// The last is the name from before tool 0.7.0.
pub const SETTINGS_FILES: [&str; 3] = ["gitmoji.toml", ".config/gitmoji.toml", "triangle.toml"];

/// Finds the settings in a folder: the root of the repository, or the current folder
/// when there is no repository. Git runs hooks at the root, so a check typed in a
/// subfolder and the hook read the same file.
pub fn locate(folder: &Path) -> Option<PathBuf> {
    SETTINGS_FILES
        .iter()
        .map(|name| folder.join(name))
        .find(|file| file.is_file())
}

impl Config {
    /// The settings used when a repository has no `gitmoji.toml`: GitHub issue numbers.
    pub fn standard() -> Result<Self, ConfigError> {
        Self::from_toml("")
    }

    /// Reads settings from a file.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.display().to_string(),
            source,
        })?;
        Self::from_toml(&text)
    }

    /// Reads settings from TOML text.
    pub fn from_toml(text: &str) -> Result<Self, ConfigError> {
        let raw: RawConfig = toml::from_str(text)?;
        let sources = raw
            .ticket
            .patterns
            .unwrap_or_else(|| vec!["#[0-9]+".to_string()]);
        let ticket_patterns = sources
            .iter()
            .map(|pattern| {
                Regex::new(&format!("^(?:{pattern})$")).map_err(|source| ConfigError::Pattern {
                    pattern: pattern.clone(),
                    source,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            changelog_groups: raw.changelog.groups,
            changelog_provisional: raw.changelog.provisional,
            ticket_patterns,
            ticket_example: raw.ticket.example.unwrap_or_else(|| "#123".to_string()),
            scopes_allowed: raw.scope.allowed,
            untagged: raw.rules.untagged.unwrap_or(Level::Error),
            lone_second: raw.rules.lone_second.unwrap_or(true),
            trailers: raw.rules.trailers.unwrap_or_default(),
            one_scope: raw.rules.one_scope.unwrap_or_default(),
            unusual: raw.rules.unusual.unwrap_or_default(),
            agent_author: raw.rules.agent_author.unwrap_or(Level::Error),
            signature: raw.rules.signature.unwrap_or_default(),
            vocabulary_version: raw.vocabulary.version,
            length: raw.rules.length.unwrap_or_default(),
            description: raw.rules.description.unwrap_or(Level::Error),
            stamp: raw.rules.stamp.unwrap_or(true),
            comment_char: '#',
            keeps_comments: false,
            // The length of an early tweet: room to say it, short enough to read at a glance.
            subject_max: raw.rules.subject_max.unwrap_or(140),
            body_max_lines: raw.rules.body_max_lines.unwrap_or(6),
            agent_markers: raw.agents.markers.unwrap_or_else(|| {
                // Addresses and product names, not first names: people are called Claude too.
                [
                    "anthropic.com",
                    "openai.com",
                    "copilot",
                    "cursoragent",
                    "devin-ai-integration",
                    "google-labs-jules",
                ]
                .map(String::from)
                .to_vec()
            }),
            scope_paths: raw.paths,
            hints: raw.hints.into_iter().collect(),
        })
    }

    /// Plain prose can pass during adoption. Ticket-first and emoji-first attempts
    /// still receive the usual diagnostics so the author can correct them.
    pub(crate) fn allows_plain_subject(&self, line: &str) -> bool {
        let Some(first) = line.split_whitespace().next() else {
            return false;
        };
        self.untagged != Level::Error
            && first.starts_with(char::is_alphanumeric)
            && !self.is_ticket(first)
            && !first.contains('\u{20e3}') // A digit keycap is an emoji, not prose.
    }

    /// The same settings for checking a first line on its own, where there is no
    /// description and no signature to ask for.
    #[must_use]
    pub fn for_a_subject_alone(mut self) -> Self {
        self.description = Level::Off;
        self.signature = SignatureRule::Off;
        self
    }

    /// True for a line the message does not carry: a comment, unless git keeps those.
    pub fn is_comment(&self, line: &str) -> bool {
        !self.keeps_comments && line.starts_with(self.comment_char)
    }

    /// True for the line git puts above the diff of `git commit --verbose`. Everything
    /// from it on is cut off before the message is stored.
    pub fn is_scissors(&self, line: &str) -> bool {
        line.strip_prefix(self.comment_char).is_some_and(|rest| {
            rest.trim() == "------------------------ >8 ------------------------"
        })
    }

    /// True when an author or co-author line names an agent.
    pub fn names_an_agent(&self, identity: &str) -> bool {
        let identity = identity.to_lowercase();
        self.agent_markers
            .iter()
            .any(|marker| identity.contains(&marker.to_lowercase()))
    }

    /// True when the text is a ticket as this repository writes them.
    pub fn is_ticket(&self, text: &str) -> bool {
        self.ticket_patterns
            .iter()
            .any(|pattern| pattern.is_match(text))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("gitmoji-config-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn locate_should_prefer_the_root_file_to_the_config_folder() {
        let top = scratch("locate");
        assert_eq!(locate(&top), None);
        std::fs::create_dir_all(top.join(".config")).unwrap();
        std::fs::write(top.join(".config/gitmoji.toml"), "").unwrap();
        assert_eq!(locate(&top), Some(top.join(".config/gitmoji.toml")));
        std::fs::write(top.join("gitmoji.toml"), "").unwrap();
        assert_eq!(locate(&top), Some(top.join("gitmoji.toml")));
        std::fs::remove_dir_all(top).unwrap();
    }

    #[test]
    fn standard_should_accept_a_github_issue() {
        assert!(Config::standard().unwrap().is_ticket("#412"));
    }

    #[test]
    fn standard_should_refuse_a_bare_number() {
        assert!(!Config::standard().unwrap().is_ticket("412"));
    }

    #[test]
    fn patterns_should_allow_backlog_task_ids() {
        let config = Config::from_toml("[ticket]\npatterns = ['TASK-[0-9]+(\\.[0-9]+)*']").unwrap();
        assert!(config.is_ticket("TASK-134.2"));
    }

    #[test]
    fn names_an_agent_should_spot_a_vendor_address() {
        assert!(
            Config::standard()
                .unwrap()
                .names_an_agent("Claude <noreply@anthropic.com>")
        );
    }

    #[test]
    fn names_an_agent_should_leave_a_person_called_claude_alone() {
        assert!(
            !Config::standard()
                .unwrap()
                .names_an_agent("Claude Dupont <claude@example.org>")
        );
    }

    #[test]
    fn from_toml_should_refuse_an_unknown_key() {
        assert!(Config::from_toml("[rules]\nlone = true").is_err());
    }

    #[test]
    fn from_toml_should_refuse_a_broken_pattern() {
        assert!(Config::from_toml("[ticket]\npatterns = ['(']").is_err());
    }
}
