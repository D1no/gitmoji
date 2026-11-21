//! Summarizes the authored intentions that arrive together in a pull request.

use crate::subject::read_tags;
use crate::vocabulary::{Slot, Vocabulary};

const AREA_LIMIT: usize = 3;
const CAPTION_LIMIT: usize = 9;

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

/// Evidence shown with a preview. Coverage counts commits before symbol deduplication.
#[derive(serde::Serialize)]
pub struct Caption {
    /// The single-line GitHub subject.
    pub subject: String,
    /// Incoming non-merge commits with a recognized heading.
    pub recognized: usize,
    /// All incoming non-merge commits, including those with unrecognized headings.
    pub total: usize,
}

/// Formats a PR subject from its authored commit subjects, in first-appearance order.
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

    let total = subjects.len();
    let mut headings = subjects.iter().filter_map(|s| heading(s, vocab));
    let Some(first) = headings.next() else {
        return Ok(Caption {
            subject: format!("Merge #{number} {title}"),
            recognized: 0,
            total,
        });
    };
    let mut areas = Vec::new();
    let mut activities = Vec::new();
    let mut one_heading = true;
    let mut recognized = 0;
    for item in std::iter::once(first).chain(headings) {
        recognized += 1;
        one_heading &= item == first;
        if let Some(area) = item.area {
            push_distinct(&mut areas, area);
        }
        push_distinct(&mut activities, item.activity);
        if let Some(modifier) = item.modifier {
            push_distinct(&mut activities, modifier);
        }
    }
    let groups = if one_heading {
        format!("`{}{}`", areas.concat(), activities.concat())
    } else if areas.is_empty() {
        render_group(&activities, CAPTION_LIMIT)
    } else {
        let area_slots = areas.len().min(AREA_LIMIT);
        let left = render_group(&areas, AREA_LIMIT);
        let right = render_group(&activities, CAPTION_LIMIT - area_slots);
        format!("{left} / {right}")
    };
    Ok(Caption {
        subject: format!("Merge #{number} [{groups}] {title}"),
        recognized,
        total,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn caption(subjects: &[&str]) -> String {
        let vocab = Vocabulary::builtin().unwrap();
        let inputs: Vec<_> = subjects.iter().map(|s| s.to_string()).collect();
        format_caption(17, "Preview", &inputs, &vocab).unwrap()
    }

    #[test]
    fn equivalent_headings_remain_one_complete_sequence() {
        assert_eq!(
            caption(&[
                "🧑‍💻🛡 #31 (tools): refuse an unsafe operation",
                "🧑‍💻🛡️ #42 (tools): guard a second operation",
            ]),
            "Merge #17 [`🧑‍💻🛡️`] Preview"
        );
        assert_eq!(
            caption(&["🧩✨🧪 old ticket"]),
            "Merge #17 [`🧩✨🧪`] Preview"
        );
    }

    #[test]
    fn mixed_headings_preserve_first_appearance_in_both_groups() {
        assert_eq!(
            caption(&["👷🔐", "🤖📝", "🔺", "👷🔌", "👷🔌", "👷🔥", "👷🐛", "📝"]),
            "Merge #17 [`👷🤖` / `🔐📝🔺🔌🔥🐛`] Preview"
        );
        assert_eq!(
            caption(&["🧩✨", "🧩✨💥"]),
            "Merge #17 [`🧩` / `✨💥`] Preview"
        );
    }

    #[test]
    fn historic_activity_and_standalone_markers_need_no_guessed_area() {
        assert_eq!(caption(&["🔖 release 1.0"]), "Merge #17 [`🔖`] Preview");
        assert_eq!(
            caption(&["📝 old style", "🔖 release"]),
            "Merge #17 [`📝🔖`] Preview"
        );
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
            "Merge #17 Preview"
        );
    }

    #[test]
    fn nine_distinct_symbols_fit_and_each_overflow_marks_its_own_side() {
        let mut rows = vec!["🧱🔧", "🎨✨", "👷📝", "🎨🛂", "👷♻", "🧱☑"];
        assert_eq!(
            caption(&rows),
            "Merge #17 [`🧱🎨👷` / `🔧✨📝🛂♻️☑️`] Preview"
        );
        rows.push("🧩☑");
        assert_eq!(
            caption(&rows),
            "Merge #17 [`🧱🎨`… / `🔧✨📝🛂♻️☑️`] Preview"
        );
        rows.push("🧱🔊");
        assert_eq!(
            caption(&rows),
            "Merge #17 [`🧱🎨`… / `🔧✨📝🛂♻️`…] Preview"
        );
        rows.remove(6);
        assert_eq!(
            caption(&rows),
            "Merge #17 [`🧱🎨👷` / `🔧✨📝🛂♻️`…] Preview"
        );
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
            "Merge #17 Add `--dry-run` to café"
        );
    }
}
