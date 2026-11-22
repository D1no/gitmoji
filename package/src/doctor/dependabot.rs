//! Check the commit headings produced by an explicit Dependabot configuration.

use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use anyhow::{Context, Result, anyhow, ensure};
use gitmoji_triangle::config::Config;
use gitmoji_triangle::subject::{self, Subject, SubjectError};
use gitmoji_triangle::vocabulary::Vocabulary;
use serde::Deserialize;
use serde_yaml_ng::{Mapping, Value};

use super::{Row, Status};
use crate::history::{DEPENDABOT_AUTHOR, dependabot_message};

#[derive(Deserialize)]
struct Dependabot {
    updates: Vec<Update>,
    #[serde(default, rename = "multi-ecosystem-groups")]
    groups: BTreeMap<String, Update>,
}

#[derive(Deserialize)]
struct Update {
    #[serde(rename = "package-ecosystem")]
    ecosystem: Option<String>,
    #[serde(rename = "multi-ecosystem-group")]
    group: Option<String>,
    #[serde(rename = "commit-message")]
    message: Option<Mapping>,
}

#[derive(Default)]
struct Findings {
    checked: usize,
    errors: Vec<String>,
    warnings: Vec<String>,
}

fn read(top: &Path, file: &str) -> Result<Option<Dependabot>> {
    let path = top.join(file);
    match path.symlink_metadata() {
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        metadata => ensure!(
            metadata?.is_file() && path.canonicalize()?.starts_with(top.canonicalize()?),
            "{file} must be a regular repository file"
        ),
    }
    let text = fs::read_to_string(path).with_context(|| format!("cannot read {file}"))?;
    // Report the location without quoting potentially unrelated registry settings.
    let invalid = |error: serde_yaml_ng::Error| {
        let at = error
            .location()
            .map(|loc| format!(" at line {}, column {}", loc.line(), loc.column()))
            .unwrap_or_default();
        anyhow!("{file} has invalid YAML or an invalid Dependabot configuration{at}")
    };
    let mut doc: Value = serde_yaml_ng::from_str(&text).map_err(invalid)?;
    doc.apply_merge().map_err(invalid)?;
    serde_yaml_ng::from_value(doc).map(Some).map_err(invalid)
}

pub(super) fn inspect(top: &Path, settings: Option<&Path>, vocab: &Vocabulary) -> Option<Row> {
    // Dependabot stops at the first file it finds in this order.
    for file in [".github/dependabot.yml", ".github/dependabot.yaml"] {
        let doc = match read(top, file) {
            Ok(None) => continue,
            Ok(Some(doc)) => doc,
            Err(error) => {
                return Some(Row::new(
                    "dependabot:prefixes",
                    Status::Mismatch,
                    error.to_string(),
                    "a readable Dependabot YAML file with an updates list",
                    format!(
                        "check {file} for YAML syntax and configuration errors, then rerun gitmoji doctor"
                    ),
                ));
            }
        };
        let mut findings = Findings::default();
        if doc
            .updates
            .iter()
            .chain(doc.groups.values())
            .all(|entry| entry.message.is_none())
        {
            return Some(findings.row("deps"));
        }
        let config = match settings.map_or_else(Config::standard, Config::load) {
            Ok(config) => config,
            Err(_) => {
                return Some(Row::new(
                    "dependabot:prefixes",
                    Status::Mismatch,
                    format!("cannot check {file} with the current Gitmoji settings"),
                    "readable Gitmoji project settings",
                    "correct the Gitmoji settings file, then rerun gitmoji doctor",
                ));
            }
        };
        findings.check_file(file, &doc, vocab, &config);
        let scope = config.scopes_allowed.first().map_or("deps", String::as_str);
        return Some(findings.row(scope));
    }
    None
}

impl Findings {
    fn row(self, example_scope: &str) -> Row {
        let mut row = if !self.errors.is_empty() {
            let example = format!("🧩⬆ — ({example_scope})");
            Row::new(
                "dependabot:prefixes",
                Status::Mismatch,
                self.errors.join("; "),
                format!("a bot prefix such as {example}"),
                format!(
                    "edit the named commit-message field using an area and scope for your project. See {}/references/dependabot.md",
                    crate::init::SKILL_DIR
                ),
            )
        } else if self.checked == 0 {
            Row::new(
                "dependabot:prefixes",
                Status::NotEnrolled,
                "no explicit Dependabot commit prefix",
                "",
                "",
            )
        } else {
            Row::new(
                "dependabot:prefixes",
                Status::Ok,
                if self.checked == 1 {
                    "checked 1 Dependabot heading".into()
                } else {
                    format!("checked {} Dependabot headings", self.checked)
                },
                "",
                "",
            )
        };
        row.warnings = self.warnings;
        row
    }

    fn check_file(&mut self, file: &str, doc: &Dependabot, vocab: &Vocabulary, config: &Config) {
        let supports_development = |update: &Update| {
            matches!(
                update.ecosystem.as_deref(),
                Some("bundler" | "composer" | "mix" | "maven" | "npm" | "pip" | "uv")
            )
        };
        for (i, update) in doc.updates.iter().enumerate() {
            if let Some(options) = &update.message {
                let location = format!("{file} updates[{i}].commit-message");
                if let Some(group) = &update.group {
                    self.errors.push(format!(
                        "{location} belongs in multi-ecosystem-groups[{group:?}]"
                    ));
                } else {
                    self.check_options(
                        &location,
                        options,
                        supports_development(update),
                        vocab,
                        config,
                    );
                }
            }
        }
        for (group, update) in &doc.groups {
            if let Some(options) = &update.message {
                let development = doc
                    .updates
                    .iter()
                    .filter(|u| u.group.as_ref() == Some(group))
                    .any(supports_development);
                self.check_options(
                    &format!("{file} multi-ecosystem-groups[{group:?}].commit-message"),
                    options,
                    development,
                    vocab,
                    config,
                );
            }
        }
    }

    fn check_options(
        &mut self,
        location: &str,
        options: &Mapping,
        supports_development: bool,
        vocab: &Vocabulary,
        config: &Config,
    ) {
        let include_scope = match options.get("include") {
            None => false,
            Some(v) if v.as_str() == Some("scope") => true,
            Some(_) => {
                self.errors
                    .push(format!("{location}.include must be scope"));
                return;
            }
        };
        let Some(prefix) = options.get("prefix") else {
            if options
                .get("prefix-development")
                .is_some_and(|value| !value.is_string())
            {
                self.errors
                    .push(format!("{location}.prefix-development must be a string"));
            } else if options.contains_key("prefix-development") || include_scope {
                self.warnings.push(format!(
                    "{location}: Dependabot infers its prefix from repository history. Set prefix to activate prefix-development or include."
                ));
            }
            return;
        };
        self.check_prefix(
            format!("{location}.prefix"),
            prefix,
            include_scope.then_some("deps"),
            vocab,
            config,
        );
        if let Some(dev) = options.get("prefix-development") {
            self.check_prefix(
                format!("{location}.prefix-development"),
                dev,
                include_scope.then_some("deps-dev"),
                vocab,
                config,
            );
        } else if include_scope && supports_development {
            // GitHub documents development dependency groups for these ecosystems.
            self.check_prefix(
                format!("{location}.prefix"),
                prefix,
                Some("deps-dev"),
                vocab,
                config,
            );
        }
    }

    fn check_prefix(
        &mut self,
        field: String,
        value: &Value,
        scope: Option<&str>,
        vocab: &Vocabulary,
        config: &Config,
    ) {
        self.checked += 1;
        let Some(prefix) = value.as_str() else {
            self.errors.push(format!("{field} must be a string"));
            return;
        };
        if prefix.trim().is_empty()
            || prefix
                .chars()
                .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'))
        {
            self.errors
                .push(format!("{field} must be a nonempty, single-line heading"));
            return;
        }
        if prefix.chars().count() > 50 {
            self.errors
                .push(format!("{field} exceeds Dependabot's 50-character limit"));
            return;
        }
        if scope.is_some()
            && matches!(
                subject::parse(vocab, config, &example_heading(prefix, None)),
                Ok(Subject::Tagged { .. })
            )
        {
            let message = format!("{field} already supplies a scope; omit include: scope");
            if !self.errors.contains(&message) {
                self.errors.push(message);
            }
            return;
        }
        let heading = example_heading(prefix, scope);
        // Dependabot performs these substitutions only when storing a commit.
        let mut stored = heading.clone();
        for (emoji, shortcode) in [("⬆️", ":arrow_up:"), ("🔒", ":lock:")] {
            if heading.contains(emoji) {
                let warning =
                    format!("{field}: Dependabot stores {emoji} as {shortcode} in commits");
                if !self.warnings.contains(&warning) {
                    self.warnings.push(warning);
                }
                stored = stored.replace(emoji, shortcode);
            }
        }
        match subject::parse(vocab, config, &heading) {
            Ok(Subject::Tagged { .. }) if stored != heading => {
                let normalized = dependabot_message(vocab, DEPENDABOT_AUTHOR, &stored);
                if !matches!(
                    subject::parse(vocab, config, &normalized),
                    Ok(Subject::Tagged { .. })
                ) {
                    self.errors.push(format!(
                        "{field}: the stored shortcode heading fails Gitmoji validation"
                    ));
                }
            }
            Ok(Subject::Tagged { .. }) => {}
            Ok(_) => self.errors.push(format!(
                "{field}: the prefix must describe an authored change"
            )),
            Err(SubjectError::NoEmojis { .. } | SubjectError::TicketFirst { .. }) => {
                self.errors.push(format!(
                    "{field}: start the prefix with the emojis for the dependency update"
                ))
            }
            Err(SubjectError::Backticks { .. }) => self.errors.push(format!(
                "{field}: remove the backticks around the prefix emojis"
            )),
            Err(error) => self.errors.push(format!("{field}: {error}")),
        }
    }
}

// Mirrors Dependabot's explicit prefix builder. It preserves a trailing space,
// appends its requested scope directly, then supplies the colon and word boundary.
fn example_heading(prefix: &str, scope: Option<&str>) -> String {
    let mut heading = prefix.to_owned();
    if let Some(scope) = scope {
        heading.push('(');
        heading.push_str(scope);
        heading.push(')');
    }
    if heading.ends_with(|c: char| c.is_ascii_alphanumeric() || matches!(c, ')' | ']')) {
        heading.push(':');
    }
    if !heading.ends_with(' ') {
        heading.push(' ');
    }
    heading.push_str("Bump dependencies");
    heading
}
