//! Build an author signature from the current harness's local session records.

mod antigravity;
mod claude;
mod codex;

use anyhow::{Result, bail};
use chrono::DateTime;
use clap::ValueEnum;
use regex::Regex;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::LazyLock;

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum Harness {
    ClaudeCode,
    Codex,
    Antigravity,
}

impl Harness {
    fn detected() -> Result<Self> {
        let mut present = Vec::new();
        if env("CLAUDECODE") == "1"
            || !env("CLAUDE_CODE_ENTRYPOINT").is_empty()
            || !env("CLAUDE_CODE_SESSION_ID").is_empty()
        {
            present.push(Self::ClaudeCode);
        }
        if !env("CODEX_THREAD_ID").is_empty() || !env("CODEX_SESSION_ID").is_empty() {
            present.push(Self::Codex);
        }
        if !env("ANTIGRAVITY_CONVERSATION_ID").is_empty() {
            present.push(Self::Antigravity);
        }
        match present.as_slice() {
            [one] => Ok(*one),
            [] => bail!(
                "no agent session detected; select --harness claude-code, codex or antigravity"
            ),
            _ => bail!(
                "several agent sessions detected; select --harness claude-code, codex or antigravity"
            ),
        }
    }
}

pub(crate) fn render(harness: Option<Harness>) -> Result<String> {
    let harness = match harness {
        Some(harness) => harness,
        None => Harness::detected()?,
    };
    let sig = match harness {
        Harness::ClaudeCode => claude::read(),
        Harness::Codex => codex::read(),
        Harness::Antigravity => antigravity::read(),
    };
    Ok(sig.lines().join("\n"))
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd)]
struct Model {
    model: String,
    effort: String,
}

impl Model {
    fn new(model: &str, effort: &str) -> Self {
        Self {
            model: field(model).unwrap_or("unknown").to_owned(),
            effort: field(effort).unwrap_or("unknown").to_owned(),
        }
    }
}

struct Signature {
    model: Model,
    name: String,
    harness: &'static str,
    session: String,
    helpers: BTreeMap<Model, usize>,
    team: BTreeSet<String>,
    research: Research,
}

impl Signature {
    fn new(harness: &'static str, session: &str) -> Self {
        Self {
            model: Model::new("", ""),
            name: String::new(),
            harness,
            session: field(session).unwrap_or_default().to_owned(),
            helpers: BTreeMap::new(),
            team: BTreeSet::new(),
            research: Research::default(),
        }
    }

    fn lines(&self) -> Vec<String> {
        let mut model = format!("{} ({})", self.model.model, self.model.effort);
        if let Some(name) = field(&self.name) {
            model.push_str(", ");
            model.push_str(name);
        }
        let mut harness = self.harness.to_owned();
        if !self.session.is_empty() {
            harness = format!("{harness} ({})", self.session);
        } else if self.harness == "Antigravity" {
            harness.push_str(" (unknown)");
        }
        let mut lines = vec!["_".into(), model, harness];
        if !self.helpers.is_empty() {
            let names = self
                .helpers
                .iter()
                .map(|(m, n)| format!("{n} × {} ({})", m.model, m.effort))
                .collect::<Vec<_>>();
            lines.push(format!("Sub-agents: {}", names.join(", ")));
        }
        if !self.team.is_empty() {
            lines.push(format!(
                "Team: {}",
                self.team
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        let counts = [
            (self.research.searches, "searches"),
            (self.research.pages, "pages"),
            (self.research.docs, "doc lookups"),
        ];
        let activity = counts
            .into_iter()
            .filter(|(count, _)| *count > 0)
            .map(|(count, label)| format!("{count} {label}"))
            .collect::<Vec<_>>();
        if !activity.is_empty() {
            lines.push(format!("Research: {}", activity.join(", ")));
        }
        lines
    }
}

#[derive(Default)]
struct Research {
    searches: usize,
    pages: usize,
    docs: usize,
}

fn env(key: &str) -> String {
    std::env::var(key).unwrap_or_default()
}

fn field(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    (!trimmed.is_empty()
        && !value.chars().any(|c| {
            c.is_control() || matches!(c, '(' | ')' | ',' | ':' | '\u{2028}' | '\u{2029}')
        }))
    .then_some(trimmed)
}

// Session identifiers may be printed and used as a single local directory component.
fn session_id(value: &str) -> &str {
    field(value)
        .filter(|s| {
            !s.contains(['/', '\\'])
                && matches!(
                    Path::new(s).components().collect::<Vec<_>>().as_slice(),
                    [Component::Normal(_)]
                )
        })
        .unwrap_or_default()
}

fn string(value: &Value) -> &str {
    value.as_str().unwrap_or_default()
}

fn array(value: &Value) -> &[Value] {
    value.as_array().map(Vec::as_slice).unwrap_or_default()
}

fn rows(path: &Path) -> impl Iterator<Item = Value> {
    File::open(path).ok().into_iter().flat_map(|file| {
        BufReader::new(file)
            .split(b'\n')
            .map_while(std::result::Result::ok)
            .filter_map(|line| serde_json::from_slice::<Value>(&line).ok())
            .filter(Value::is_object)
    })
}

fn entries(path: &Path) -> Vec<PathBuf> {
    let mut result = fs::read_dir(path)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .collect::<Vec<_>>();
    result.sort();
    result
}

fn filename(path: &Path) -> &str {
    path.file_name().and_then(OsStr::to_str).unwrap_or_default()
}

fn git(args: &[&str]) -> String {
    Command::new("git")
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_default()
}

// Compare Git's seconds with subsecond harness receipts in UTC.
fn timestamp(value: &str) -> i64 {
    DateTime::parse_from_rfc3339(value)
        .map(|date| date.timestamp_micros())
        .unwrap_or_default()
}

fn git_timestamp(args: &[&str]) -> i64 {
    git(args)
        .parse::<i64>()
        .unwrap_or_default()
        .saturating_mul(1_000_000)
}

fn last_signed(harness: &str, session: &str) -> i64 {
    if session.is_empty() {
        return 0;
    }
    // Escape the exact signature line using Git's extended-regexp syntax.
    // regex::escape also quotes spaces and '-' which git's POSIX ERE does not require.
    let escaped = session
        .chars()
        .flat_map(|c| {
            if ".^$*+?()[]{}|\\".contains(c) {
                vec!['\\', c]
            } else {
                vec![c]
            }
        })
        .collect::<String>();
    let grep = format!("--grep=^{harness} \\({escaped}\\)$");
    git_timestamp(&[
        "log",
        "--all",
        "HEAD",
        "-1",
        "--extended-regexp",
        &grep,
        "--format=%ct",
    ])
}

fn receipt_cutoff(sha: &str, completed: i64) -> i64 {
    let time = git_timestamp(&["show", "-s", "--format=%ct", &format!("{sha}^{{commit}}")]);
    if time != 0 && time / 1_000_000 == completed / 1_000_000 {
        completed
    } else {
        time
    }
}

fn regex(text: &str) -> Regex {
    Regex::new(text).unwrap_or_else(|e| unreachable!("invalid built-in pattern: {e}"))
}
static RECEIPT: LazyLock<Regex> = LazyLock::new(|| regex(r"(?m)^\[[^\]\n]+ ([0-9a-f]{7,64})\] "));
static AGMSG_SEND: LazyLock<Regex> =
    LazyLock::new(|| regex(r"agmsg/scripts/send\.sh\s+\w+\s+[\w-]+\s+([A-Za-z][\w.-]*)"));
static AGMSG_LOOP: LazyLock<Regex> = LazyLock::new(|| {
    regex(r"for\s+(\w+)\s+in\s+([^;]+);\s*do[^\n]*agmsg/scripts/send\.sh\s+\w+\s+[\w-]+\s+\$(\w+)")
});

fn add_peer(team: &mut BTreeSet<String>, name: &str) {
    if let Some(peer) = field(name) {
        team.insert(peer.to_owned());
    }
}

fn agmsg_peers(command: &str, team: &mut BTreeSet<String>) {
    for m in AGMSG_SEND.captures_iter(command) {
        add_peer(team, &m[1]);
    }
    for m in AGMSG_LOOP.captures_iter(command) {
        if m[1] == m[3] {
            for name in m[2].split_whitespace() {
                if name
                    .chars()
                    .all(|c| c.is_alphanumeric() || ".-_".contains(c))
                {
                    add_peer(team, name);
                }
            }
        }
    }
}
