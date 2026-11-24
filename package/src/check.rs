//! Checks a whole commit message: the subject and the lines the body should carry.

use crate::config::{Config, Level, SignatureRule};
use crate::subject::{self, Subject, Ticket};
use crate::trailers::{VERSION_MARK, check_review_links, records, trailer};
use crate::vocabulary::{Vocabulary, normalize};

/// What a check found.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Report {
    /// Reasons the commit is refused.
    pub errors: Vec<String>,
    /// Things worth a second look that do not stop the commit.
    pub warnings: Vec<String>,
    /// A plain reading of an accepted subject.
    pub reading: Option<String>,
}

impl Report {
    /// True when the commit may go through.
    pub fn passed(&self) -> bool {
        self.errors.is_empty()
    }
}

fn report_at(report: &mut Report, level: Level, text: String) {
    match level {
        Level::Off => {}
        Level::Warn => report.warnings.push(text),
        Level::Error => report.errors.push(text),
    }
}

/// True for `<model> (<effort>)`, as in `example-model (high)`.
fn is_model_and_effort(text: &str) -> bool {
    let plain = |text: &str| !text.is_empty() && !text.contains(['(', ')', ':', ',']);
    text.split_once(" (")
        .and_then(|(model, rest)| rest.strip_suffix(')').map(|effort| (model, effort)))
        .is_some_and(|(model, effort)| plain(model) && plain(effort))
}

/// True for a model line such as `example-model (high)` or
/// `Example Model (high), editor`.
fn is_model_line(line: &str) -> bool {
    let plain = |text: &str| !text.is_empty() && !text.contains(['(', ')', ':', ',']);
    match line.split_once(", ") {
        Some((model, name)) => is_model_and_effort(model) && plain(name),
        None => is_model_and_effort(line),
    }
}

/// True for a line such as `Sub-agents: 2 × example-helper (medium), 1 × example-model (high)`.
fn is_subagent_line(line: &str) -> bool {
    let Some(list) = line.strip_prefix("Sub-agents: ") else {
        return false;
    };
    list.split(", ").all(|item| {
        item.split_once(" × ")
            .or_else(|| item.split_once(" x "))
            .is_some_and(|(count, model)| {
                count.parse::<u32>().is_ok_and(|n| n > 0) && is_model_and_effort(model)
            })
    })
}

/// True for a harness line such as `Claude Code` or `Claude Code (example-session)`.
fn is_harness_line(line: &str) -> bool {
    let (harness, session) = match line.split_once(" (") {
        Some((harness, rest)) => match rest.strip_suffix(')') {
            Some(session) => (harness, Some(session)),
            None => return false,
        },
        None => (line, None),
    };
    let plain = |text: &str| !text.is_empty() && !text.contains(['(', ')', ':', ',']);
    plain(harness) && session.is_none_or(plain)
}

/// True for a `Team:` or `Research:` line with something after the colon.
fn is_note_line(line: &str) -> bool {
    ["Team: ", "Research: "].iter().any(|key| {
        line.strip_prefix(key)
            .is_some_and(|rest| !rest.trim().is_empty())
    })
}

/// The version a line names when it is the line the hook writes: `△`, one space, a
/// backtick, some text, a backtick, and then the end of the line. That form is what
/// marks the line, so it is found wherever it stands; what is between the backticks is
/// taken as it is. A sentence that merely begins with the sign is not the line.
fn version_line(line: &str) -> Option<&str> {
    // The closing backtick is the last character of the line: not even a space follows.
    let version = line
        .strip_prefix(VERSION_MARK)?
        .strip_prefix(" `")?
        .strip_suffix('`')?;
    let plain = !version.is_empty() && !version.contains('`') && version.trim() == version;
    plain.then_some(version)
}

/// True for the lines an agent signs with, after the line that opens the block: the
/// model line and the harness line, then up to three lines for sub-agents, the team
/// and research.
fn is_signed(rest: &[&str]) -> bool {
    let [model, harness, extra @ ..] = rest else {
        return false;
    };
    is_model_line(model)
        && is_harness_line(harness)
        && extra.len() <= 3
        && extra
            .iter()
            .all(|line| is_subagent_line(line) || is_note_line(line))
}

/// The block that closes a body: which of its lines belong to it, and whether an agent
/// signed in it.
#[derive(Debug, Default, PartialEq, Eq)]
struct Closing {
    /// Positions among the lines of the body, the opening line first.
    lines: Vec<usize>,
    /// True when the model and harness lines follow the opening line.
    signed: bool,
}

/// Finds the block that closes a body, given with its empty lines.
///
/// As the hook stores it, the block opens with the version line, which is told by its
/// own form and not by where it stands. The lines straight under it belong to it when
/// they are an agent's signature; a person's commit has the version line alone.
///
/// As an agent writes it, before the hook has run, and as older commits have it, the
/// block is the last paragraph of the body, opened by an underscore on its own line and
/// followed by the signature.
///
/// Where both are found, the later one is the block. Everything else is the author's
/// prose and is left alone.
fn closing(body: &[&str]) -> Closing {
    let stored = body
        .iter()
        .rposition(|line| version_line(line).is_some())
        .map(|at| {
            let under: Vec<usize> = (at + 1..body.len())
                .take_while(|&below| !body[below].trim().is_empty())
                .collect();
            let rest: Vec<&str> = under.iter().map(|&below| body[below].trim()).collect();
            if is_signed(&rest) {
                Closing {
                    lines: std::iter::once(at).chain(under).collect(),
                    signed: true,
                }
            } else {
                Closing {
                    lines: vec![at],
                    signed: false,
                }
            }
        });
    let written = body
        .iter()
        .rposition(|line| !line.trim().is_empty())
        .and_then(|end| {
            let start = body[..end]
                .iter()
                .rposition(|line| line.trim().is_empty())
                .map_or(0, |gap| gap + 1);
            let rest: Vec<&str> = body[start + 1..=end]
                .iter()
                .map(|line| line.trim())
                .collect();
            (body[start].trim_end() == "_" && is_signed(&rest)).then(|| Closing {
                lines: (start..=end).collect(),
                signed: true,
            })
        });
    match (stored, written) {
        (Some(stored), Some(written)) if written.lines[0] > stored.lines[0] => written,
        (Some(stored), _) => stored,
        (None, Some(written)) => written,
        (None, None) => Closing::default(),
    }
}

/// True for a subject that names a version as a change of vocabulary does: after the
/// emoji, `gitmoji`, a space, the version between backticks, and a colon.
fn names_a_version(subject: &str) -> bool {
    let mut words = subject.split_whitespace().skip(1);
    words.next() == Some("gitmoji")
        && words.next().is_some_and(|version| {
            version
                .strip_prefix('`')
                .and_then(|rest| rest.strip_suffix("`:"))
                .is_some_and(|inner| !inner.is_empty() && !inner.contains('`'))
        })
}

/// Attribution and provenance identify the source without explaining the change.
fn names_people(key: &str) -> bool {
    [
        "signed-off-by",
        "co-authored-by",
        "reviewed-by",
        "acked-by",
        "tested-by",
        "reported-by",
        "suggested-by",
        "helped-by",
        "cc",
        "change-id",
    ]
    .contains(&key)
}

/// The lines of a message by the part they play, as positions among the lines given.
///
/// This is the one reading of a message. The check, the hook that writes the version
/// line and `gitmoji history` all use it, so they agree on which lines count.
pub(crate) struct Parts {
    /// The first line of the message.
    pub subject: Option<usize>,
    /// Lines straight under the subject, which git joins into it.
    pub joined: Vec<usize>,
    /// The lines under the first empty line, up to the trailers that end the message.
    pub body: Vec<usize>,
    /// The block of trailers that ends the message, when there is one.
    pub tail: Vec<usize>,
    /// How many comment lines were left out after the subject.
    pub comments: usize,
}

/// Reads a message as git will store it, not as the file it hands to the hook: git cuts
/// off the diff of `--verbose` below the scissors line and, when the message was edited,
/// drops comment lines. A hook cannot tell whether it was edited, so a line that starts
/// with the comment character never counts.
pub(crate) fn parts(config: &Config, lines: &[&str]) -> Parts {
    // The subject never starts with a comment, because the emojis come first. A first
    // line that opens with a ticket is still read, so the author is told to move it.
    let opens_with_ticket =
        |line: &str| config.is_ticket(line.split(' ').next().unwrap_or_default());
    let mut subject = None;
    let mut rest: Vec<usize> = Vec::new();
    let mut comments = 0;
    for (at, line) in lines
        .iter()
        .enumerate()
        .take_while(|(_, line)| !config.is_scissors(line))
    {
        if subject.is_none() {
            if !line.trim().is_empty() && (!config.is_comment(line) || opens_with_ticket(line)) {
                subject = Some(at);
            }
        } else if config.is_comment(line) {
            comments += 1;
        } else {
            rest.push(at);
        }
    }
    // Git reads everything up to the first empty line as the subject, so a line straight
    // under the first one is not a description: it becomes part of the subject.
    let boundary = rest
        .iter()
        .position(|&at| lines[at].trim().is_empty())
        .unwrap_or(rest.len());
    let body = rest.split_off(boundary);
    // The trailers that end the message are not part of its body.
    let text: Vec<&str> = body.iter().map(|&at| lines[at]).collect();
    let cut = records(&text, |_| false)
        .iter()
        .flat_map(|record| record.lines.first().copied())
        .min()
        .unwrap_or(body.len());
    let mut body = body;
    let tail = body.split_off(cut);
    Parts {
        subject,
        joined: rest,
        body,
        tail,
        comments,
    }
}

/// Where a message closes: the line that opens its closing block, when it has one, and
/// the last line of what comes before the trailers. Both as positions among `lines`.
pub(crate) fn closing_at(config: &Config, lines: &[&str]) -> (Option<usize>, Option<usize>) {
    let parts = parts(config, lines);
    let written: Vec<usize> = parts
        .body
        .iter()
        .copied()
        .filter(|&at| !lines[at].trim().is_empty())
        .collect();
    let text: Vec<&str> = parts.body.iter().map(|&at| lines[at]).collect();
    let head = closing(&text).lines.first().map(|&at| parts.body[at]);
    let last = written
        .last()
        .or(parts.joined.last())
        .copied()
        .or(parts.subject);
    (head, last)
}

/// The version of the vocabulary a stored message says it was checked against: the one
/// on the line that opens its closing block. The sign anywhere else says nothing.
pub fn checked_against(config: &Config, message: &str) -> Option<String> {
    let lines: Vec<&str> = message.lines().collect();
    let (head, _) = closing_at(config, &lines);
    version_line(lines[head?]).map(str::to_string)
}

/// True when the lines of a paragraph are exactly the block that closes a body.
pub(crate) fn is_closing(paragraph: &[&str]) -> bool {
    !paragraph.is_empty() && closing(paragraph).lines.len() == paragraph.len()
}

/// Checks a commit message as git hands it to a `commit-msg` hook.
///
/// `author` is the identity git will record as the author, when the caller knows it.
pub fn check_message(
    vocab: &Vocabulary,
    config: &Config,
    message: &str,
    author: Option<&str>,
) -> Report {
    check_with_source(vocab, config, message, author, MessageSource::Draft)
}

/// Validate a commit already stored by Git. Cleanup cannot remove lines from it.
pub fn check_stored_message(
    vocab: &Vocabulary,
    config: &Config,
    message: &str,
    author: Option<&str>,
) -> Report {
    check_with_source(vocab, config, message, author, MessageSource::Stored)
}

#[derive(PartialEq)]
enum MessageSource {
    Draft,
    PendingMerge,
    Stored,
}

/// These identify a shell launched by one of the bundled signature helpers' harnesses.
/// Authorship names and persistent preferences can also occur in a person's shell.
fn in_agent_session() -> bool {
    std::env::var_os("CLAUDECODE").as_deref() == Some(std::ffi::OsStr::new("1"))
        || [
            "CLAUDE_CODE_SESSION_ID",
            "CLAUDE_CODE_ENTRYPOINT",
            "CODEX_THREAD_ID",
            "CODEX_SESSION_ID",
            "ANTIGRAVITY_CONVERSATION_ID",
        ]
        .iter()
        .any(|key| std::env::var_os(key).is_some_and(|value| !value.is_empty()))
}

fn leading_issue_reference(line: &str) -> Option<&str> {
    let rest = line.strip_prefix('#')?;
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    (digits > 0).then(|| &line[..1 + digits])
}

fn check_with_source(
    vocab: &Vocabulary,
    config: &Config,
    message: &str,
    author: Option<&str>,
    source: MessageSource,
) -> Report {
    let lines: Vec<&str> = message.lines().collect();
    let parts = parts(config, &lines);
    let subject_line = parts.subject.map_or("", |at| lines[at]);
    let joined: Vec<&str> = parts.joined.iter().map(|&at| lines[at]).collect();
    let body: Vec<&str> = parts.body.iter().map(|&at| lines[at]).collect();
    let body = body.as_slice();
    // The trailers that end the message, after the block that closes the body.
    let tail: Vec<&str> = parts.tail.iter().map(|&at| lines[at]).collect();
    let comments = parts.comments;

    let mut report = Report::default();
    // A message git wrote by itself, such as a merge, is left as it is and asked nothing.
    if let Ok(Subject::Skipped(reason)) = subject::parse(vocab, config, subject_line) {
        report.reading = Some(format!("not checked: {reason}"));
        return report;
    }
    // A reference can disappear during cleanup even if other prose survives.
    // Inspect the draft before removing editor comments, up to the verbose diff.
    if source == MessageSource::Draft
        && let Some(at) = parts.subject
        && let Some(ticket) = lines[at + 1..]
            .iter()
            .take_while(|line| !config.is_scissors(line))
            .find_map(|line| leading_issue_reference(line))
    {
        report.errors.push(format!(
            "a line starts with `{ticket}`. Git can discard it as a comment when the message is edited. Start with a word, for example `See {ticket} for the decision.`"
        ));
    }
    if let Some(adopted) = config
        .vocabulary_version
        .as_ref()
        .filter(|adopted| **adopted != vocab.version)
    {
        report.warnings.push(format!(
            "this repository has adopted gitmoji {adopted}, and this gitmoji speaks {}. Install the matching version, or adopt the new one: set it in gitmoji.toml and record it with a commit that starts: 🔺 gitmoji `{}`:. What changed between versions: {}",
            vocab.version,
            vocab.version,
            crate::CHANGELOG
        ));
    }
    let parsed = subject::parse(vocab, config, subject_line);
    let is_marker = matches!(parsed, Ok(Subject::Marker(_)));
    let review_response = matches!(parsed, Ok(Subject::Tagged { .. }))
        && subject_line.trim().starts_with(subject::REVIEW_RESPONSE);
    let length = subject_line.trim().chars().count();
    if length > config.subject_max {
        let text = format!(
            "the first line is {length} characters; keep the whole line to {} so nothing is cut off in a list",
            config.subject_max
        );
        report_at(&mut report, config.length, text);
    }
    // A person or an agent says why. A job, a marker and a line git wrote need not.
    let mut owes_a_description = false;
    match parsed {
        Err(_) if config.allows_plain_subject(subject_line) => report_at(
            &mut report,
            config.untagged,
            "untagged subject; `gitmoji history` will list it for review".to_string(),
        ),
        Err(error) => report.errors.push(error.to_string()),
        Ok(Subject::Skipped(reason)) => report.reading = Some(format!("not checked: {reason}")),
        Ok(Subject::Marker(entry)) => {
            report.reading = Some(entry.name.clone());
            // A change of vocabulary names its version between backticks, the same way
            // the line that closes a message does, so one search finds both.
            if entry.code == ":small_red_triangle:" && !names_a_version(subject_line) {
                report_at(
                    &mut report,
                    config.unusual,
                    format!(
                        "write the version between backticks: {} gitmoji `{}`: <what changes>",
                        entry.emoji, vocab.version
                    ),
                );
            }
        }
        Ok(Subject::Tagged {
            tags,
            ticket,
            scope,
            ..
        }) => {
            let (_, second) = tags.second;
            let mut reading = String::new();
            if review_response {
                reading.push_str("Review response: ");
            }
            if let Some(area) = tags.area {
                reading.push_str(&area.name);
                reading.push_str(": ");
            }
            reading.push_str(&second.name.to_lowercase());
            if let Some(modifier) = tags.modifier {
                reading.push_str(&format!(", {}", modifier.name.to_lowercase()));
            }
            owes_a_description = matches!(ticket, Ticket::Id(_));
            match &ticket {
                Ticket::Id(id) => reading.push_str(&format!(", for {id}")),
                Ticket::Job => reading.push_str(", made by a job"),
            }
            reading.push_str(&format!(", in {scope}"));
            report.reading = Some(reading);

            if let Some(trailer) = &second.trailer {
                let prefix = format!("{trailer}:");
                let present = joined
                    .iter()
                    .chain(body)
                    .chain(&tail)
                    .any(|line| line.trim_start().starts_with(&prefix));
                if !present {
                    let text = format!(
                        "{} {} should name its {} in the body, on a line starting with `{prefix}`",
                        second.emoji,
                        second.name,
                        trailer.to_lowercase()
                    );
                    report_at(&mut report, config.trailers, text);
                }
            }
            if let Some(area) = tags.area {
                let usual = &second.areas;
                let fits = usual.is_empty()
                    || usual
                        .iter()
                        .any(|emoji| normalize(emoji) == normalize(&area.emoji));
                if !fits {
                    let text = format!(
                        "{}{} is an unusual pair: {} {} usually goes with {}. If the area is right, carry on; if not, run `gitmoji guide {}`",
                        area.emoji,
                        second.emoji,
                        second.emoji,
                        second.name,
                        usual.join(" or "),
                        second.emoji
                    );
                    report_at(&mut report, config.unusual, text);
                }
            }
            if let (true, Some(modifier)) = (second.closes, tags.modifier) {
                let text = format!(
                    "{} {} closes work that was left open, so it does not carry {} itself",
                    second.emoji, second.name, modifier.emoji
                );
                report_at(&mut report, config.unusual, text);
            }
        }
    }

    let agent_text = "an agent is never the author or a co-author of a commit. Remove it and sign the end of the body: an underscore on its own line, then the model, then the harness. Run `gitmoji guide message`";
    // The trailers as git reads them: one record per trailer, however it was folded.
    // They end the message, after the block that closes the body, where git and its
    // hosts look for them. Some authors put them before that block instead.
    let metadata_only =
        |key: &str| names_people(key) || (review_response && matches!(key, "review" | "addresses"));
    let last = records(&tail, |_| false);
    let reason_after = last.iter().any(|record| !metadata_only(&record.key));
    let mut trailers = records(body, is_closing);
    if review_response {
        // Git exposes only the final trailer paragraph. A review link just
        // before the signature, or attached to it without a blank line, would
        // disappear from git log's trailer fields even when the URL is valid.
        let review_key = |key: &str| matches!(key, "review" | "addresses");
        let in_signature = body
            .rsplit(|line| line.trim().is_empty())
            .find(|paragraph| !paragraph.is_empty() && !is_closing(paragraph))
            .is_some_and(|paragraph| {
                (paragraph[0] == "_" || version_line(paragraph[0]).is_some())
                    && paragraph
                        .iter()
                        .any(|line| trailer(line).is_some_and(|(key, _)| review_key(&key)))
            });
        if in_signature || trailers.iter().any(|record| review_key(&record.key)) {
            report.errors.push("Place Review: and Addresses: in the final Git trailer paragraph, separated from the signature or prose by a blank line.".to_owned());
        } else if let Err(problem) = check_review_links(&last) {
            report.errors.push(problem.to_owned());
        }
    }
    trailers.extend(last);
    // A co-author is looked for in those, and on any single line that reads as one, so
    // that a credit git would not parse is refused all the same.
    // Each credit is kept with its value in lower case, so that one credit found both
    // ways, or written in capitals, is reported once.
    let plain = |value: &str| {
        value
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    };
    let mut credited: Vec<(String, String)> = joined
        .iter()
        .chain(body)
        .filter(|line| config.names_an_agent(line))
        .filter_map(|line| {
            trailer(line)
                .filter(|(key, _)| key == "co-authored-by")
                .map(|(_, value)| (line.trim().to_string(), plain(value)))
        })
        .collect();
    for record in &trailers {
        let value = plain(&record.value);
        let seen = credited
            .iter()
            .any(|(_, known)| value.starts_with(known.as_str()));
        if record.key == "co-authored-by" && config.names_an_agent(&record.value) && !seen {
            credited.push((format!("Co-authored-by: {}", record.value), value));
        }
    }
    let block = closing(body);
    if credited.is_empty() {
        let prose: Vec<_> = body
            .iter()
            .enumerate()
            .filter(|(index, _)| !block.lines.contains(index))
            .map(|(_, line)| *line)
            .collect();
        if crate::attribution::has_harness_footer(&prose.join("\n")) {
            report.warnings.push("The commit ends with a tool attribution line. Remove it if the harness inserted it.".to_owned());
        }
    }
    for (line, _) in credited {
        report_at(
            &mut report,
            config.agent_author,
            format!("`{line}`: {agent_text}"),
        );
    }
    if let Some(author) = author.filter(|author| config.names_an_agent(author)) {
        report_at(
            &mut report,
            config.agent_author,
            format!("the author is set to `{author}`: {agent_text}"),
        );
    }
    // Trailers such as `Limitation: ...` and the signature are not prose, so they do not count.
    let is_trailer = |line: &str| {
        line.split_once(": ").is_some_and(|(key, _)| {
            !key.contains(' ') && key.contains(|c: char| c.is_ascii_uppercase())
        })
    };
    // A line that was never wrapped counts as the lines it would fill at 72 characters,
    // so an essay typed on one line is still an essay.
    let written: Vec<(usize, &str)> = body
        .iter()
        .enumerate()
        .map(|(index, line)| (index, line.trim()))
        .filter(|(_, line)| !line.is_empty())
        .collect();
    let signed = block.signed;
    // What the author wrote: every line that is not empty and not in the closing block.
    let content: Vec<&(usize, &str)> = written
        .iter()
        .filter(|(index, _)| !block.lines.contains(index))
        .collect();
    // Only the trailers before the closing block sit among the lines counted here.
    let before: Vec<_> = records(body, is_closing)
        .into_iter()
        .filter(|record| metadata_only(&record.key))
        .collect();
    let metadata: Vec<usize> = before
        .iter()
        .flat_map(|record| record.lines.iter().copied())
        .collect();
    let said = content
        .iter()
        .any(|(index, line)| {
            !metadata.contains(index)
                && !trailer(line).is_some_and(|(key, _)| metadata_only(&key))
                // A version line left over above the closing block is not a reason.
                && version_line(line).is_none()
        })
        // A trailer after the closing block that carries a reason, such as `Limitation:`.
        || reason_after;
    if !joined.is_empty() {
        report_at(
            &mut report,
            config.description,
            "leave an empty line under the first line. Without it git joins the next line into the subject, and the commit has no description".to_string(),
        );
    } else if owes_a_description && !said {
        report_at(
            &mut report,
            config.description,
            format!(
                "the description is empty. Leave an empty line under the first line and say why this change was made, in a line or two; the diff already shows what changed{}",
                if comments > 0 {
                    format!(
                        ". A line that starts with `{}` does not count, because git drops such lines from a message that was edited",
                        config.comment_char
                    )
                } else {
                    String::new()
                }
            ),
        );
    }
    let prose: usize = content
        .iter()
        .map(|(_, line)| line)
        .filter(|line| !is_trailer(line))
        .map(|line| line.chars().count().div_ceil(72))
        .sum();
    if prose > config.body_max_lines {
        let text = format!(
            "the body runs to {prose} lines of 72 characters; keep it to {} and say only what the ticket does not",
            config.body_max_lines
        );
        report_at(&mut report, config.length, text);
    }
    check_signature(&mut report, config, signed, source, is_marker);
    report
}

/// A generated branch integration has its own subject and body; apply the same
/// signature policy used for authored commits once Git establishes its provenance.
pub fn check_branch_message(config: &Config, message: &str, stored: bool) -> Report {
    let lines: Vec<_> = message.lines().collect();
    let split = parts(config, &lines);
    let body: Vec<_> = split.body.iter().map(|&i| lines[i]).collect();
    let mut report = Report {
        reading: Some("a branch merge caption".into()),
        ..Report::default()
    };
    let source = if stored {
        MessageSource::Stored
    } else {
        MessageSource::PendingMerge
    };
    check_signature(&mut report, config, closing(&body).signed, source, false);
    report
}

fn check_signature(
    report: &mut Report,
    config: &Config,
    signed: bool,
    source: MessageSource,
    is_marker: bool,
) {
    let signature_level = match config.signature {
        SignatureRule::Agents
            if source != MessageSource::Stored && !is_marker && in_agent_session() =>
        {
            Level::Error
        }
        SignatureRule::Off | SignatureRule::Agents => Level::Off,
        SignatureRule::Warn => Level::Warn,
        SignatureRule::Error => Level::Error,
    };
    if signature_level != Level::Off && !signed {
        let advice = if source == MessageSource::PendingMerge {
            "this merge is already pending. To sign it, append the output of `gitmoji signature` to a copy of the prepared merge message and finish with `git commit --file=<signed-message>`. Follow the terminal signing recipe in the installed Gitmoji skill: references/pull-requests.md#signing-a-local-merge-from-an-agent-terminal. For future merges, use `git merge --no-commit <source>` to pause before signing."
        } else {
            "an agent signs the end of the body: an underscore on its own line, the model with its effort, the harness with its session, and where they apply lines for Sub-agents, Team and Research. Run `gitmoji signature` in the agent session and append its output. Run `gitmoji guide message`"
        };
        report_at(report, signature_level, advice.to_owned());
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    /// Checks a first line on its own, with the standard settings.
    fn check(message: &str) -> Report {
        let vocab = Vocabulary::builtin().unwrap();
        let config = Config::standard().unwrap().for_a_subject_alone();
        check_message(&vocab, &config, message, None)
    }

    /// Checks with the settings given. The rule that a commit has a description is
    /// off unless the settings name it, so that each test is about one thing.
    fn check_with(settings: &str, message: &str, author: Option<&str>) -> Report {
        let vocab = Vocabulary::builtin().unwrap();
        let mut config = Config::from_toml(settings).unwrap();
        if !settings.contains("description") {
            config.description = Level::Off;
        }
        check_message(&vocab, &config, message, author)
    }

    #[test]
    fn untagged_policy_only_relaxes_the_missing_intention() {
        let warn = "[rules]\nuntagged = 'warn'";
        for title in ["Adjust retry budget", "Réduire les délais", "更新説明"] {
            assert!(!check(title).passed());
            let report = check_with(warn, title, None);
            assert!(report.passed());
            assert!(report.warnings.iter().any(|w| w.contains("untagged")));
            let quiet = check_with("[rules]\nuntagged = 'off'", title, None);
            assert!(quiet.passed() && quiet.warnings.is_empty());
        }
        for attempt in [
            "",
            "#61 first",
            "👷 broken subject",
            ":bug: broken",
            "1️⃣ unknown",
        ] {
            assert!(!check_with(warn, attempt, None).passed(), "{attempt}");
        }
        assert!(!check_with(warn, "Adjust budget", Some("Agent <bot@openai.com>")).passed());
    }

    #[test]
    fn check_should_refuse_a_commit_without_a_description() {
        let strict = "[rules]\ndescription = 'error'";
        let bare = "🧩👔 #1 (server): allow an appeal\n";
        let report = check_with(strict, bare, None);
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].contains("description is empty"));
        // A signature alone says who, not why.
        let signed = "🧩👔 #1 (server): allow an appeal\n\n_\nexample-model (high)\nClaude Code\n";
        assert_eq!(check_with(strict, signed, None).errors.len(), 1);
        // Git's comment lines are not a description either.
        let commented = "🧩👔 #1 (server): allow an appeal\n\n# Please enter the commit message\n";
        assert_eq!(check_with(strict, commented, None).errors.len(), 1);

        let said = "🧩👔 #1 (server): allow an appeal\n\nThe law gives applicants 30 days.\n";
        assert!(check_with(strict, said, None).errors.is_empty());
        let limited =
            "🧩🩹 #1 (server): retry the upload\n\nLimitation: the SDK drops large files\n";
        assert!(check_with(strict, limited, None).errors.is_empty());
    }

    #[test]
    fn check_should_read_the_message_as_git_will_store_it() {
        let strict = "[rules]\ndescription = 'error'";
        // A reason straight under the first line is joined into the subject by git.
        let joined = "🧩👔 #1 (server): allow an appeal\nThe law gives applicants 30 days.\n";
        let report = check_with(strict, joined, None);
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].contains("empty line under the first line"));
        // The diff that `git commit --verbose` shows is cut off before it is stored.
        let verbose = "🧩👔 #1 (server): allow an appeal\n# Please enter the commit message\n# ------------------------ >8 ------------------------\ndiff --git a/x b/x\n+a line of code\n";
        let report = check_with(strict, verbose, None);
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].contains("description is empty"));
        // Another comment character, as `core.commentChar` sets it.
        let vocab = Vocabulary::builtin().unwrap();
        let mut config = Config::standard().unwrap();
        config.comment_char = ';';
        let other = "🧩👔 #1 (server): allow an appeal\n\n; Please enter the commit message\n; On branch main\n";
        assert_eq!(check_message(&vocab, &config, other, None).errors.len(), 1);
        // Lines that only name people are not a reason, however git lets them be written.
        for people in [
            "Signed-off-by: Ada <ada@example.com>\n",
            "Signed-off-by : Ada <ada@example.com>\n",
            "Signed-off-by: Ada\n <ada@example.com>\n",
        ] {
            let message = format!("🧩👔 #1 (server): allow an appeal\n\n{people}");
            assert_eq!(
                check_with(strict, &message, None).errors.len(),
                1,
                "{people}"
            );
        }
        // A line that opens with the comment character is dropped by git when a message
        // was edited, even when it opens with a ticket, so it is never the description.
        let ticket_led = "🧩👔 #1 (server): allow an appeal\n\n#7 asked for the same window.\n";
        let report = check_with(strict, ticket_led, None);
        assert!(report.errors.iter().any(|error| error.contains("See #7")));
        // When Git already stored the line, history reads it as the author’s prose.
        let mut keeping = Config::standard().unwrap();
        keeping.keeps_comments = true;
        assert!(
            check_stored_message(&vocab, &keeping, ticket_led, None)
                .errors
                .is_empty()
        );
    }

    #[test]
    fn check_should_refuse_an_agent_credited_on_a_folded_trailer() {
        let folded =
            "🧩👔 #1 (server): x\n\nWhy.\n\nCo-authored-by: Cursor\n <cursoragent@cursor.com>\n";
        assert_eq!(check_with("", folded, None).errors.len(), 1);
        // One credit is reported once, whichever way it was found.
        let plain =
            "🧩👔 #1 (server): x\n\nWhy.\n\nCo-authored-by: Claude <noreply@anthropic.com>\n";
        assert_eq!(check_with("", plain, None).errors.len(), 1);
        let shouted =
            "🧩👔 #1 (server): x\n\nWhy.\n\nCO-AUTHORED-BY : Claude <noreply@anthropic.com>\n";
        assert_eq!(check_with("", shouted, None).errors.len(), 1);
        // A person is left alone, also at a tool vendor's own domain.
        let person =
            "🧩👔 #1 (server): x\n\nWhy.\n\nCo-authored-by: Example Person <person@cursor.com>\n";
        assert!(check_with("", person, None).passed());
    }

    #[test]
    fn a_review_response_can_cite_a_conversation_and_its_individual_findings() {
        let subject = "👌 <`🧩🐛`> #1 (feed): retain the selected item";
        let why = "The review traced the loss to sorting.";
        let review = "Review: https://code.example.com/pull/3#discussion_r4";
        let addresses = "Addresses: [https://work.ghe.com/a/b/issues/6#issuecomment-3]";
        let links = format!("{review}\n{addresses}");
        let human = "[rules]\ndescription = 'error'\nsignature = 'off'";
        let agent = "[rules]\ndescription = 'error'\nsignature = 'error'";
        let signature = "_\nexample (high)\nCodex";
        for (rules, body) in [
            (human, why.to_string()),
            (human, format!("{why}\n\n{review}")),
            (human, format!("{why}\n\n{links}")),
            (agent, format!("{why}\n\n{signature}\n\n{links}")),
        ] {
            let report = check_with(rules, &format!("{subject}\n\n{body}"), None);
            assert!(report.passed(), "{report:?}");
            assert!(report.reading.unwrap().starts_with("Review response: "));
        }
        assert!(!check_with(human, &format!("{subject}\n\n{links}"), None).passed());
        assert!(!check_with(agent, &format!("{subject}\n\n{links}\n\n{signature}"), None).passed());
        for signature in [signature, "△ `0.21.0`\nexample (high)\nCodex", "△ `0.21.0`"] {
            for body in [
                format!("{why}\n\n{links}\n\n{signature}"),
                format!("{why}\n\n{signature}\n{links}"),
                format!("{why}\n\n{signature}\n{links}\n\n△ `0.21.0`"),
                format!("{why}\n\n{review}\n\n{signature}\n\n{addresses}"),
            ] {
                let report = check_with(human, &format!("{subject}\n\n{body}"), None);
                assert!(
                    report
                        .errors
                        .iter()
                        .any(|e| e.contains("final Git trailer paragraph")),
                    "{body}: {report:?}"
                );
            }
        }
        // A quoted example in the prose is not review metadata.
        let example = format!("{subject}\n\n{why}\n\n```text\n{review}\n```\n\n{signature}");
        assert!(check_with(agent, &example, None).passed());
        for (tail, expected) in [
            (format!("{review}\n{review}"), "at most one Review:"),
            (addresses.to_owned(), "requires a primary Review:"),
            (
                format!("{review}\n{addresses}\n{addresses}"),
                "at most one Addresses:",
            ),
            (
                format!("{review}\nAddresses: #42"),
                "Addresses: needs one bracketed list",
            ),
            ("Review:".to_owned(), "Review: needs"),
            (
                "Review: https://host/review\n continuation".to_owned(),
                "Review: needs",
            ),
        ] {
            let report = check_with(human, &format!("{subject}\n\n{why}\n\n{tail}"), None);
            assert!(
                report.errors.iter().any(|e| e.contains(expected)),
                "{tail}: {report:?}"
            );
        }
        // Existing project-defined trailer semantics on ordinary subjects survive migration.
        let old = "🧩🐛 #1 (feed): retain the selected item\n\nReview: maintainer sign-off\nReview: QA sign-off\nAddresses: UX-12";
        let report = check_with(human, old, None);
        assert!(report.passed() && !report.reading.unwrap().starts_with("Review response: "));
    }

    #[test]
    fn generated_credit_above_the_signature_is_advisory() {
        let message = "🧩👔 #1 (server): retry once\n\nRetry transient failures.\n\nMade with Cursor\n\n_\nexample-model (high)\nCursor";
        let report = check(message);
        assert!(
            report
                .warnings
                .iter()
                .any(|line| line.contains("tool attribution"))
        );
        assert!(
            !check(&message.replace("Made with Cursor", "> Made with Cursor"))
                .warnings
                .iter()
                .any(|line| line.contains("tool attribution"))
        );
    }

    #[test]
    fn check_should_read_the_closing_block_in_both_forms() {
        let strict = "[rules]\ndescription = 'error'\nsignature = 'error'";
        let agent = "example-model (high)\nClaude Code (example-session)\n";
        // As the hook stores it, and as an agent writes it before the hook has run.
        for opening in ["△ `2.0.0-draft.20`", "_"] {
            let message = format!("🧩👔 #1 (server): x\n\nWhy.\n\n{opening}\n{agent}");
            assert!(check_with(strict, &message, None).passed(), "{opening}");
            // Git's trailers come after the block and do not hide it.
            let trailed = format!("{message}\nSigned-off-by: Ada <ada@example.com>\n");
            assert!(check_with(strict, &trailed, None).passed(), "{opening}");
        }
        // Only a last paragraph that opens with the exact line is the block. A sentence
        // that begins with the sign, an indented example and a version without the
        // space are the author's prose.
        for prose in [
            "△ denotes a triangle in this guide.",
            "  △ `2.0.0-draft.20`",
            "△`2.0.0-draft.20`",
            "△ 2.0.0-draft.20",
            "△ `2.0.0-draft.20` and more",
            "△ `2.0.0` `twice`",
            "△ `2.0.0-draft.20` ",
            "△ `2.0.0-draft.20`\t",
            "△ ``",
        ] {
            let message = format!("🧩👔 #1 (server): x\n\n{prose}\n");
            assert_eq!(
                closing(&message.lines().skip(2).collect::<Vec<_>>()),
                Closing::default(),
                "{prose}"
            );
            assert_eq!(
                checked_against(&Config::standard().unwrap(), &message),
                None,
                "{prose}"
            );
        }
        // Two version lines and no reason are still no description.
        let twice = "🧩👔 #1 (server): x\n\n△ `2.0.0-draft.20`\n\n△ `2.0.0-draft.20`\n";
        assert_eq!(
            check_with("[rules]\ndescription = 'error'", twice, None)
                .errors
                .len(),
            1
        );
        // What stands between the backticks is taken as it is.
        assert_eq!(
            checked_against(
                &Config::standard().unwrap(),
                "🧩👔 #1 (server): x\n\nWhy.\n\n△ `next, unreleased`\n"
            ),
            Some("next, unreleased".to_string())
        );
        // The line is told by its form, not by where it stands: a sentence under it does
        // not hide it, and is then the description.
        let under =
            "🧩👔 #1 (server): x\n\n△ `2.0.0-draft.20`\nAdded after the hook ran, by hand.\n";
        assert_eq!(
            checked_against(&Config::standard().unwrap(), under),
            Some("2.0.0-draft.20".to_string())
        );
        assert!(check_with("[rules]\ndescription = 'error'", under, None).passed());
        // A person's commit closes with the version line alone: no signature in it.
        let person = "🧩👔 #1 (server): x\n\nWhy.\n\n△ `2.0.0-draft.20`\n";
        assert!(check_with("[rules]\ndescription = 'error'", person, None).passed());
        assert_eq!(check_with(strict, person, None).errors.len(), 1);
        // The block is not a description, with or without a signature in it.
        let bare = "🧩👔 #1 (server): x\n\n△ `2.0.0-draft.20`\n";
        assert_eq!(
            check_with("[rules]\ndescription = 'error'", bare, None)
                .errors
                .len(),
            1
        );
        let signed = format!("🧩👔 #1 (server): x\n\n△ `2.0.0-draft.20`\n{agent}");
        assert_eq!(
            check_with("[rules]\ndescription = 'error'", &signed, None)
                .errors
                .len(),
            1
        );
        // A trailer that carries a reason still counts after the block.
        let limited =
            "🧩🩹 #1 (server): x\n\n△ `2.0.0-draft.20`\n\nLimitation: the SDK drops large files\n";
        assert!(check_with("[rules]\ndescription = 'error'", limited, None).passed());
        // And an agent credited after it is still refused.
        let credited = format!("{person}\nCo-authored-by: Claude <noreply@anthropic.com>\n");
        assert_eq!(check_with("", &credited, None).errors.len(), 1);
    }

    #[test]
    fn check_should_ask_for_backticks_around_the_version_of_a_revision() {
        assert!(
            check("🔺 gitmoji `2.0.0-draft.22`: say why")
                .warnings
                .is_empty()
        );
        for subject in [
            "🔺 gitmoji 2.0.0-draft.22: say why",
            "🔺 gitmoji `2.0.0-draft.22` say why",
            "🔺 adopt the convention",
        ] {
            let report = check(subject);
            assert!(report.passed(), "{subject}");
            assert_eq!(report.warnings.len(), 1, "{subject}");
        }
        // A release marker is asked nothing of the kind.
        assert!(check("🔖 release 1.6.0").warnings.is_empty());
    }

    #[test]
    fn check_should_leave_a_message_git_wrote_alone() {
        let strict = "[rules]\ndescription = 'error'\nsignature = 'error'\nlength = 'error'\nsubject_max = 20";
        let merge = "Merge branch 'feature/a-rather-long-branch-name' into main\n";
        assert!(check_with(strict, merge, None).passed());
        assert!(
            check_with(
                strict,
                "Revert \"🧩👔 #1 (server): allow an appeal\"\n\nThis reverts commit abc.\n",
                None
            )
            .passed()
        );
        // Nothing in it is asked about, also not who is named in it, and no version
        // warning is given.
        let with_agent = "Merge branch 'x'\n\nCo-authored-by: Claude <noreply@anthropic.com>\n";
        let report = check_with(
            &format!("{strict}\n[vocabulary]\nversion = '0.0.1'"),
            with_agent,
            None,
        );
        assert!(report.passed());
        assert!(report.warnings.is_empty());
    }

    #[test]
    fn check_should_ask_no_description_of_a_job_a_marker_or_git() {
        let strict = "[rules]\ndescription = 'error'";
        for subject in [
            "🎨💄 — (tokens): take the new hues from Figma",
            "🔖 release 1.6.0",
            "🔺 gitmoji `2.0.0`: adopt the convention",
            "Merge branch 'main'",
        ] {
            assert!(
                check_with(strict, subject, None).errors.is_empty(),
                "{subject}"
            );
        }
        // And the rule can be turned down or off.
        let bare = "🧩👔 #1 (server): allow an appeal";
        let warned = check_with("[rules]\ndescription = 'warn'", bare, None);
        assert!(warned.errors.is_empty());
        assert_eq!(warned.warnings.len(), 1);
        assert!(check_with("[rules]\ndescription = 'off'", bare, None).passed());
    }

    #[test]
    fn check_should_warn_when_the_repository_adopted_another_vocabulary() {
        let report = check_with(
            "[vocabulary]\nversion = '1.0.0'",
            "🧩👔 #1 (server): x",
            None,
        );
        assert_eq!(report.warnings.len(), 1);
    }

    #[test]
    fn check_should_stay_quiet_when_the_adopted_vocabulary_matches() {
        let version = Vocabulary::builtin().unwrap().version;
        let settings = format!("[vocabulary]\nversion = '{version}'");
        assert!(
            check_with(&settings, "🧩👔 #1 (server): x", None)
                .warnings
                .is_empty()
        );
    }

    #[test]
    fn check_should_warn_about_style_in_infrastructure() {
        assert_eq!(check("🧱💄 #1 (infra): x").warnings.len(), 1);
    }

    #[test]
    fn check_should_accept_style_in_experience_without_a_warning() {
        assert!(check("🎨💄 #1 (app): x").warnings.is_empty());
    }

    #[test]
    fn check_should_warn_when_a_closer_is_marked_unsettled() {
        assert_eq!(
            check("🧩🏁🧪 #1 (server): keep the queue").warnings.len(),
            1
        );
    }

    #[test]
    fn check_should_refuse_an_agent_as_co_author() {
        let message = "🧩👔 #1 (server): x\n\nCo-Authored-By: Claude <noreply@anthropic.com>\n";
        assert!(!check(message).passed());
    }

    #[test]
    fn check_should_allow_a_person_as_co_author() {
        let message = "🧩👔 #1 (server): x\n\nCo-Authored-By: Ada <ada@example.org>\n";
        assert!(check(message).passed());
    }

    #[test]
    fn check_should_refuse_an_agent_as_author() {
        let report = check_with("", "🧩👔 #1 (server): x", Some("Codex <codex@openai.com>"));
        assert!(!report.passed());
    }

    #[test]
    fn check_should_ask_for_a_signature_when_the_repository_wants_one() {
        let report = check_with("[rules]\nsignature = 'error'", "🧩👔 #1 (server): x", None);
        assert!(!report.passed());
    }

    #[test]
    fn check_should_accept_a_signature_with_a_given_name_and_a_session() {
        let message = "🧩👔 #1 (server): x\n\n_\nExample Model (high), editor\nClaude Code (example-session)\n";
        assert!(check_with("[rules]\nsignature = 'error'", message, None).passed());
    }

    #[test]
    fn check_should_accept_a_signature_without_a_name_or_a_session() {
        let message = "🧩👔 #1 (server): x\n\n_\nExample Model (high)\nClaude Code\n";
        assert!(check_with("[rules]\nsignature = 'error'", message, None).passed());
    }

    #[test]
    fn check_should_refuse_a_signature_without_the_underscore() {
        let message =
            "🧩👔 #1 (server): x\n\nExample Model (high)\nClaude Code (example-session)\n";
        assert!(!check_with("[rules]\nsignature = 'error'", message, None).passed());
    }

    #[test]
    fn check_should_refuse_a_signature_without_the_harness() {
        let message = "🧩👔 #1 (server): x\n\n_\nExample Model (high), editor\n";
        assert!(!check_with("[rules]\nsignature = 'error'", message, None).passed());
    }

    #[test]
    fn check_should_warn_about_a_first_line_that_is_too_long() {
        let subject = format!("🧩👔 #1 (server): {}", "x".repeat(125));
        assert_eq!(subject.chars().count(), 141);
        assert_eq!(check(&subject).warnings.len(), 1);
    }

    #[test]
    fn check_should_accept_a_first_line_at_the_limit() {
        let subject = format!("🧩👔 #1 (server): {}", "x".repeat(124));
        assert!(check(&subject).warnings.is_empty());
    }

    #[test]
    fn check_should_count_an_unwrapped_essay_by_its_length() {
        let essay = "word ".repeat(120);
        let report = check(&format!("🧩👔 #1 (server): x\n\n{essay}\n"));
        assert_eq!(report.warnings.len(), 1);
    }

    #[test]
    fn check_should_warn_about_an_essay_in_the_body() {
        let body = "one line of prose\n".repeat(7);
        let report = check(&format!("🧩👔 #1 (server): x\n\n{body}"));
        assert_eq!(report.warnings.len(), 1);
    }

    #[test]
    fn check_should_not_count_trailers_and_the_signature_as_prose() {
        let body = "why\n".repeat(6);
        let message = format!(
            "🛰️🩹 #1 (portal): x\n\n{body}\nLimitation: the portal sends dates as text\n\n_\nExample Model (high)\nClaude Code (example-session)\n"
        );
        assert!(check(&message).warnings.is_empty());
    }

    #[test]
    fn is_model_line_should_accept_a_model_and_its_effort() {
        assert!(is_model_line("example-model (high)"));
    }

    #[test]
    fn is_model_line_should_accept_a_given_name() {
        assert!(is_model_line("Example Model (high), editor"));
    }

    #[test]
    fn is_model_line_should_refuse_a_model_without_its_effort() {
        assert!(!is_model_line("Example Model, editor"));
    }

    #[test]
    fn is_model_line_should_accept_unknown_values() {
        assert!(is_model_line("unknown (unknown)"));
    }

    #[test]
    fn is_subagent_line_should_accept_counts_of_models() {
        assert!(is_subagent_line(
            "Sub-agents: 2 × example-helper (medium), 1 × example-model (high)"
        ));
    }

    #[test]
    fn check_should_accept_a_signature_with_team_and_research() {
        let message = "🧩👔 #1 (server): x\n\n_\nexample-model (high), editor\nClaude Code (example-session)\nSub-agents: 2 × example-helper (medium)\nTeam: reviewer, tester\nResearch: 3 searches, 12 pages\n";
        assert!(check_with("[rules]\nsignature = 'error'", message, None).passed());
    }

    #[test]
    fn check_should_refuse_prose_inside_a_signature() {
        let message = "🧩👔 #1 (server): x\n\n_\nexample-model (high)\nClaude Code\nI also thought hard about it\n";
        assert!(!check_with("[rules]\nsignature = 'error'", message, None).passed());
    }

    #[test]
    fn is_subagent_line_should_refuse_a_model_without_a_count() {
        assert!(!is_subagent_line("Sub-agents: example-helper (medium)"));
    }

    #[test]
    fn check_should_accept_a_signature_that_lists_sub_agents() {
        let message = "🧩👔 #1 (server): x\n\n_\nexample-model (high), editor\nClaude Code (example-session)\nSub-agents: 2 × example-helper (medium)\n";
        assert!(check_with("[rules]\nsignature = 'error'", message, None).passed());
    }

    #[test]
    fn is_model_line_should_refuse_a_trailer() {
        assert!(!is_model_line("Co-Authored-By: Claude (high)"));
    }

    #[test]
    fn check_should_pass_a_valid_subject() {
        assert!(check("🧩👔 #412 (server): allow an appeal within 30 days\n").passed());
    }

    #[test]
    fn check_should_read_the_subject_back() {
        assert_eq!(
            check("🧩👔 #412 (server): allow an appeal")
                .reading
                .unwrap(),
            "Capability: business rule, for #412, in server"
        );
    }

    #[test]
    fn check_should_ignore_git_comment_lines() {
        assert!(check("# Please enter the commit message\n🧩👔 #412 (server): x\n").passed());
    }

    #[test]
    fn check_should_say_so_when_the_ticket_comes_first() {
        assert!(check("#412 `🧩👔` (server): x").errors[0].contains("ticket second"));
    }

    #[test]
    fn check_should_fail_a_conventional_commit() {
        assert!(!check("feat(server): add appeals").passed());
    }

    #[test]
    fn check_should_warn_when_an_accommodation_names_no_limitation() {
        assert_eq!(
            check("🛰️🩹 #398 (portal): parse portal dates by hand")
                .warnings
                .len(),
            1
        );
    }

    #[test]
    fn check_should_accept_an_accommodation_with_its_limitation() {
        let message = "🛰️🩹 #398 (portal): parse portal dates by hand\n\nLimitation: the portal sends dates as text\n";
        assert!(check(message).warnings.is_empty());
    }

    #[test]
    fn check_should_fail_an_empty_message() {
        assert!(!check("\n\n").passed());
    }
}
