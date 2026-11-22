//! Summarizes the authored intentions that arrive together in a pull request.

use crate::subject::read_tags;
use crate::vocabulary::{Slot, Token, Vocabulary};

const AREA_LIMIT: usize = 3;
const PR_LIMIT: usize = 9;
const MERGE_LIMIT: usize = 14;

#[derive(Clone, Copy, PartialEq, Eq)]
struct Heading<'v> {
    area: Option<&'v str>,
    activity: &'v str,
    modifier: Option<&'v str>,
}

fn heading<'v>(subject: &str, vocab: &'v Vocabulary) -> Option<Heading<'v>> {
    let cluster = subject.split_whitespace().next()?;
    if cluster.is_ascii() {
        return None;
    }
    if let Some((group, entry)) = vocab.find(cluster)
        && group.slot == Slot::Marker
    {
        return Some(Heading {
            area: None,
            activity: &entry.emoji,
            modifier: None,
        });
    }
    // A historical heading may come from a different ticket, scope or area policy.
    let tags = read_tags(vocab, true, cluster).ok()?;
    Some(Heading {
        area: tags.area.map(|e| e.emoji.as_str()),
        activity: &tags.second.1.emoji,
        modifier: tags.modifier.map(|e| e.emoji.as_str()),
    })
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
    let (groups, human) = title.strip_prefix('[')?.split_once("] ")?;
    if human.trim().is_empty() {
        return None;
    }
    let activity = |slot| slot != Slot::Area;
    if let Some((left, right)) = groups.split_once(" / ") {
        let areas = group_slots(left, AREA_LIMIT, vocab, |slot| slot == Slot::Area)?;
        if ![PR_LIMIT, MERGE_LIMIT]
            .iter()
            .any(|limit| group_slots(right, limit - areas, vocab, activity).is_some())
        {
            return None;
        }
    } else if ![PR_LIMIT, MERGE_LIMIT]
        .iter()
        .any(|&limit| group_slots(groups, limit, vocab, activity).is_some())
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

/// Recognize a branch caption from a stored subject and its separately read body.
pub fn is_branch_caption(subject: &str, body: &str, vocab: &Vocabulary) -> bool {
    let Some((route, summary)) = without_inventory(subject, vocab).split_once(": ") else {
        return false;
    };
    let Some((sources, target)) = route.split_once(" into ") else {
        return false;
    };
    if sources.is_empty() || target.is_empty() || summary.is_empty() {
        return false;
    }
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
    /// The landed commit subject, including the GitHub PR suffix.
    pub subject: String,
    /// Incoming non-merge commits with a recognized heading.
    pub recognized: usize,
    /// All incoming non-merge commits, including those with unrecognized headings.
    pub total: usize,
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
    let inventory = Inventory::from_subjects(subjects, vocab);
    let render_title = |limit| match inventory.render(limit) {
        Some(groups) => format!("[{groups}] {human}"),
        None => human.to_owned(),
    };
    let title = render_title(limit);
    let previous_title = contains_merge
        .then(|| render_title(PR_LIMIT))
        .filter(|old| old != &title);
    Ok(Caption {
        previous_title,
        subject: format!("{title} (#{number})"),
        title,
        recognized: inventory.recognized,
        total: subjects.len(),
    })
}

/// Render the leading, bracketed inventory for a local branch merge, if any
/// incoming authored commit carries recognized symbols.
pub fn merge_inventory(subjects: &[String], vocab: &Vocabulary) -> Option<String> {
    Inventory::from_subjects(subjects, vocab)
        .render(MERGE_LIMIT)
        .map(|groups| format!("[{groups}]"))
}

struct Inventory<'v> {
    areas: Vec<&'v str>,
    activities: Vec<&'v str>,
    one_heading: bool,
    recognized: usize,
}

impl<'v> Inventory<'v> {
    fn from_subjects(subjects: &[String], vocab: &'v Vocabulary) -> Self {
        let mut inventory = Self {
            areas: Vec::new(),
            activities: Vec::new(),
            one_heading: true,
            recognized: 0,
        };
        let mut first = None;
        for item in subjects.iter().filter_map(|s| heading(s, vocab)) {
            inventory.recognized += 1;
            inventory.one_heading &= item == *first.get_or_insert(item);
            if let Some(area) = item.area {
                push_distinct(&mut inventory.areas, area);
            }
            push_distinct(&mut inventory.activities, item.activity);
            if let Some(modifier) = item.modifier {
                push_distinct(&mut inventory.activities, modifier);
            }
        }
        inventory
    }

    fn render(&self, limit: usize) -> Option<String> {
        if self.recognized == 0 {
            return None;
        }
        Some(if self.one_heading {
            format!("`{}{}`", self.areas.concat(), self.activities.concat())
        } else if self.areas.is_empty() {
            render_group(&self.activities, limit)
        } else {
            let area_slots = self.areas.len().min(AREA_LIMIT);
            let left = render_group(&self.areas, AREA_LIMIT);
            let right = render_group(&self.activities, limit - area_slots);
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
        assert_eq!(caption(&["🧩✨🧪 old ticket"]), "[`🧩✨🧪`] Preview");
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
