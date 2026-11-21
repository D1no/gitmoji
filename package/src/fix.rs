//! Rewrites what an author typed into what is committed: emoji characters, never shortcodes.

use crate::check::{closing_at, parts};
use crate::config::Config;
use crate::subject;
use crate::trailers::VERSION_MARK;
use crate::vocabulary::{Token, Vocabulary};

/// A message after fixing, and what was changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fixed {
    /// The message to commit.
    pub message: String,
    /// One line per change, for the author.
    pub changes: Vec<String>,
}

/// Replaces shortcodes such as `:jigsaw:` by the emoji, wherever the vocabulary knows them.
fn replace_shortcodes(vocab: &Vocabulary, text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(':') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let known = after.find(':').and_then(|end| {
            let code = &rest[start..start + end + 2];
            vocab
                .find_by_code(code)
                .map(|(_, entry)| (entry.emoji.as_str(), code.len()))
        });
        match known {
            Some((emoji, length)) => {
                out.push_str(emoji);
                rest = &rest[start + length..];
            }
            None => {
                out.push(':');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Writes every known emoji the way the vocabulary spells it.
fn canonical(vocab: &Vocabulary, cluster: &str) -> Option<String> {
    let tokens = vocab.tokens(cluster).ok()?;
    Some(
        tokens
            .iter()
            .map(|token| match token {
                Token::Known(_, entry) => entry.emoji.as_str(),
                Token::Retired(retired) => retired.emoji.as_str(),
            })
            .collect(),
    )
}

fn fix_subject(vocab: &Vocabulary, line: &str) -> String {
    let (first, rest) = line.split_once(' ').unwrap_or((line, ""));
    // Backticks around the emojis were asked for by an earlier draft; they are dropped.
    let typed = first.replace('`', "");
    let emojis = replace_shortcodes(vocab, &typed);
    // Only a first word made of vocabulary emojis is rewritten; anything else is left
    // for the check to report.
    match canonical(vocab, &emojis) {
        Some(emojis) if !emojis.is_empty() => format!("{emojis} {rest}").trim_end().to_string(),
        _ => line.to_string(),
    }
}

/// Writes the line that says which version of the vocabulary the message is checked
/// against, ``△ `<version>` ``. `lines` are the lines of the file with their endings, up to
/// `end`, where the message stops and a diff may follow.
///
/// The line opens the block that closes the body. Where an agent has signed, its
/// underscore, or the line of an earlier check, becomes this line. Where nobody has,
/// the line is added as a block of its own after the description. Either way it stays
/// above the trailers that end the message, where git and its hosts look for them, and
/// above git's comments.
fn write_version_line(vocab: &Vocabulary, config: &Config, lines: &mut Vec<String>, end: usize) {
    let text: Vec<String> = lines[..end]
        .iter()
        .map(|line| line.trim_end_matches(['\n', '\r']).to_string())
        .collect();
    let read: Vec<&str> = text.iter().map(String::as_str).collect();
    let wanted = format!("{VERSION_MARK} `{}`", vocab.version);
    // The same reading of the message as the check and `gitmoji history` use.
    let (head, last) = closing_at(config, &read);
    let ending_of = |lines: &[String], at: usize| -> String {
        let ending = &lines[at][text[at].len()..];
        // A file may end without a line break.
        if ending.is_empty() { "\n" } else { ending }.to_string()
    };
    if let Some(at) = head {
        if text[at].trim() != wanted {
            let ending = ending_of(lines, at);
            lines[at] = format!("{wanted}{ending}");
        }
        return;
    }
    let Some(last) = last else {
        return;
    };
    let ending = ending_of(lines, last);
    if !lines[last].ends_with('\n') {
        lines[last].push_str(&ending);
    }
    lines.splice(
        last + 1..last + 1,
        [ending.clone(), format!("{wanted}{ending}")],
    );
}

/// Fixes a commit message before it is checked: writes emojis for shortcodes in the
/// subject, and adds the line that names the version it is checked against. Every
/// other byte is left as it was. A message git wrote by itself is not touched.
pub fn fix_message(vocab: &Vocabulary, config: &Config, message: &str) -> Fixed {
    let mut changes = Vec::new();
    let mut lines: Vec<String> = message.split_inclusive('\n').map(String::from).collect();
    let bare = |line: &str| line.trim_end_matches(['\n', '\r']).to_string();
    // Below the scissors line is the diff of `git commit --verbose`, not the message.
    let end = lines
        .iter()
        .position(|line| config.is_scissors(&bare(line)))
        .unwrap_or(lines.len());
    let read: Vec<&str> = lines[..end]
        .iter()
        .map(|line| line.trim_end_matches(['\n', '\r']))
        .collect();
    let subject = parts(config, &read).subject;
    if let Some(at) = subject {
        let text = bare(&lines[at]);
        let better = fix_subject(vocab, &text);
        if better != text {
            changes.push(format!("wrote emojis for what was typed: {better}"));
            let ending = lines[at][text.len()..].to_string();
            lines[at] = format!("{better}{ending}");
        }
        let generated = subject::is_generated(&better);
        if config.stamp && !generated && !config.allows_plain_subject(&better) {
            write_version_line(vocab, config, &mut lines, end);
        }
    }
    Fixed {
        message: lines.concat(),
        changes,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn fix(message: &str) -> String {
        fix_message(
            &Vocabulary::builtin().unwrap(),
            &Config::from_toml("[rules]\nstamp = false").unwrap(),
            message,
        )
        .message
    }

    /// Fixes with the standard settings, which write the version line, here `△ `<v>``.
    fn with_version_line(message: &str) -> String {
        let vocab = Vocabulary::builtin().unwrap();
        fix_message(&vocab, &Config::standard().unwrap(), message)
            .message
            .replace(&vocab.version, "<v>")
    }

    #[test]
    fn kept_comment_and_misplaced_ticket_do_not_turn_body_prose_into_a_subject() {
        let vocab = Vocabulary::builtin().unwrap();
        for (first, keep) in [("# literal title", true), ("#412 misplaced ticket", false)] {
            let mut config = Config::from_toml("[rules]\nstamp = false").unwrap();
            config.keeps_comments = keep;
            let message = format!("{first}\r\n\r\n:jigsaw::necktie: example in a body\r\n");
            assert_eq!(fix_message(&vocab, &config, &message).message, message);
        }
    }

    #[test]
    fn relaxed_plain_subject_stays_unstamped_for_later_review() {
        let vocab = Vocabulary::builtin().unwrap();
        let config = Config::from_toml("[rules]\nuntagged = 'warn'").unwrap();
        let prose = "Adjust retry budget\r\n\r\nWhy.\r\n";
        assert_eq!(fix_message(&vocab, &config, prose).message, prose);
        let tagged = "🧩👔 #1 (server): adjust retry budget\n\nWhy.\n";
        assert!(
            fix_message(&vocab, &config, tagged)
                .message
                .contains(VERSION_MARK)
        );
    }

    #[test]
    fn fix_should_close_a_message_with_the_version_line() {
        // A person's commit: a block of its own after the description.
        assert_eq!(
            with_version_line("🧩👔 #1 (a): x\n\nWhy.\n"),
            "🧩👔 #1 (a): x\n\nWhy.\n\n△ `<v>`\n"
        );
        // An agent's signature: its underscore becomes the line.
        assert_eq!(
            with_version_line("🧩👔 #1 (a): x\n\nWhy.\n\n_\nmodel (high)\nHarness\n"),
            "🧩👔 #1 (a): x\n\nWhy.\n\n△ `<v>`\nmodel (high)\nHarness\n"
        );
        // Above the trailers that end the message, so git still finds them.
        assert_eq!(
            with_version_line("🧩👔 #1 (a): x\n\nWhy.\n\nSigned-off-by: Ada <ada@example.com>\n"),
            "🧩👔 #1 (a): x\n\nWhy.\n\n△ `<v>`\n\nSigned-off-by: Ada <ada@example.com>\n"
        );
        assert_eq!(
            with_version_line(
                "🧩👔 #1 (a): x\n\nWhy.\n\n_\nmodel (high)\nHarness\n\nSigned-off-by: Ada <ada@example.com>\n"
            ),
            "🧩👔 #1 (a): x\n\nWhy.\n\n△ `<v>`\nmodel (high)\nHarness\n\nSigned-off-by: Ada <ada@example.com>\n"
        );
        // Above git's comments and the diff of a verbose commit.
        assert_eq!(
            with_version_line(
                "🧩👔 #1 (a): x\n\nWhy.\n\n# Please enter\n# ------------------------ >8 ------------------------\ndiff\n"
            ),
            "🧩👔 #1 (a): x\n\nWhy.\n\n△ `<v>`\n\n# Please enter\n# ------------------------ >8 ------------------------\ndiff\n"
        );
        // A marker has no description; the line follows its subject.
        assert_eq!(
            with_version_line("🔖 release 1.6.0\n"),
            "🔖 release 1.6.0\n\n△ `<v>`\n"
        );
        // A file without a final line break, and one with Windows line endings.
        assert_eq!(
            with_version_line("🧩👔 #1 (a): x\n\nWhy."),
            "🧩👔 #1 (a): x\n\nWhy.\n\n△ `<v>`\n"
        );
        assert_eq!(
            with_version_line("🧩👔 #1 (a): x\r\n\r\nWhy.\r\n"),
            "🧩👔 #1 (a): x\r\n\r\nWhy.\r\n\r\n△ `<v>`\r\n"
        );
    }

    #[test]
    fn fix_should_write_the_version_line_once_and_keep_it_current() {
        let version = Vocabulary::builtin().unwrap().version;
        for message in [
            "🧩👔 #1 (a): x\n\nWhy.\n",
            "🧩👔 #1 (a): x\n\nWhy.\n\n_\nmodel (high)\nHarness\n\nSigned-off-by: Ada <ada@example.com>\n",
        ] {
            let once = with_version_line(message);
            assert_eq!(
                with_version_line(&once.replace("<v>", &version)),
                once,
                "{message}"
            );
        }
        // An amended commit from an earlier version is checked again, so it says so,
        // also where the file ends without a line break.
        assert_eq!(
            with_version_line("🧩👔 #1 (a): x\n\nWhy.\n\n△ `1.0.0`\nmodel (high)\nHarness\n"),
            "🧩👔 #1 (a): x\n\nWhy.\n\n△ `<v>`\nmodel (high)\nHarness\n"
        );
        assert_eq!(
            with_version_line("🧩👔 #1 (a): x\n\nWhy.\n\n△ `1.0.0`"),
            "🧩👔 #1 (a): x\n\nWhy.\n\n△ `<v>`\n"
        );
        // A sentence that begins with the sign, and an indented example, are the
        // author's own words: they are kept, and the line is added after them.
        assert_eq!(
            with_version_line("🧩👔 #1 (a): x\n\nWhy.\n\n△ denotes a triangle in this guide.\n"),
            "🧩👔 #1 (a): x\n\nWhy.\n\n△ denotes a triangle in this guide.\n\n△ `<v>`\n"
        );
        assert_eq!(
            with_version_line("🧩👔 #1 (a): x\n\nThe stored form is:\n  △ `1.0.0`\n"),
            "🧩👔 #1 (a): x\n\nThe stored form is:\n  △ `1.0.0`\n\n△ `<v>`\n"
        );
        // A △ in the description is the author's own and is left alone.
        assert_eq!(
            with_version_line(
                "🧩👔 #1 (a): x\n\n△ `1.0.0` is what older commits say.\nSo this explains it.\n"
            ),
            "🧩👔 #1 (a): x\n\n△ `1.0.0` is what older commits say.\nSo this explains it.\n\n△ `<v>`\n"
        );
    }

    #[test]
    fn fix_should_not_add_the_version_line_to_what_git_wrote() {
        for message in [
            "Merge branch 'main'\n",
            "Revert \"🧩👔 #1 (a): x\"\n\nThis reverts commit abc.\n",
            "fixup! 🧩👔 #1 (a): x\n",
            "amend! subject to replace\n\nReplacement message\n",
            "Reapply \"Adjust retries\"\n\nThis reverts commit abc.\n",
            "Squashed commit of the following:\n\ncommit abc\n",
            "# only a comment\n",
            "",
        ] {
            assert_eq!(with_version_line(message), message, "{message}");
        }
    }

    #[test]
    fn fix_should_leave_everything_but_the_subject_as_it_was() {
        for message in [
            "🧩👔 #1 (server): x\n\nWhy.\n\nCo-authored-by: Ada Lovelace <ada@example.com>\n",
            "🧩👔 #1 (server): x\r\n\r\nWhy, with Windows line endings.\r\n",
            "🧩👔 #1 (server): x\n\nWhy.\n# ------------------------ >8 ------------------------\n+:jigsaw: in a diff\n",
        ] {
            assert_eq!(fix(message), message);
        }
        assert_eq!(
            fix("# a comment first\n\n:jigsaw::necktie: #1 (a): x\r\n\r\nWhy :bug: stays.\r\n"),
            "# a comment first\n\n🧩👔 #1 (a): x\r\n\r\nWhy :bug: stays.\r\n"
        );
    }

    #[test]
    fn fix_should_write_emojis_for_shortcodes() {
        assert_eq!(
            fix(":jigsaw::necktie: #412 (server): allow an appeal"),
            "🧩👔 #412 (server): allow an appeal"
        );
    }

    #[test]
    fn fix_should_handle_a_mix_of_emoji_and_shortcode() {
        assert_eq!(fix("🧩:shield: #1 (api): x"), "🧩🛡️ #1 (api): x");
    }

    #[test]
    fn fix_should_spell_an_emoji_the_way_the_vocabulary_does() {
        assert_eq!(fix("🧩\u{1F6E1} #1 (api): x"), "🧩🛡️ #1 (api): x");
    }

    #[test]
    fn fix_should_drop_backticks_around_the_emojis() {
        assert_eq!(fix("`🧩👔` #1 (api): x"), "🧩👔 #1 (api): x");
    }

    #[test]
    fn fix_should_write_the_emoji_of_a_marker() {
        assert_eq!(fix(":bookmark: release 1.6.0"), "🔖 release 1.6.0");
    }

    #[test]
    fn fix_should_leave_an_unknown_shortcode_for_the_check_to_report() {
        assert_eq!(fix(":tada: #1 (api): x"), ":tada: #1 (api): x");
    }

    #[test]
    fn fix_should_leave_a_line_of_words_alone() {
        assert_eq!(fix("Merge branch 'main'"), "Merge branch 'main'");
    }

    #[test]
    fn fix_should_not_touch_the_summary_or_the_body() {
        let message = "🧩👔 #1 (api): explain :shield: in the docs\n\nBody with :jigsaw:\n";
        assert_eq!(fix(message), message);
    }

    #[test]
    fn fix_should_skip_git_comment_lines() {
        assert_eq!(
            fix("# comment\n:jigsaw::necktie: #1 (api): x"),
            "# comment\n🧩👔 #1 (api): x"
        );
    }

    #[test]
    fn fix_should_report_what_it_changed() {
        let fixed = fix_message(
            &Vocabulary::builtin().unwrap(),
            &Config::standard().unwrap(),
            "`:jigsaw::necktie:` #1 (a): x",
        );
        assert_eq!(fixed.changes.len(), 1);
    }
}
