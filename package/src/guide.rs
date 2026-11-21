//! Guidance in small pieces, so a reader only sees the part of the sheet they need.

use std::fmt::Write as _;

use crate::vocabulary::{Entry, Group, Slot, Vocabulary};

fn first_sentence(text: &str) -> &str {
    text.split_once(". ").map_or(text, |(first, _)| first)
}

fn emojis(group: &Group) -> String {
    group
        .entries
        .iter()
        .map(|entry| entry.emoji.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

fn entry_line(out: &mut String, entry: &Entry) {
    // Writing to a String cannot fail.
    let _ = writeln!(
        out,
        "  {} {:<20} {}",
        entry.emoji,
        entry.name,
        first_sentence(&entry.meaning)
    );
}

fn group_block(out: &mut String, group: &Group) {
    let _ = writeln!(out, "{}: {}", group.name, group.question);
    for entry in &group.entries {
        entry_line(out, entry);
    }
    let _ = writeln!(out, "  Choose: {}", group.tiebreak);
    out.push('\n');
}

fn group_pointer(out: &mut String, group: &Group) {
    let _ = writeln!(
        out,
        "  {:<14} {:<22} gitmoji guide \"{}\"",
        group.name,
        emojis(group),
        group.name
    );
}

/// The first thing to read: the format, four questions and the areas.
pub fn overview(vocab: &Vocabulary) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Gitmoji Triangle {}\n", vocab.version);
    let _ = writeln!(out, "  {}", vocab.format.subject);
    let _ = writeln!(out, "  {}", vocab.format.example);
    let _ = writeln!(
        out,
        "  {}   (a job made it: no ticket)",
        vocab.format.job_example
    );
    out.push_str(
        "\nBefore you commit, answer four questions:\n\
         \x20 1. Which area is the change part of, or about?            see the list below\n\
         \x20 2. What is different now (a noun), or what did you do (a verb)?\n\
         \x20                                                           gitmoji guide <area emoji>\n\
         \x20 3. Is the work not settled: a trial, a stopgap, breaking?  gitmoji guide modifiers\n\
         \x20 4. Which ticket is it for? A job has none and writes an em dash.\n\
         \x20 Then write it: summary, body, who wrote it.               gitmoji guide message\n\
         \nThe scope is the package that owns the decision, one per commit.\n\
         A commit carries one pair. If two seem to fit, ask whether a reviewer could accept\n\
         one part and defer the rest. If yes, split there. If neither works alone, it is one\n\
         commit, tagged by its point. Parts forced by the main change ride along: its types,\n\
         its test, a lockfile.\n\n",
    );
    for group in vocab.groups_in(Slot::Area) {
        let _ = writeln!(out, "Areas, {}: {}", group.name, group.question);
        for entry in &group.entries {
            entry_line(&mut out, entry);
        }
        // The same text the area guide prints, so the offline reference cannot fall behind.
        let _ = writeln!(out, "  Choose: {}", group.tiebreak);
        out.push('\n');
    }
    out.push_str(
        "More: gitmoji guide groups | gitmoji guide message | gitmoji guide rules | gitmoji guide <emoji> | gitmoji guide all\n",
    );
    out
}

/// What an author most often needs once the area is known.
pub fn area(vocab: &Vocabulary, group: &Group, entry: &Entry) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{} {}: {}", entry.emoji, entry.name, entry.meaning);
    let _ = writeln!(out, "Choose the area: {}\n", group.tiebreak);
    let _ = writeln!(out, "Second emoji, most often with {}:\n", entry.name);
    for name in &entry.typical {
        if let Some(typical) = vocab.group(name) {
            group_block(&mut out, typical);
        }
    }
    out.push_str(
        "Nothing here fits? Every other group, one line each: gitmoji guide groups\n\
         Third emoji, only when the work is not settled:    gitmoji guide modifiers\n",
    );
    out
}

/// One line per group of second emojis, so a reader can pick the group to open.
pub fn groups(vocab: &Vocabulary) -> String {
    let mut out = String::from("Behaviours (a noun: the system now has a different one):\n");
    for behaviour in vocab.groups_in(Slot::Behaviour) {
        group_pointer(&mut out, behaviour);
    }
    out.push_str("\nIntents (a verb: what you did), when no behaviour names the change:\n");
    for intent in vocab.groups_in(Slot::Intent) {
        group_pointer(&mut out, intent);
    }
    out
}

/// One group in full, with examples.
pub fn group(group: &Group) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{} ({}): {}\n",
        group.name,
        group.slot.label(),
        group.question
    );
    for entry in &group.entries {
        let _ = writeln!(out, "{} {}  {}", entry.emoji, entry.name, entry.code);
        let _ = writeln!(out, "  {}", entry.meaning);
        if let Some(example) = &entry.example {
            let _ = writeln!(out, "  e.g. {example}");
        }
        if let Some(trailer) = &entry.trailer {
            let _ = writeln!(out, "  The body carries a line starting with `{trailer}:`");
        }
        out.push('\n');
    }
    let _ = writeln!(out, "Choose: {}", group.tiebreak);
    out
}

/// One entry, its neighbours and how to tell them apart.
pub fn entry(group: &Group, entry: &Entry) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{} {}  {}  ({}, group \"{}\")",
        entry.emoji,
        entry.name,
        entry.code,
        group.slot.label(),
        group.name
    );
    let _ = writeln!(out, "  {}", entry.meaning);
    if let Some(example) = &entry.example {
        let _ = writeln!(out, "  e.g. {example}");
    }
    if let Some(trailer) = &entry.trailer {
        let _ = writeln!(out, "  The body carries a line starting with `{trailer}:`");
    }
    let _ = writeln!(out, "\nIts group asks: {}", group.question);
    for sibling in group.entries.iter().filter(|e| e.emoji != entry.emoji) {
        entry_line(&mut out, sibling);
    }
    let _ = writeln!(out, "Choose: {}", group.tiebreak);
    out
}

/// The rules, numbered.
pub fn rules(vocab: &Vocabulary) -> String {
    let mut out = String::new();
    for (index, rule) in vocab.rules.iter().enumerate() {
        let _ = writeln!(out, "{}. {}. {}", index + 1, rule.title, rule.text);
    }
    out
}

/// How to write the summary and the body.
pub fn writing(vocab: &Vocabulary) -> String {
    let mut out = String::new();
    for item in &vocab.writing {
        let _ = writeln!(out, "{}. {}\n", item.title, item.text);
    }
    out
}

/// Every group of the given slots, in full.
pub fn slots(vocab: &Vocabulary, wanted: &[Slot]) -> String {
    let mut out = String::new();
    for group in vocab.groups.iter().filter(|g| wanted.contains(&g.slot)) {
        group_block(&mut out, group);
    }
    out
}

/// The whole sheet as text.
pub fn all(vocab: &Vocabulary) -> String {
    let mut out = overview(vocab);
    out.push('\n');
    out.push_str(&rules(vocab));
    out.push('\n');
    out.push_str(&slots(
        vocab,
        &[Slot::Behaviour, Slot::Intent, Slot::Modifier, Slot::Marker],
    ));
    for retired in &vocab.retired {
        let _ = writeln!(
            out,
            "Retired: {} is now {}. {}",
            retired.emoji, retired.into, retired.note
        );
    }
    out
}

/// Finds the piece of guidance a word or an emoji asks for.
pub fn topic(vocab: &Vocabulary, wanted: &str) -> Option<String> {
    let wanted = wanted.trim();
    match wanted.to_lowercase().as_str() {
        "" | "start" | "overview" => return Some(overview(vocab)),
        "rules" => return Some(rules(vocab)),
        "message" | "writing" | "body" | "summary" => return Some(writing(vocab)),
        "groups" | "group" => return Some(groups(vocab)),
        "all" => return Some(all(vocab)),
        "areas" | "area" => return Some(slots(vocab, &[Slot::Area])),
        "behaviours" | "behaviour" | "behaviors" => return Some(slots(vocab, &[Slot::Behaviour])),
        "intents" | "intent" => return Some(slots(vocab, &[Slot::Intent])),
        "modifiers" | "modifier" => return Some(slots(vocab, &[Slot::Modifier])),
        "markers" | "marker" => return Some(slots(vocab, &[Slot::Marker])),
        _ => {}
    }
    if let Some(found) = vocab.group(wanted) {
        return Some(group(found));
    }
    let (found_group, found_entry) = vocab.find(wanted).or_else(|| vocab.find_by_name(wanted))?;
    if found_group.slot == Slot::Area {
        Some(area(vocab, found_group, found_entry))
    } else {
        Some(entry(found_group, found_entry))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn vocab() -> Vocabulary {
        Vocabulary::builtin().unwrap()
    }

    #[test]
    fn overview_should_say_how_to_choose_between_areas() {
        assert!(overview(&vocab()).contains("over a wire"));
    }

    #[test]
    fn the_full_reference_should_carry_every_tiebreak() {
        let v = vocab();
        let text = all(&v);
        let missing: Vec<&str> = v
            .groups
            .iter()
            .filter(|group| !text.contains(&group.tiebreak))
            .map(|group| group.name.as_str())
            .collect();
        assert!(missing.is_empty(), "{missing:?}");
    }

    #[test]
    fn the_guide_should_not_say_what_a_later_reviewer_looks_for() {
        // The guide is read by the author while committing. What a reviewer reads into a
        // history afterwards is kept out of it, so that no tag looks like an admission.
        let text = format!("{}{}", all(&vocab()), writing(&vocab())).to_lowercase();
        let found: Vec<&str> = [
            "later reader",
            "red flag",
            "wrong tool",
            "reviewer will",
            "will be flagged",
            "suspicious",
            "did not search",
            "without having checked",
        ]
        .into_iter()
        .filter(|phrase| text.contains(phrase))
        .collect();
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn overview_should_stay_short() {
        assert!(overview(&vocab()).lines().count() < 45);
    }

    #[test]
    fn area_guide_should_show_its_typical_groups() {
        assert!(topic(&vocab(), "🎨").unwrap().contains("looks:"));
    }

    #[test]
    fn area_guide_should_not_repeat_groups_other_areas_show() {
        assert!(!topic(&vocab(), "🎨").unwrap().contains("domain logic"));
    }

    #[test]
    fn area_guide_should_stay_short() {
        assert!(topic(&vocab(), "🧩").unwrap().lines().count() < 30);
    }

    #[test]
    fn groups_should_list_every_group_of_second_emojis() {
        let v = vocab();
        let wanted = v.groups_in(Slot::Behaviour).count() + v.groups_in(Slot::Intent).count();
        assert_eq!(groups(&v).matches("gitmoji guide").count(), wanted);
    }

    #[test]
    fn topic_should_find_a_group_by_name() {
        assert!(
            topic(&vocab(), "protection")
                .unwrap()
                .contains("Validation guard")
        );
    }

    #[test]
    fn topic_should_find_an_entry_by_emoji() {
        assert!(topic(&vocab(), "🩹").unwrap().contains("Limitation:"));
    }

    #[test]
    fn topic_should_find_an_area_by_name() {
        assert!(topic(&vocab(), "data").unwrap().contains("from data:"));
    }

    #[test]
    fn topic_should_return_nothing_for_an_unknown_word() {
        assert!(topic(&vocab(), "nonsense").is_none());
    }
}
