//! Summarizes the authored intentions that arrive together in a pull request.

use std::collections::BTreeMap;

use crate::config::Config;
use crate::subject::{REVIEW_RESPONSE, authored_heading, read_tags};
use crate::vocabulary::{Slot, Token, Vocabulary};

const AREA_LIMIT: usize = 3;
const PR_LIMIT: usize = 9;
const MERGE_LIMIT: usize = 14;

#[derive(Clone, Copy, PartialEq, Eq)]
/// Vocabulary symbols in an authored subject, independent of current project policy.
pub struct Heading<'s, 'v> {
    /// The authored inner cluster, before normalizing variation selectors.
    cluster: &'s str,
    /// First token after the cluster, including after a review-response wrapper.
    pub ticket: Option<&'s str>,
    /// True for an explicitly authored review response.
    pub review_response: bool,
    /// Canonical area symbol, when present.
    pub area: Option<&'v str>,
    /// Canonical activity or marker symbol.
    pub activity: &'v str,
    /// Canonical modifier symbol, when present.
    pub modifier: Option<&'v str>,
}

impl<'v> Heading<'_, 'v> {
    /// Canonical symbols in their authored order, including standalone markers.
    pub fn symbols(self) -> impl Iterator<Item = &'v str> {
        [self.area, Some(self.activity), self.modifier]
            .into_iter()
            .flatten()
    }

    /// Display the leading emoji run canonically, retaining the original spacing and text.
    pub fn display(self, subject: &str) -> String {
        subject.replacen(self.cluster, &self.symbols().collect::<String>(), 1)
    }
}

/// Read the leading symbols of a historical authored subject.
pub fn heading<'s, 'v>(subject: &'s str, vocab: &'v Vocabulary) -> Option<Heading<'s, 'v>> {
    let trimmed = subject.trim_start();
    let (cluster, rest, review_response) = if trimmed.starts_with(REVIEW_RESPONSE) {
        authored_heading(trimmed).ok()?
    } else {
        // Older bare headings can separate their fields with tabs or other whitespace.
        let (cluster, rest) = trimmed
            .split_once(char::is_whitespace)
            .unwrap_or((trimmed, ""));
        (cluster, rest, false)
    };
    let ticket = rest.split_whitespace().next();
    if cluster.is_ascii() {
        return None;
    }
    if let Some((group, entry)) = vocab.find(cluster)
        && group.slot == Slot::Marker
        && !review_response
    {
        return Some(Heading {
            cluster,
            ticket,
            review_response,
            area: None,
            activity: &entry.emoji,
            modifier: None,
        });
    }
    // A historical heading may come from a different ticket, scope or area policy.
    let tags = read_tags(vocab, true, cluster).ok()?;
    Some(Heading {
        cluster,
        ticket,
        review_response,
        area: tags.area.map(|e| e.emoji.as_str()),
        activity: &tags.second.1.emoji,
        modifier: tags.modifier.map(|e| e.emoji.as_str()),
    })
}

// The two PR-state signs have a fixed order independent of topological commit order.
const MODIFIERS: [&str; 3] = ["🚧", "🧪", "💥"];

/// Number of provisional signs in a canonical managed caption prefix.
pub fn sign_count(signs: &str) -> Option<usize> {
    match signs {
        "🚧" | "🧪" => Some(1),
        "🚧🧪" => Some(2),
        _ => None,
    }
}

fn push_distinct<'a>(symbols: &mut Vec<&'a str>, symbol: &'a str) {
    if !symbols.contains(&symbol) {
        symbols.push(symbol);
    }
}

fn render_group(symbols: &[&str], limit: usize) -> String {
    if symbols.len() <= limit {
        return format!("`{}`", symbols.concat());
    }
    // An ellipsis takes the final slot and sits outside the code span in GitHub's file list.
    format!("`{}`…", symbols[..limit - 1].concat())
}

/// Count the canonical symbols in a group previously produced by `render_group`.
fn group_slots(
    group: &str,
    limit: usize,
    vocab: &Vocabulary,
    allowed: impl Fn(Slot) -> bool,
) -> Option<usize> {
    let (run, omitted) = match group.strip_suffix('…') {
        Some(run) => (run, true),
        None => (group, false),
    };
    let cluster = run.strip_prefix('`')?.strip_suffix('`')?;
    let tokens = vocab.tokens(cluster).ok()?;
    let slots = tokens.len() + usize::from(omitted);
    if tokens.is_empty() || slots > limit || (omitted && slots != limit) {
        return None;
    }
    let mut symbols = Vec::new();
    for token in tokens {
        let Token::Known(group, entry) = token else {
            return None;
        };
        let symbol = entry.emoji.as_str();
        if !allowed(group.slot)
            || (symbols.is_empty() && group.slot == Slot::Modifier)
            || symbols.contains(&symbol)
        {
            return None;
        }
        symbols.push(symbol);
    }
    (symbols.concat() == cluster).then_some(slots)
}

// Only the renderer's leading inventory is ours. PR authors also use labels such
// as "[WIP]", Markdown links and inline code in brackets at the start of a title.
fn after_inventory<'a>(title: &'a str, vocab: &Vocabulary) -> Option<&'a str> {
    let (signs, inventory) = title
        .split_once(' ')
        .and_then(|(prefix, rest)| sign_count(prefix).map(|count| (count, rest)))
        .unwrap_or((0, title));
    let (groups, human) = inventory.strip_prefix('[')?.split_once("] ")?;
    if human.trim().is_empty() {
        return None;
    }
    let activity = |slot| slot != Slot::Area;
    if let Some((left, right)) = groups.split_once(" / ") {
        let areas = group_slots(left, AREA_LIMIT, vocab, |slot| slot == Slot::Area)?;
        if ![PR_LIMIT, MERGE_LIMIT]
            .iter()
            .any(|limit| group_slots(right, limit - signs - areas, vocab, activity).is_some())
        {
            return None;
        }
    } else if ![PR_LIMIT, MERGE_LIMIT]
        .iter()
        .any(|&limit| group_slots(groups, limit - signs, vocab, activity).is_some())
    {
        let cluster = groups.strip_prefix('`')?.strip_suffix('`')?;
        let tags = read_tags(vocab, true, cluster).ok()?;
        let canonical = [
            tags.area?.emoji.as_str(),
            &tags.second.1.emoji,
            tags.modifier.map_or("", |entry| entry.emoji.as_str()),
        ];
        if canonical.concat() != cluster {
            return None;
        }
    }
    Some(human)
}

/// Remove one canonical caption inventory, preserving the title and PR references.
pub fn without_inventory<'a>(title: &'a str, vocab: &Vocabulary) -> &'a str {
    after_inventory(title, vocab).unwrap_or(title)
}

/// Identify the local merge message produced from Git's recorded sources. Only use
/// this after verifying the merge state or the stored commit's parent count.
pub fn is_branch_message(message: &str, vocab: &Vocabulary) -> bool {
    message
        .split_once('\n')
        .is_some_and(|(subject, body)| is_branch_caption(subject, body, vocab))
}

/// Whether a displayed branch label consists of Git refname characters.
/// Parsing a caption still needs the merge state to establish its meaning.
pub fn valid_branch_label(name: &str) -> bool {
    !name.is_empty()
        && !name
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || "~^:?*[\\".contains(c))
}

/// Read the source label from a possible branch-merge subject. A match is a
/// display hint only: a subject by itself cannot establish commit ancestry.
pub fn branch_sources<'a>(subject: &'a str, vocab: &Vocabulary) -> Option<&'a str> {
    let (route, summary) = without_inventory(subject, vocab).split_once(": ")?;
    let (sources, target) = route.split_once(" into ")?;
    // The renderer quotes sources starting with Git's comment character and
    // identifies a same-name source from FETCH_HEAD as "fetched <branch>".
    let names = sources
        .strip_prefix('\'')
        .and_then(|s| s.strip_suffix('\''))
        .unwrap_or(sources);
    (valid_branch_label(target)
        && !summary.trim().is_empty()
        && names
            .split(", ")
            .all(|name| valid_branch_label(name.strip_prefix("fetched ").unwrap_or(name))))
    .then_some(sources)
}

/// Recognize a branch caption from a stored subject and its separately read body.
pub fn is_branch_caption(subject: &str, body: &str, vocab: &Vocabulary) -> bool {
    let Some(sources) = branch_sources(subject, vocab) else {
        return false;
    };
    body.lines()
        .find(|line| !line.is_empty())
        .and_then(|line| line.strip_prefix("Arrived from "))
        .and_then(|line| line.strip_prefix(sources))
        .and_then(|rest| rest.strip_prefix(" ("))
        .and_then(|rest| rest.strip_suffix("):"))
        .is_some_and(|n| n.parse::<usize>().is_ok())
}

fn human_title<'a>(mut title: &'a str, number: u64, vocab: &Vocabulary) -> &'a str {
    let old_prefix = format!("Merge #{number} ");
    let suffix = format!(" (#{number})");
    loop {
        let candidate = title.strip_prefix(&old_prefix).unwrap_or(title);
        if let Some(rest) = after_inventory(candidate, vocab) {
            title = rest;
        } else if let Some(rest) = title.strip_suffix(&suffix)
            && !rest.trim().is_empty()
        {
            title = rest;
        } else {
            return title;
        }
    }
}

/// Evidence shown with a preview. Coverage counts commits before symbol deduplication.
#[derive(serde::Serialize)]
pub struct Caption {
    /// The PR title shown before landing.
    pub title: String,
    /// The former nine-slot title when it differs, for verifying historical PR landings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_title: Option<String>,
    /// All modifier glyphs observed in authored headings, in stable display order.
    pub modifiers: Vec<String>,
    /// Distinct configured ticket IDs for each modifier, in first-appearance order.
    pub modifier_tickets: BTreeMap<String, Vec<String>>,
    // Earlier renderers included provisional signs inside the activity group and could
    // truncate a modifier. Compare exact prior output when verifying historical landings.
    #[serde(skip)]
    historical_titles: Vec<String>,
    /// The landed commit subject, including the GitHub PR suffix.
    pub subject: String,
    /// Incoming non-merge commits with a recognized heading.
    pub recognized: usize,
    /// All incoming non-merge commits, including those with unrecognized headings.
    pub total: usize,
}

impl Caption {
    /// Match an exact modern or historical rendering of this verified incoming inventory.
    pub fn matches_landed_title(&self, local: &str) -> bool {
        self.title == local
            || self.previous_title.as_deref() == Some(local)
            || self.historical_titles.iter().any(|title| title == local)
    }
}

/// Formats the landed commit subject from its authored commits, in first-appearance order.
/// Returns an error for a zero PR number or a title unsuitable for a single commit line.
pub fn format_caption(
    number: u64,
    title: &str,
    subjects: &[String],
    vocab: &Vocabulary,
) -> Result<String, String> {
    Ok(summarize(number, title, subjects, vocab)?.subject)
}

/// Build the rendered caption and its commit coverage from the same inventory.
pub fn summarize(
    number: u64,
    title: &str,
    subjects: &[String],
    vocab: &Vocabulary,
) -> Result<Caption, String> {
    summarize_pr(number, title, subjects, false, vocab)
}

/// Format a PR title from a complete incoming revision range. A PR introducing
/// an integration commit uses the larger inventory reserved for branch merges.
pub fn summarize_pr(
    number: u64,
    title: &str,
    subjects: &[String],
    contains_merge: bool,
    vocab: &Vocabulary,
) -> Result<Caption, String> {
    let settings = Config::standard().map_err(|err| err.to_string())?;
    summarize_pr_with_tickets(number, title, subjects, contains_merge, vocab, |text| {
        settings.is_ticket(text)
    })
}

/// Use a project's configured ticket syntax for machine-readable modifier evidence.
pub fn summarize_pr_with_tickets(
    number: u64,
    title: &str,
    subjects: &[String],
    contains_merge: bool,
    vocab: &Vocabulary,
    is_ticket: impl Fn(&str) -> bool,
) -> Result<Caption, String> {
    if number == 0 {
        return Err("use a positive pull request number".to_string());
    }
    if title.trim().is_empty()
        || title
            .chars()
            .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'))
    {
        return Err("use a nonempty, single-line pull request title".to_string());
    }

    let human = human_title(title, number, vocab);
    let limit = if contains_merge {
        MERGE_LIMIT
    } else {
        PR_LIMIT
    };
    let inventory = Inventory::from_subjects(subjects, vocab, is_ticket);
    let modifiers = MODIFIERS
        .iter()
        .filter(|m| inventory.modifier_tickets.contains_key(**m))
        .map(|m| (*m).to_owned())
        .collect();
    let signs = inventory.signs();
    let mut text = human;
    // Authors can type their own supported sign ahead of a new title. The editable
    // text still owns other emoji and titles made up entirely of a sign.
    while let Some((manual, rest)) = text.split_once(' ')
        && sign_count(manual).is_some()
        && !rest.trim().is_empty()
        && manual.chars().all(|sign| signs.contains(sign))
    {
        text = rest;
    }
    let render_title = |limit| match inventory.prefix(limit, &signs) {
        Some(prefix) => format!("{prefix} {text}"),
        None => human.to_owned(),
    };
    let title = render_title(limit);
    let previous_title = contains_merge
        .then(|| render_title(PR_LIMIT))
        .filter(|old| old != &title);
    let mut historical_titles = Vec::new();
    if !inventory.modifier_tickets.is_empty() {
        for width in [limit, PR_LIMIT] {
            if let Some(groups) = inventory.render(&inventory.activities, width, false) {
                let old = format!("[{groups}] {human}");
                if old != title && !historical_titles.contains(&old) {
                    historical_titles.push(old);
                }
            }
        }
    }
    Ok(Caption {
        previous_title,
        subject: format!("{title} (#{number})"),
        title,
        historical_titles,
        modifiers,
        modifier_tickets: inventory.modifier_tickets,
        recognized: inventory.recognized,
        total: subjects.len(),
    })
}

/// Render the leading, bracketed inventory for a local branch merge, if any
/// incoming authored commit carries recognized symbols.
pub fn merge_inventory(subjects: &[String], vocab: &Vocabulary) -> Option<String> {
    let inventory = Inventory::from_subjects(subjects, vocab, |_| false);
    inventory.prefix(MERGE_LIMIT, &inventory.signs())
}

struct Inventory<'v> {
    areas: Vec<&'v str>,
    activities: Vec<&'v str>,
    one_heading: bool,
    recognized: usize,
    modifier_tickets: BTreeMap<String, Vec<String>>,
}

impl<'v> Inventory<'v> {
    fn from_subjects(
        subjects: &[String],
        vocab: &'v Vocabulary,
        is_ticket: impl Fn(&str) -> bool,
    ) -> Self {
        let mut inventory = Self {
            areas: Vec::new(),
            activities: Vec::new(),
            one_heading: true,
            recognized: 0,
            modifier_tickets: BTreeMap::new(),
        };
        let mut first = None;
        for subject in subjects {
            let Some(item) = heading(subject, vocab) else {
                continue;
            };
            inventory.recognized += 1;
            let authored_tags = (item.area, item.activity, item.modifier);
            inventory.one_heading &= authored_tags == *first.get_or_insert(authored_tags);
            if let Some(area) = item.area {
                push_distinct(&mut inventory.areas, area);
            }
            push_distinct(&mut inventory.activities, item.activity);
            if let Some(modifier) = item.modifier {
                push_distinct(&mut inventory.activities, modifier);
                let tickets = inventory
                    .modifier_tickets
                    .entry(modifier.to_owned())
                    .or_default();
                if let Some(ticket) = item.ticket
                    && ticket != "—"
                    && is_ticket(ticket)
                    && !tickets.iter().any(|seen| seen == ticket)
                {
                    tickets.push(ticket.to_owned());
                }
            }
        }
        inventory
    }

    fn signs(&self) -> String {
        MODIFIERS[..2]
            .iter()
            .copied()
            .filter(|m| self.modifier_tickets.contains_key(*m))
            .collect()
    }

    fn prefix(&self, limit: usize, signs: &str) -> Option<String> {
        let activities: Vec<_> = self
            .activities
            .iter()
            .copied()
            .filter(|symbol| !MODIFIERS[..2].contains(symbol))
            .collect();
        let groups = self.render(&activities, limit - signs.chars().count(), true)?;
        if signs.is_empty() {
            Some(format!("[{groups}]"))
        } else {
            Some(format!("{signs} [{groups}]"))
        }
    }

    fn render(&self, activities: &[&str], limit: usize, keep_breaking: bool) -> Option<String> {
        if self.recognized == 0 {
            return None;
        }
        let render_activities = |slots| {
            if keep_breaking && activities.len() > slots && activities[slots - 1..].contains(&"💥")
            {
                // Keep the first activities in their original order, reserving the last
                // visible slot for the breaking modifier and one for the ellipsis.
                format!("`{}💥`…", activities[..slots - 2].concat())
            } else {
                render_group(activities, slots)
            }
        };
        Some(if self.one_heading {
            format!("`{}{}`", self.areas.concat(), activities.concat())
        } else if self.areas.is_empty() {
            render_activities(limit)
        } else {
            let area_slots = self.areas.len().min(AREA_LIMIT);
            let left = render_group(&self.areas, AREA_LIMIT);
            let right = render_activities(limit - area_slots);
            format!("{left} / {right}")
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn caption(subjects: &[&str]) -> String {
        let vocab = Vocabulary::builtin().unwrap();
        let inputs: Vec<_> = subjects.iter().map(|s| s.to_string()).collect();
        summarize(17, "Preview", &inputs, &vocab).unwrap().title
    }

    #[test]
    fn equivalent_headings_remain_one_complete_sequence() {
        assert_eq!(
            caption(&[
                "🧑‍💻🛡 #31 (tools): refuse an unsafe operation",
                "🧑‍💻🛡️ #42 (tools): guard a second operation",
            ]),
            "[`🧑‍💻🛡️`] Preview"
        );
        assert_eq!(caption(&["🧩✨🧪 old ticket"]), "🧪 [`🧩✨`] Preview");
    }

    #[test]
    fn mixed_headings_preserve_first_appearance_in_both_groups() {
        assert_eq!(
            caption(&["👷🔐", "🤖📝", "🔺", "👷🔌", "👷🔌", "👷🔥", "👷🐛", "📝"]),
            "[`👷🤖` / `🔐📝🔺🔌🔥🐛`] Preview"
        );
        assert_eq!(caption(&["🧩✨", "🧩✨💥"]), "[`🧩` / `✨💥`] Preview");
    }

    #[test]
    fn historic_activity_and_standalone_markers_need_no_guessed_area() {
        assert_eq!(caption(&["🔖 release 1.0"]), "[`🔖`] Preview");
        assert_eq!(caption(&["📝 old style", "🔖 release"]), "[`📝🔖`] Preview");
    }

    #[test]
    fn untagged_and_invalid_headings_contribute_no_symbols() {
        assert_eq!(
            caption(&[
                "Merge #3 [`🧩📝`] Foo",
                "Revert \"🎨✨ old\"",
                "🌮📝",
                "🎨📝🌮",
                "🎨",
                "📝🔖"
            ]),
            "Preview"
        );
    }

    #[test]
    fn nine_distinct_symbols_fit_and_each_overflow_marks_its_own_side() {
        let mut rows = vec!["🧱🔧", "🎨✨", "👷📝", "🎨🛂", "👷♻", "🧱☑"];
        assert_eq!(caption(&rows), "[`🧱🎨👷` / `🔧✨📝🛂♻️☑️`] Preview");
        rows.push("🧩☑");
        assert_eq!(caption(&rows), "[`🧱🎨`… / `🔧✨📝🛂♻️☑️`] Preview");
        rows.push("🧱🔊");
        assert_eq!(caption(&rows), "[`🧱🎨`… / `🔧✨📝🛂♻️`…] Preview");
        rows.remove(6);
        assert_eq!(caption(&rows), "[`🧱🎨👷` / `🔧✨📝🛂♻️`…] Preview");
    }

    #[test]
    fn refresh_replaces_only_generated_prefixes_and_the_same_pr_suffix() {
        let vocab = Vocabulary::builtin().unwrap();
        let subjects = ["🎨✨ #3 (ui): preview".to_string()];
        let human = "[WIP] Show `preview` (#9)";
        let expected = format!("[`🎨✨`] {human}");
        for input in [
            human.to_owned(),
            format!("[`🧱👷` / `🛂🔧`] {human}"),
            format!("Merge #17 [`🧑‍💻⬆️`] {human} (#17)"),
            format!("Merge #17 [`🧩👷`… / `🐛♻️✨🔊🔐`…] {human}"),
            format!("{expected} (#17) (#17)"),
            format!("{human} (#17)"),
            format!("{expected} (#17)"),
            expected.clone(),
        ] {
            let caption = summarize(17, &input, &subjects, &vocab).unwrap();
            assert_eq!(caption.title, expected, "{input}");
            assert_eq!(caption.subject, format!("{expected} (#17)"), "{input}");
        }
    }

    #[test]
    fn preserves_human_labels_code_links_and_different_pr_reference() {
        let vocab = Vocabulary::builtin().unwrap();
        for human in [
            "[`breaking`] docs",
            "[`🧱`] infrastructure",
            "[`💥`] api break",
            "[`v3` / `v4`] migrations",
            "[`👷` / `flake`] CI",
            "[`👷`… / `🐛`] CI",
            "[`🐛`](./bugs) Fix bugs",
            "Merge #18 [`🎨✨`] Preview",
            "Merge #17 automatically on green",
            "Scope  (#123)",
        ] {
            let caption = summarize(17, human, &[], &vocab).unwrap();
            assert_eq!(caption.title, human, "{human}");
            assert_eq!(caption.subject, format!("{human} (#17)"));
        }
    }

    #[test]
    fn no_coverage_removes_an_obsolete_prefix_and_keeps_the_title() {
        let vocab = Vocabulary::builtin().unwrap();
        let result = summarize(
            17,
            "[`📝`] [WIP] Update docs (#17)",
            &["prose".into()],
            &vocab,
        )
        .unwrap();
        assert_eq!(result.title, "[WIP] Update docs");
        assert_eq!(result.subject, "[WIP] Update docs (#17)");
        assert_eq!((result.recognized, result.total), (0, 1));
    }

    #[test]
    fn late_modifiers_survive_both_caps_and_repeated_title_sync() {
        let vocab = Vocabulary::builtin().unwrap();
        let rows: Vec<_> = [
            "🧱🔧",
            "🎨✨",
            "👷📝",
            "🧩🛂",
            "👷♻",
            "🧱☑",
            "👷🔊",
            "👷🔥",
            "👷🐛",
            "👷🔐",
            "👷🔌",
            "👷🔒",
            "👷🔧🧪 #42",
            "🎨✨💥 #77",
            "🧱🔥🚧 #42",
            "🧱🐛🚧 #42",
            "👷🐛🚧 #43",
        ]
        .map(String::from)
        .into();
        for wide in [false, true] {
            let preview = |name| summarize_pr(17, name, &rows, wide, &vocab).unwrap();
            let result = preview("Preview");
            assert_eq!(result.modifiers, ["🚧", "🧪", "💥"]);
            assert_eq!(result.modifier_tickets["🚧"], ["#42", "#43"]);
            assert_eq!(result.modifier_tickets["🧪"], ["#42"]);
            assert_eq!(result.modifier_tickets["💥"], ["#77"]);
            assert!(result.title.starts_with("🚧🧪 [`🧱🎨`… / `"));
            assert!(result.title.ends_with("💥`…] Preview"));
            for symbol in ["🚧", "🧪", "💥"] {
                assert_eq!(result.title.matches(symbol).count(), 1);
            }
            assert_eq!(without_inventory(&result.title, &vocab), "Preview");
            assert_eq!(preview(&result.subject).title, result.title);
            assert!(result.matches_landed_title("[`🧱🎨`… / `🔧✨📝🛂♻️`…] Preview"));
            assert!(!result.matches_landed_title("[`🧱🎨`… / `🔧✨📝🛂♻️`…] Other PR"));
            assert_eq!(
                summarize(17, &result.title, &[], &vocab).unwrap().title,
                "Preview"
            );
        }
    }

    #[test]
    fn resync_follows_authored_signs_and_preserves_other_leading_emoji() {
        let vocab = Vocabulary::builtin().unwrap();
        let rows = ["🎨✨🚧 #12".to_owned()];
        let expected = "🚧 [`🎨✨`] Greybox";
        for title in [
            "Greybox",
            "🚧 Greybox",
            "🧪 [`🎨✨`] Greybox",
            "[`🎨✨🚧`] Greybox",
        ] {
            assert_eq!(summarize(17, title, &rows, &vocab).unwrap().title, expected);
        }
        for title in [
            "🚧 Road works",
            "🧪 [WIP] Greybox",
            "🚧 [`unknown`] Road works",
        ] {
            let settled = summarize(17, title, &["🎨✨ #12".into()], &vocab).unwrap();
            assert_eq!(settled.title, format!("[`🎨✨`] {title}"));
            assert!(settled.modifiers.is_empty());
            assert_eq!(without_inventory(title, &vocab), title);
        }
        let data = summarize_pr_with_tickets(
            17,
            "Preview",
            &[
                "🧱✨🚧 DEMO-12".into(),
                "🧱✨🚧 DEMO-12".into(),
                "🧱✨🚧 #77".into(),
                "🧱🔧🧪 —".into(),
            ],
            false,
            &vocab,
            |t| t.starts_with("DEMO-"),
        )
        .unwrap();
        assert_eq!(data.modifier_tickets["🚧"], ["DEMO-12"]);
        assert!(data.modifier_tickets["🧪"].is_empty());
        let branch = merge_inventory(&rows, &vocab).unwrap();
        assert!(is_branch_caption(
            &format!("{branch} topic into trunk: Greybox"),
            "Arrived from topic (1):",
            &vocab
        ));
    }

    #[test]
    fn title_must_be_usable_as_a_single_commit_subject() {
        let vocab = Vocabulary::builtin().unwrap();
        for title in ["", " ", "two\nlines", "title\r", "title\0", "a\u{2028}b"] {
            assert!(format_caption(1, title, &[], &vocab).is_err());
        }
        assert!(format_caption(0, "Preview", &[], &vocab).is_err());
        assert_eq!(
            format_caption(17, "Add `--dry-run` to café", &[], &vocab).unwrap(),
            "Add `--dry-run` to café (#17)"
        );
    }
}
