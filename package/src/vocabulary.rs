//! The vocabulary: areas, behaviours, intents, modifiers and markers.

use serde::{Deserialize, Serialize};

const BUILTIN: &str = include_str!("../vocabulary.toml");

/// Where an entry may stand in a subject.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Slot {
    /// First emoji: which area the change touches or is about.
    Area,
    /// Second emoji as a noun: which part is different now.
    Behaviour,
    /// Second emoji as a verb: what was done.
    Intent,
    /// Optional third emoji: the work is not settled.
    Modifier,
    /// A lone emoji for commits such as releases.
    Marker,
}

impl Slot {
    /// True for the two kinds that may stand in the second place.
    pub fn is_second(self) -> bool {
        matches!(self, Slot::Behaviour | Slot::Intent)
    }

    /// Lower-case name as used in the data file and in messages.
    pub fn label(self) -> &'static str {
        match self {
            Slot::Area => "area",
            Slot::Behaviour => "behaviour",
            Slot::Intent => "intent",
            Slot::Modifier => "modifier",
            Slot::Marker => "marker",
        }
    }
}

/// One emoji and what it means.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Entry {
    /// The emoji as it is written in a subject.
    pub emoji: String,
    /// The shortcode, such as `:shield:`.
    pub code: String,
    /// Short name, such as "Validation guard".
    pub name: String,
    /// One or two sentences on what it covers.
    pub meaning: String,
    /// A pair followed by a summary.
    #[serde(default)]
    pub example: Option<String>,
    /// Worth learning first.
    #[serde(default)]
    pub core: bool,
    /// For areas: the groups an author most often needs with this area.
    #[serde(default)]
    pub typical: Vec<String>,
    /// A trailer the commit body should carry, such as `Limitation`.
    #[serde(default)]
    pub trailer: Option<String>,
    /// The areas this entry usually goes with. Empty means any.
    #[serde(default)]
    pub areas: Vec<String>,
    /// True for the intents that close work left open.
    #[serde(default)]
    pub closes: bool,
}

/// Entries that answer one question and differ along one line.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Group {
    /// The slot every entry of the group stands in.
    pub slot: Slot,
    /// Short name, such as "protection".
    pub name: String,
    /// The question every entry answers.
    pub question: String,
    /// How to choose between the entries.
    pub tiebreak: String,
    /// The entries.
    pub entries: Vec<Entry>,
}

/// One rule of the convention.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Rule {
    /// A few words.
    pub title: String,
    /// The rule in full.
    pub text: String,
}

/// An emoji that used to be valid and what replaced it.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Retired {
    /// The old emoji.
    pub emoji: String,
    /// The emoji to use now.
    pub into: String,
    /// One sentence of explanation.
    pub note: String,
}

/// Canonical review-response heading and the optional provenance field in its message.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReviewResponseFormat {
    /// The subject grammar, including literal delimiters.
    pub subject: String,
    /// A complete example subject.
    pub example: String,
    /// When to use the wrapper.
    pub meaning: String,
    /// Name of the optional Git trailer.
    pub optional_trailer: String,
    /// Expected trailer value.
    pub trailer_format: String,
    /// Whether one response can cite multiple discussions.
    pub repeatable: bool,
}

/// The shape of a subject, with examples.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Format {
    /// The grammar in words.
    pub subject: String,
    /// An ordinary commit.
    pub example: String,
    /// A commit made by a job.
    pub job_example: String,
    /// A marker commit.
    pub marker_example: String,
    /// Available in vocabularies supporting review-response commits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_response: Option<ReviewResponseFormat>,
}

fn release_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

/// The whole vocabulary.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Vocabulary {
    /// Release that supplies these rules. Older exported vocabularies retain their own version.
    #[serde(default = "release_version")]
    pub version: String,
    /// The subject format.
    pub format: Format,
    /// The rules, in the order to read them.
    pub rules: Vec<Rule>,
    /// How to write the summary and the body.
    #[serde(default)]
    pub writing: Vec<Rule>,
    /// Emojis that were folded into others.
    #[serde(default)]
    pub retired: Vec<Retired>,
    /// All groups, in sheet order.
    pub groups: Vec<Group>,
}

/// The built-in data file could not be read.
#[derive(Debug, thiserror::Error)]
#[error("the built-in vocabulary is not valid: {0}")]
pub struct VocabularyError(#[from] toml::de::Error);

/// One emoji found in a subject.
#[derive(Debug, Clone, Copy)]
pub enum Token<'v> {
    /// An entry of the vocabulary and its group.
    Known(&'v Group, &'v Entry),
    /// An emoji that was folded into another.
    Retired(&'v Retired),
}

/// Removes the variation selector, so `⚙️` and `⚙` compare equal.
pub fn normalize(text: &str) -> String {
    text.chars().filter(|c| *c != '\u{FE0F}').collect()
}

impl Vocabulary {
    /// Parses the vocabulary compiled into this crate.
    pub fn builtin() -> Result<Self, VocabularyError> {
        Ok(toml::from_str(BUILTIN)?)
    }

    /// Every group together with each of its entries.
    pub fn entries(&self) -> impl Iterator<Item = (&Group, &Entry)> {
        self.groups
            .iter()
            .flat_map(|group| group.entries.iter().map(move |entry| (group, entry)))
    }

    /// Looks an entry up by its emoji.
    pub fn find(&self, emoji: &str) -> Option<(&Group, &Entry)> {
        let wanted = normalize(emoji);
        self.entries()
            .find(|(_, entry)| normalize(&entry.emoji) == wanted)
    }

    /// Looks an entry up by its shortcode, such as `:shield:`.
    pub fn find_by_code(&self, code: &str) -> Option<(&Group, &Entry)> {
        self.entries().find(|(_, entry)| entry.code == code)
    }

    /// Looks an entry up by its name, ignoring case.
    pub fn find_by_name(&self, name: &str) -> Option<(&Group, &Entry)> {
        self.entries()
            .find(|(_, entry)| entry.name.eq_ignore_ascii_case(name))
    }

    /// Looks a group up by its name, ignoring case.
    pub fn group(&self, name: &str) -> Option<&Group> {
        self.groups
            .iter()
            .find(|group| group.name.eq_ignore_ascii_case(name))
    }

    /// The groups of one slot, in sheet order.
    pub fn groups_in(&self, slot: Slot) -> impl Iterator<Item = &Group> {
        self.groups.iter().filter(move |group| group.slot == slot)
    }

    /// Splits a run of emojis into tokens.
    ///
    /// Returns the text that could not be matched as the error.
    pub fn tokens<'v>(&'v self, cluster: &str) -> Result<Vec<Token<'v>>, String> {
        let mut known: Vec<(String, Token<'v>)> = self
            .entries()
            .map(|(group, entry)| (normalize(&entry.emoji), Token::Known(group, entry)))
            .chain(
                self.retired
                    .iter()
                    .map(|retired| (normalize(&retired.emoji), Token::Retired(retired))),
            )
            .collect();
        // Longest first, so a sequence joined by a zero-width joiner wins over its first part.
        known.sort_by_key(|(emoji, _)| std::cmp::Reverse(emoji.len()));

        let text = normalize(cluster);
        let mut rest = text.as_str();
        let mut tokens = Vec::new();
        while !rest.is_empty() {
            let Some((emoji, token)) = known.iter().find(|(emoji, _)| rest.starts_with(emoji))
            else {
                return Err(rest.to_string());
            };
            tokens.push(*token);
            rest = &rest[emoji.len()..];
        }
        Ok(tokens)
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
    fn bundled_vocabulary_uses_the_package_release() {
        assert_eq!(
            Vocabulary::builtin().unwrap().version,
            env!("CARGO_PKG_VERSION")
        );
    }

    #[test]
    fn builtin_should_hold_sixty_six_entries() {
        assert_eq!(vocab().entries().count(), 66);
    }

    #[test]
    fn builtin_should_have_no_duplicate_emoji() {
        let v = vocab();
        let mut seen: Vec<String> = v.entries().map(|(_, e)| normalize(&e.emoji)).collect();
        seen.sort();
        let before = seen.len();
        seen.dedup();
        assert_eq!(seen.len(), before);
    }

    #[test]
    fn find_should_ignore_the_variation_selector() {
        assert_eq!(vocab().find("⚙").unwrap().1.name, "Setting");
    }

    #[test]
    fn tokens_should_keep_a_joined_emoji_whole() {
        let v = vocab();
        let tokens = v.tokens("🧑‍💻📝").unwrap();
        assert_eq!(tokens.len(), 2);
    }

    #[test]
    fn tokens_should_report_what_they_cannot_read() {
        assert_eq!(vocab().tokens("🧩x").unwrap_err(), "x");
    }

    #[test]
    fn every_usual_area_of_an_entry_should_be_an_area() {
        let v = vocab();
        let wrong: Vec<&String> = v
            .entries()
            .flat_map(|(_, e)| e.areas.iter())
            .filter(|emoji| v.find(emoji).is_none_or(|(g, _)| g.slot != Slot::Area))
            .collect();
        assert!(wrong.is_empty(), "{wrong:?}");
    }

    #[test]
    fn find_by_code_should_know_the_satellite_as_github_names_it() {
        assert_eq!(
            vocab()
                .find_by_code(":artificial_satellite:")
                .unwrap()
                .1
                .name,
            "Integration"
        );
    }

    #[test]
    fn every_typical_group_of_an_area_should_exist() {
        let v = vocab();
        let missing: Vec<&String> = v
            .entries()
            .flat_map(|(_, e)| e.typical.iter())
            .filter(|name| v.group(name).is_none())
            .collect();
        assert!(missing.is_empty(), "{missing:?}");
    }
}
