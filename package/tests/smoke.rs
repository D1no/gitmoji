//! Smoke test: four made-up product stories, written out as commit subjects.
//!
//! The story is in `tests/fixtures/lifecycle-story.txt`. It covers the whole vocabulary,
//! so a change that breaks an entry, the format or the checker shows up here.

#![allow(clippy::unwrap_used)]

use gitmoji_triangle::check::check_message;
use gitmoji_triangle::config::Config;
use gitmoji_triangle::subject::{self, Subject};
use gitmoji_triangle::vocabulary::{Vocabulary, normalize};

const STORY: &str = include_str!("fixtures/lifecycle-story.txt");

fn subjects() -> impl Iterator<Item = &'static str> {
    STORY
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
}

#[test]
fn every_subject_of_the_story_should_pass_the_check() {
    let vocab = Vocabulary::builtin().unwrap();
    let config = Config::standard().unwrap().for_a_subject_alone();
    let refused: Vec<String> = subjects()
        .filter_map(|line| {
            let report = check_message(&vocab, &config, line, None);
            (!report.passed()).then(|| format!("{line}: {}", report.errors.join("; ")))
        })
        .collect();
    assert!(refused.is_empty(), "{refused:#?}");
}

#[test]
fn the_story_should_keep_its_length() {
    assert_eq!(subjects().count(), 181);
}

#[test]
fn the_story_should_use_every_entry_of_the_vocabulary() {
    let vocab = Vocabulary::builtin().unwrap();
    let config = Config::standard().unwrap().for_a_subject_alone();
    let mut used: Vec<String> = Vec::new();
    for line in subjects() {
        match subject::parse(&vocab, &config, line).unwrap() {
            Subject::Marker(entry) => used.push(normalize(&entry.emoji)),
            Subject::Tagged { tags, .. } => {
                used.extend(tags.area.map(|entry| normalize(&entry.emoji)));
                used.push(normalize(&tags.second.1.emoji));
                used.extend(tags.modifier.map(|entry| normalize(&entry.emoji)));
            }
            Subject::Skipped(_) => {}
        }
    }
    let unused: Vec<&str> = vocab
        .entries()
        .filter(|(_, entry)| !used.contains(&normalize(&entry.emoji)))
        .map(|(_, entry)| entry.emoji.as_str())
        .collect();
    assert!(unused.is_empty(), "{unused:?}");
}

#[test]
fn the_story_should_raise_no_unusual_pair() {
    let vocab = Vocabulary::builtin().unwrap();
    let config = Config::standard().unwrap().for_a_subject_alone();
    let unusual: Vec<String> = subjects()
        .flat_map(|line| check_message(&vocab, &config, line, None).warnings)
        .filter(|warning| warning.contains("unusual pair"))
        .collect();
    assert!(unusual.is_empty(), "{unusual:#?}");
}
