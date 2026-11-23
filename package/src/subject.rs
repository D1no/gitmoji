//! Reads a commit subject and says what is wrong with it.

use crate::config::Config;
use crate::vocabulary::{Entry, Group, Slot, Token, Vocabulary};

/// What stands where the ticket goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ticket {
    /// A ticket in the repository's own format.
    Id(String),
    /// An em dash: a job made the commit, so there is no ticket.
    Job,
}

/// The emojis of a subject, each in its slot.
#[derive(Debug, Clone, Copy)]
pub struct Tags<'v> {
    /// The area, when one was given.
    pub area: Option<&'v Entry>,
    /// The behaviour or intent and its group.
    pub second: (&'v Group, &'v Entry),
    /// The modifier, when one was given.
    pub modifier: Option<&'v Entry>,
}

/// A subject that was read successfully.
#[derive(Debug, Clone)]
pub enum Subject<'v> {
    /// A line the convention does not apply to, with the reason.
    Skipped(&'static str),
    /// A marker commit such as a release.
    Marker(&'v Entry),
    /// An ordinary commit.
    Tagged {
        /// The emojis.
        tags: Tags<'v>,
        /// The ticket, or the dash of a job.
        ticket: Ticket,
        /// The scope in parentheses.
        scope: String,
        /// The text after the colon.
        summary: String,
    },
}

/// Why a subject was refused. Each message says what to do next.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SubjectError {
    /// Nothing to read.
    #[error("the subject is empty")]
    Empty,
    /// The review-response prefix was used without one enclosed authored heading.
    #[error(
        "a review response starts with 👌 followed by one emoji heading in angle brackets and backticks, for example: 👌 <`🎨🐛`> #9 (feed): repair the preview"
    )]
    ReviewHeading,
    /// The ticket was written before the emojis.
    #[error(
        "the emojis come first and the ticket second: 🧩👔 {ticket} (scope): summary, not {ticket} 🧩👔 ..."
    )]
    TicketFirst {
        /// The ticket that was found at the start.
        ticket: String,
    },
    /// The line does not open with emojis.
    #[error("start the line with the emojis, for example: {example}")]
    NoEmojis {
        /// A valid subject.
        example: String,
    },
    /// The emojis are wrapped in backticks, as an earlier draft asked.
    #[error(
        "drop the backticks: the emojis are simply the first characters of the line, for example: {example}"
    )]
    Backticks {
        /// A valid subject.
        example: String,
    },
    /// No emojis at all.
    #[error("the line starts with nothing; put the area and a behaviour or intent first")]
    NoTags,
    /// Something among the emojis is not in the vocabulary.
    #[error(
        "{text} is not in the vocabulary. The line starts with emojis only, with no space between them. Run `gitmoji guide` to see the choices"
    )]
    Unknown {
        /// The text that could not be read.
        text: String,
    },
    /// An emoji that was folded into another.
    #[error("{emoji} is no longer used; write {into}. {note}")]
    Retired {
        /// The old emoji.
        emoji: String,
        /// Its replacement.
        into: String,
        /// Why.
        note: String,
    },
    /// Only an area was given.
    #[error(
        "{emoji} {name} says where, not what. Add a behaviour or an intent after it. Run `gitmoji guide {emoji}`"
    )]
    AreaOnly {
        /// The area emoji.
        emoji: String,
        /// The area name.
        name: String,
    },
    /// Two behaviours or intents in one subject.
    #[error(
        "{first} and {second} both name what changed, and a commit carries one. If a reviewer could accept one and defer the other, make two commits. If neither works without the other, tag the commit by its point and let the other ride along"
    )]
    TwoSeconds {
        /// The first of the two.
        first: String,
        /// The second of the two.
        second: String,
    },
    /// Two areas in one subject.
    #[error(
        "{first} and {second} both name an area, and a commit carries one. If the change is one decision, name the area it is part of; if it is two, split"
    )]
    TwoAreas {
        /// The first of the two.
        first: String,
        /// The second of the two.
        second: String,
    },
    /// A modifier where the behaviour or intent belongs.
    #[error(
        "{emoji} {name} is a third emoji. First say what changed, then add it: for example 🧩⛓️{emoji}"
    )]
    ModifierAsSecond {
        /// The modifier emoji.
        emoji: String,
        /// The modifier name.
        name: String,
    },
    /// More than one modifier.
    #[error("only one third emoji is allowed, and {first} and {second} are both modifiers")]
    TwoModifiers {
        /// The first of the two.
        first: String,
        /// The second of the two.
        second: String,
    },
    /// The emojis are not in slot order.
    #[error("the order is area, then behaviour or intent, then an optional modifier")]
    Order,
    /// A marker was combined with other emojis.
    #[error(
        "{emoji} {name} stands alone: no other emoji, no ticket, no scope. For example: {emoji} release 1.6.0"
    )]
    MarkerInTags {
        /// The marker emoji.
        emoji: String,
        /// The marker name.
        name: String,
    },
    /// The area was left out where the repository requires it.
    #[error("this repository asks for an area before {emoji}. Run `gitmoji guide` to pick one")]
    AreaRequired {
        /// The emoji that was given alone.
        emoji: String,
    },
    /// Nothing after the emojis.
    #[error(
        "after the emojis comes one space and the ticket, such as {example}, or an em dash (—) when a job made the commit"
    )]
    NoTicket {
        /// What a ticket looks like here.
        example: String,
    },
    /// A hyphen or an en dash where the em dash belongs.
    #[error("use an em dash (—) for a commit without a ticket, not `{found}`")]
    WrongDash {
        /// What was written.
        found: String,
    },
    /// The ticket is not in the repository's format.
    #[error(
        "{found} is not a ticket here. Write it like {example}, or an em dash (—) when a job made the commit"
    )]
    BadTicket {
        /// What was written.
        found: String,
        /// What a ticket looks like here.
        example: String,
    },
    /// Unsettled work without a ticket.
    #[error(
        "{emoji} {name} marks work that has to be closed later, so it needs a ticket and cannot carry a dash"
    )]
    ModifierWithoutTicket {
        /// The modifier emoji.
        emoji: String,
        /// The modifier name.
        name: String,
    },
    /// No scope in parentheses.
    #[error(
        "after the ticket comes the scope in parentheses, a colon and the summary: {ticket} (scope): summary"
    )]
    NoScope {
        /// The ticket as written.
        ticket: String,
    },
    /// The scope is not one this repository knows.
    #[error("{found} is not a scope of this repository. Choose one of: {allowed}")]
    BadScope {
        /// What was written.
        found: String,
        /// The scopes the repository allows.
        allowed: String,
    },
    /// Nothing after the colon.
    #[error("the summary is missing. Say what the system does now, in the words of the domain")]
    NoSummary,
}

const SKIP_PREFIXES: [(&str, &str); 5] = [
    ("Revert \"", "a revert made by git"),
    ("Reapply \"", "a reapplied commit made by git"),
    ("fixup! ", "a fixup commit"),
    ("squash! ", "a squash commit"),
    ("amend! ", "an autosquash amend commit"),
];

fn generated_reason(line: &str) -> Option<&'static str> {
    let line = line.trim();
    if line == "Squashed commit of the following:" {
        return Some("a squash message prepared by git");
    }
    // Pending and stored merges have Git state or parents. For text alone, claim
    // only a known integration form: "Merge duplicate forms" may be authored work.
    if [
        "Merge branch '",
        "Merge branches '",
        "Merge remote-tracking branch '",
        "Merge remote-tracking branches '",
        "Merge tag '",
        "Merge tags '",
        "Merge commit '",
        "Merge commits '",
    ]
    .iter()
    .any(|prefix| {
        line.strip_prefix(prefix)
            .is_some_and(|rest| rest.contains('\''))
    }) {
        return Some("a merge commit");
    }
    if let Some(rest) = line.strip_prefix("Merge pull request #")
        && let Some((number, source)) = rest.split_once(" from ")
        && number.parse::<u64>().is_ok_and(|n| n > 0)
        && !source.trim().is_empty()
    {
        return Some("a merge commit");
    }
    for (prefix, reason) in [
        ("Merge #", "a PR merge caption"),
        ("Revert #", "a PR revert caption"),
        ("Reapply #", "a PR reapply caption"),
    ] {
        if let Some((number, title)) = line.strip_prefix(prefix).and_then(|s| s.split_once(' '))
            && number
                .as_bytes()
                .first()
                .is_some_and(|b| matches!(b, b'1'..=b'9'))
            && number.parse::<u64>().is_ok()
            && !title.trim().is_empty()
            && !title
                .chars()
                .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'))
        {
            return Some(reason);
        }
    }
    SKIP_PREFIXES
        .iter()
        .find_map(|(prefix, reason)| line.starts_with(prefix).then_some(*reason))
}

/// True when the subject matches a recognized Git or host message.
pub fn is_generated(line: &str) -> bool {
    generated_reason(line).is_some()
}

/// Prefix for work prompted by code review.
pub const REVIEW_RESPONSE: &str = "👌";
/// Shortcode accepted by the hook for the review-response prefix.
pub const REVIEW_RESPONSE_CODE: &str = ":ok_hand:";

/// Split the leading emoji cluster from the ticket and the rest of the subject.
///
/// The boolean identifies a review response. Callers validate the inner tags
/// using the same vocabulary and repository rules as for an ordinary decision.
pub fn authored_heading(line: &str) -> Result<(&str, &str, bool), SubjectError> {
    let (first, rest) = line.split_once(' ').unwrap_or((line, ""));
    if !first.starts_with(REVIEW_RESPONSE) {
        return Ok((first, rest, false));
    }
    if first != REVIEW_RESPONSE {
        return Err(SubjectError::ReviewHeading);
    }
    let (wrapped, tail) = rest.split_once(' ').unwrap_or((rest, ""));
    let cluster = wrapped
        .strip_prefix("<`")
        .and_then(|value| value.strip_suffix("`>"))
        .filter(|value| !value.is_empty() && !value.contains(['`', '<', '>']))
        .ok_or(SubjectError::ReviewHeading)?;
    Ok((cluster, tail, true))
}

/// Reads one subject line.
pub fn parse<'v>(
    vocab: &'v Vocabulary,
    config: &Config,
    line: &str,
) -> Result<Subject<'v>, SubjectError> {
    let line = line.trim();
    if line.is_empty() {
        return Err(SubjectError::Empty);
    }
    if let Some(reason) = generated_reason(line) {
        return Ok(Subject::Skipped(reason));
    }
    if let Some(marker) = leading_marker(vocab, line) {
        return Ok(Subject::Marker(marker));
    }
    let (cluster, rest, _) = authored_heading(line)?;
    if cluster.contains('`') {
        return Err(SubjectError::Backticks {
            example: vocab.format.example.clone(),
        });
    }
    if config.is_ticket(cluster) {
        return Err(SubjectError::TicketFirst {
            ticket: cluster.to_string(),
        });
    }
    if cluster.is_ascii() {
        return Err(SubjectError::NoEmojis {
            example: vocab.format.example.clone(),
        });
    }
    let tags = read_tags(vocab, config.lone_second, cluster)?;

    let (ticket_text, rest) = rest.split_once(' ').unwrap_or((rest, ""));
    let ticket = read_ticket(config, ticket_text)?;
    if let (Ticket::Job, Some(modifier)) = (&ticket, tags.modifier) {
        return Err(SubjectError::ModifierWithoutTicket {
            emoji: modifier.emoji.clone(),
            name: modifier.name.clone(),
        });
    }

    let no_scope = || SubjectError::NoScope {
        ticket: ticket_text.to_string(),
    };
    let inner = rest.strip_prefix('(').ok_or_else(no_scope)?;
    let (scope, rest) = inner.split_once(')').ok_or_else(no_scope)?;
    if scope.is_empty() || scope.contains(' ') {
        return Err(no_scope());
    }
    if !config.scopes_allowed.is_empty() && !config.scopes_allowed.iter().any(|s| s == scope) {
        return Err(SubjectError::BadScope {
            found: scope.to_string(),
            allowed: config.scopes_allowed.join(", "),
        });
    }
    let summary = rest.strip_prefix(':').ok_or_else(no_scope)?.trim();
    if summary.is_empty() {
        return Err(SubjectError::NoSummary);
    }
    Ok(Subject::Tagged {
        tags,
        ticket,
        scope: scope.to_string(),
        summary: summary.to_string(),
    })
}

fn leading_marker<'v>(vocab: &'v Vocabulary, line: &str) -> Option<&'v Entry> {
    let first = line.split(' ').next()?;
    let (group, entry) = vocab.find(first)?;
    (group.slot == Slot::Marker).then_some(entry)
}

fn read_ticket(config: &Config, text: &str) -> Result<Ticket, SubjectError> {
    match text {
        "" => Err(SubjectError::NoTicket {
            example: config.ticket_example.clone(),
        }),
        "—" => Ok(Ticket::Job),
        "-" | "–" | "--" => Err(SubjectError::WrongDash {
            found: text.to_string(),
        }),
        _ if config.is_ticket(text) => Ok(Ticket::Id(text.to_string())),
        _ => Err(SubjectError::BadTicket {
            found: text.to_string(),
            example: config.ticket_example.clone(),
        }),
    }
}

pub(crate) fn read_tags<'v>(
    vocab: &'v Vocabulary,
    lone_second: bool,
    cluster: &str,
) -> Result<Tags<'v>, SubjectError> {
    if cluster.is_empty() {
        return Err(SubjectError::NoTags);
    }
    let tokens = vocab
        .tokens(cluster)
        .map_err(|text| SubjectError::Unknown { text })?;
    let mut area: Option<&Entry> = None;
    let mut second: Option<(&Group, &Entry)> = None;
    let mut modifier: Option<&Entry> = None;
    for token in tokens {
        let (group, entry) = match token {
            Token::Known(group, entry) => (group, entry),
            Token::Retired(retired) => {
                return Err(SubjectError::Retired {
                    emoji: retired.emoji.clone(),
                    into: retired.into.clone(),
                    note: retired.note.clone(),
                });
            }
        };
        match group.slot {
            Slot::Marker => {
                return Err(SubjectError::MarkerInTags {
                    emoji: entry.emoji.clone(),
                    name: entry.name.clone(),
                });
            }
            Slot::Area => {
                if let Some(first) = area {
                    return Err(SubjectError::TwoAreas {
                        first: first.emoji.clone(),
                        second: entry.emoji.clone(),
                    });
                }
                if second.is_some() || modifier.is_some() {
                    return Err(SubjectError::Order);
                }
                area = Some(entry);
            }
            Slot::Behaviour | Slot::Intent => {
                if let Some((_, first)) = second {
                    return Err(SubjectError::TwoSeconds {
                        first: first.emoji.clone(),
                        second: entry.emoji.clone(),
                    });
                }
                if modifier.is_some() {
                    return Err(SubjectError::Order);
                }
                second = Some((group, entry));
            }
            Slot::Modifier => {
                if let Some(first) = modifier {
                    return Err(SubjectError::TwoModifiers {
                        first: first.emoji.clone(),
                        second: entry.emoji.clone(),
                    });
                }
                if second.is_none() {
                    return Err(SubjectError::ModifierAsSecond {
                        emoji: entry.emoji.clone(),
                        name: entry.name.clone(),
                    });
                }
                modifier = Some(entry);
            }
        }
    }
    let Some(second) = second else {
        // Only an area is left as a possibility: a modifier alone was refused above.
        return match area {
            Some(entry) => Err(SubjectError::AreaOnly {
                emoji: entry.emoji.clone(),
                name: entry.name.clone(),
            }),
            None => Err(SubjectError::NoTags),
        };
    };
    if area.is_none() && !lone_second {
        return Err(SubjectError::AreaRequired {
            emoji: second.1.emoji.clone(),
        });
    }
    Ok(Tags {
        area,
        second,
        modifier,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn read(line: &str) -> Result<String, SubjectError> {
        let vocab = Vocabulary::builtin().unwrap();
        let config = Config::standard().unwrap();
        parse(&vocab, &config, line).map(|subject| match subject {
            Subject::Skipped(reason) => format!("skipped: {reason}"),
            Subject::Marker(entry) => format!("marker: {}", entry.name),
            Subject::Tagged {
                tags,
                ticket,
                scope,
                ..
            } => format!(
                "{}/{}/{} {:?} {}",
                tags.area.map_or("-", |e| e.name.as_str()),
                tags.second.1.name,
                tags.modifier.map_or("-", |e| e.name.as_str()),
                ticket,
                scope
            ),
        })
    }

    #[test]
    fn review_response_exposes_one_ordinary_heading_and_uses_its_ticket_rules() {
        let reviewed = "👌 <`🎨🐛🚧`> #19 (feed): repair the preview";
        assert_eq!(
            authored_heading(reviewed).unwrap(),
            ("🎨🐛🚧", "#19 (feed): repair the preview", true)
        );
        assert_eq!(
            read(reviewed),
            read("🎨🐛🚧 #19 (feed): repair the preview")
        );
        assert!(matches!(
            read("👌 <`🎨🐛🚧`> — (feed): repair the preview"),
            Err(SubjectError::ModifierWithoutTicket { .. })
        ));
        let vocab = Vocabulary::builtin().unwrap();
        let config =
            Config::from_toml("[ticket]\npatterns = ['WORK-[0-9]+']\n[scope]\nallowed = ['feed']")
                .unwrap();
        assert!(
            parse(
                &vocab,
                &config,
                "👌 <`🎨🐛`> WORK-7 (feed): repair the preview"
            )
            .is_ok()
        );
        assert!(matches!(
            parse(&vocab, &config, reviewed),
            Err(SubjectError::BadTicket { .. })
        ));
        assert!(matches!(
            parse(
                &vocab,
                &config,
                "👌 <`🎨🐛`> WORK-7 (other): repair the preview"
            ),
            Err(SubjectError::BadScope { .. })
        ));
    }

    #[test]
    fn review_response_rejects_generated_forms_invalid_tags_and_inventories() {
        for inner in [
            "🎨",
            "🎨🐛☑️",
            "🔖",
            "🚧",
            "👌",
            "Merge",
            "Revert",
            "🎨 / 🐛",
            "🐛`",
            "",
        ] {
            let line = format!("👌 <`{inner}`> #9 (feed): fix the preview");
            assert!(read(&line).is_err(), "{line}");
        }
        for opening in [
            "👌",
            "👌🏼 <`🎨🐛`>",
            "👌<`🎨🐛`>",
            "👌 <🎨🐛>",
            "👌 [`🎨🐛`]",
            "👌  <`🎨🐛`>",
            "👌 <`🎨🐛`>suffix",
        ] {
            assert!(
                read(&format!("{opening} #9 (feed): fix the preview")).is_err(),
                "{opening}"
            );
        }
    }

    #[test]
    fn parse_should_read_a_pair_with_a_ticket() {
        assert_eq!(
            read("🧩👔 #412 (server): allow an appeal within 30 days").unwrap(),
            "Capability/Business rule/- Id(\"#412\") server"
        );
    }

    #[test]
    fn parse_should_read_a_modifier() {
        assert_eq!(
            read("🗃️🏷️💥 #444 (db): drop the legacy column").unwrap(),
            "Data/Contract/Breaking Id(\"#444\") db"
        );
    }

    #[test]
    fn parse_should_read_a_job_commit() {
        assert_eq!(
            read("🎨💄 — (tokens): take the new hues from Figma").unwrap(),
            "Experience/Style/- Job tokens"
        );
    }

    #[test]
    fn parse_should_allow_a_lone_second_emoji() {
        assert_eq!(
            read("🛡️ #7 (api): refuse an empty name").unwrap(),
            "-/Validation guard/- Id(\"#7\") api"
        );
    }

    #[test]
    fn parse_should_read_a_marker() {
        assert_eq!(read("🔖 release 1.6.0").unwrap(), "marker: Release");
    }

    #[test]
    fn parse_should_skip_a_merge_commit() {
        assert_eq!(
            read("Merge branch 'main'").unwrap(),
            "skipped: a merge commit"
        );
    }

    #[test]
    fn parse_should_recognize_generated_subjects_without_matching_ordinary_prose() {
        assert_eq!(
            read("amend! existing subject").unwrap(),
            "skipped: an autosquash amend commit"
        );
        assert_eq!(
            read("Reapply \"Adjust retries\"").unwrap(),
            "skipped: a reapplied commit made by git"
        );
        assert_eq!(
            read("Squashed commit of the following:").unwrap(),
            "skipped: a squash message prepared by git"
        );
        for line in [
            "amend existing subject",
            "Reapply the rule",
            "Squashed commit of the following: change all retry limits",
        ] {
            assert!(read(line).is_err(), "{line}");
        }
    }

    #[test]
    fn only_numbered_revert_captions_with_a_title_are_generated() {
        assert_eq!(
            read("Revert #24 Restore the previous retry limit").unwrap(),
            "skipped: a PR revert caption"
        );
        assert_eq!(
            read("Reapply #24 [`🧩🐛`] Restore the retry limit").unwrap(),
            "skipped: a PR reapply caption"
        );
        for line in [
            "Revert changes in #24",
            "Reverted #24 the new default",
            "Reapply #0 title",
            "Revert #024 title",
            "Revert #+24 title",
            "Reapply #-24 title",
            "Revert #２４ title",
            "Revert #24a title",
            "Revert #18446744073709551616 title",
            "Revert #24: title",
            "Revert #24",
            "Revert #24   ",
            "Revert #24\ttitle",
            "Reapply #24 two\nlines",
        ] {
            assert!(!is_generated(line), "{line:?}");
        }
    }

    #[test]
    fn parse_should_refuse_a_ticket_before_the_emojis() {
        assert!(matches!(
            read("#412 `🧩👔` (server): x"),
            Err(SubjectError::TicketFirst { .. })
        ));
    }

    #[test]
    fn parse_should_refuse_two_behaviours() {
        assert!(matches!(
            read("🧩👔🛡️ #412 (server): x"),
            Err(SubjectError::TwoSeconds { .. })
        ));
    }

    #[test]
    fn parse_should_refuse_a_modifier_in_second_place() {
        assert!(matches!(
            read("🧩🧪 #405 (server): try a queue"),
            Err(SubjectError::ModifierAsSecond { .. })
        ));
    }

    #[test]
    fn parse_should_refuse_an_area_alone() {
        assert!(matches!(
            read("🧩 #1 (server): x"),
            Err(SubjectError::AreaOnly { .. })
        ));
    }

    #[test]
    fn parse_should_point_a_retired_emoji_to_its_successor() {
        assert!(matches!(
            read("🧩🚑 #1 (server): x"),
            Err(SubjectError::Retired { .. })
        ));
    }

    #[test]
    fn parse_should_refuse_a_hyphen_for_the_dash() {
        assert!(matches!(
            read("🎨💄 - (tokens): x"),
            Err(SubjectError::WrongDash { .. })
        ));
    }

    #[test]
    fn parse_should_refuse_a_ticket_in_another_format() {
        assert!(matches!(
            read("🎨💄 TASK-1 (tokens): x"),
            Err(SubjectError::BadTicket { .. })
        ));
    }

    #[test]
    fn parse_should_refuse_unsettled_work_without_a_ticket() {
        assert!(matches!(
            read("🧩⛓️🧪 — (server): try a queue"),
            Err(SubjectError::ModifierWithoutTicket { .. })
        ));
    }

    #[test]
    fn parse_should_refuse_a_missing_scope() {
        assert!(matches!(
            read("🧩👔 #412 allow an appeal"),
            Err(SubjectError::NoScope { .. })
        ));
    }

    #[test]
    fn parse_should_refuse_an_empty_summary() {
        assert_eq!(read("🧩👔 #412 (server): "), Err(SubjectError::NoSummary));
    }

    #[test]
    fn parse_should_refuse_a_line_that_starts_with_words() {
        assert!(matches!(
            read("feat(server): add appeals"),
            Err(SubjectError::NoEmojis { .. })
        ));
    }

    #[test]
    fn parse_should_refuse_an_emoji_outside_the_vocabulary() {
        assert!(matches!(
            read("🧩🎉 #412 (server): x"),
            Err(SubjectError::Unknown { .. })
        ));
    }

    #[test]
    fn parse_should_ask_to_drop_backticks() {
        assert!(matches!(
            read("`🧩👔` #412 (server): x"),
            Err(SubjectError::Backticks { .. })
        ));
    }

    #[test]
    fn parse_should_refuse_a_scope_outside_the_list() {
        let vocab = Vocabulary::builtin().unwrap();
        let config = Config::from_toml("[scope]\nallowed = ['app', 'server']").unwrap();
        assert!(matches!(
            parse(&vocab, &config, "🧩👔 #412 (db): x"),
            Err(SubjectError::BadScope { .. })
        ));
    }
}
