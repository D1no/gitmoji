//! PreToolUse checks for GitHub post bodies exposed by Claude Code and Codex.

use super::{Status, inspect};
use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use serde_json::Value;
use std::fs;
use std::io::{self, Read};
use std::path::Path;

#[derive(Deserialize)]
struct Event {
    hook_event_name: String,
    tool_name: String,
    tool_input: Value,
}

/// Emit the hooks' common blocking exit code with an explanation on stderr.
/// Zero quietly continues to the harness's ordinary permission evaluation.
pub(super) fn run() -> Result<u8> {
    let result = serde_json::from_reader(io::stdin().lock())
        .context("invalid PreToolUse event")
        .and_then(check_event);
    if let Err(reason) = result {
        eprintln!("Gitmoji: {reason}");
        return Ok(2);
    }
    Ok(0)
}

fn check_event(event: Event) -> Result<()> {
    ensure!(
        event.hook_event_name == "PreToolUse",
        "expected a PreToolUse event"
    );
    if event.tool_name == "Bash" {
        return shell(
            event.tool_input["command"]
                .as_str()
                .context("Bash event is missing its command")?,
        );
    }
    if let Some(action) = event.tool_name.strip_prefix("mcp__github__") {
        return match action {
            "add_issue_comment"
            | "update_issue_comment"
            | "add_comment_to_pending_review"
            | "create_pull_request" => check_field(&event.tool_input, true),
            "add_reply_to_pull_request_comment" => check_field(
                &event.tool_input,
                event.tool_input.get("reaction").is_none(),
            ),
            "issue_write" => check_field(&event.tool_input, event.tool_input["method"] == "create"),
            "update_pull_request" | "pull_request_review_write" => {
                check_field(&event.tool_input, false)
            }
            _ => Ok(()),
        };
    }
    Ok(())
}

fn check_field(input: &Value, required: bool) -> Result<()> {
    match input.get("body") {
        Some(body) => check_body(body.as_str().context("post body must be text")?),
        None if required => bail!("include a body prepared with gitmoji comment sign"),
        None => Ok(()),
    }
}

fn check_body(body: &str) -> Result<()> {
    let checked = inspect(body);
    ensure!(
        matches!(checked.status, Status::Verified),
        "post has no matching agent fingerprint; prepare the final body with gitmoji comment sign"
    );
    for warning in checked.warnings {
        eprintln!("Gitmoji: {warning}");
    }
    Ok(())
}

// Divide ordinary shell sequences only at unquoted command boundaries. Leave
// quoting of each command to shlex. Nothing from the event is executed.
fn shell(command: &str) -> Result<()> {
    let command = command.trim();
    let mut start = 0;
    let mut quote = None;
    let mut escaped = false;
    for (i, ch) in command.char_indices() {
        if escaped {
            escaped = false;
        } else if ch == '\\' && quote != Some('\'') {
            escaped = true;
        } else if quote == Some(ch) {
            quote = None;
        } else if quote.is_none() {
            match ch {
                '\'' | '"' => quote = Some(ch),
                ';' | '&' | '|' | '\n' => {
                    gh_command(&command[start..i], true)?;
                    start = i + ch.len_utf8();
                }
                _ => {}
            }
        }
    }
    gh_command(&command[start..], start != 0)
}

// The CLI accepts its repository option on either side of the subcommand.
// Skip it only before identifying the group and action: a body or title
// option later on could legitimately have a value that starts with -R.
fn next_command<'a>(args: &mut impl Iterator<Item = &'a str>) -> Option<&'a str> {
    while let Some(word) = args.next() {
        if matches!(word, "-R" | "--repo") {
            args.next();
        } else if !word.starts_with("--repo=") && !word.starts_with("-R") {
            return Some(word);
        }
    }
    None
}

fn gh_command(command: &str, compound: bool) -> Result<()> {
    let Some(words) = shlex::split(command) else {
        return Ok(());
    };
    let mut args = words.iter().map(String::as_str).peekable();
    if matches!(args.peek(), Some(&"env") | Some(&"command")) {
        args.next();
    }
    while args.peek().is_some_and(|word| {
        word.split_once('=').is_some_and(|(name, _)| {
            !name.is_empty() && name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
        })
    }) {
        args.next();
    }
    let Some(program) = args.next() else {
        return Ok(());
    };
    if !matches!(
        Path::new(program).file_name().and_then(|s| s.to_str()),
        Some("gh" | "gh.exe")
    ) {
        return Ok(());
    }
    let Some(group) = next_command(&mut args) else {
        return Ok(());
    };
    let args: Vec<_> = args.collect();
    if group == "api" {
        return gh_api(&args, compound);
    }
    if !matches!(group, "issue" | "pr") {
        return Ok(());
    }
    let mut command_args = args.iter().copied();
    let Some(verb) = next_command(&mut command_args) else {
        return Ok(());
    };
    let flags = &args[args.len() - command_args.len()..];
    let has = |names: &[&str]| {
        flags
            .iter()
            .any(|arg| names.iter().any(|name| flag_matches(arg, name)))
    };
    if flags == ["--help"] {
        return Ok(());
    }
    match verb {
        "close" | "reopen" if has(&["-c", "--comment"]) => bail!(
            "publish the signed comment separately with gh {group} comment --body-file before changing its state"
        ),
        "create" | "comment" | "review" | "edit" => {}
        _ => return Ok(()),
    }
    if verb == "edit"
        && !has(&["--body", "-b", "--body-file", "-F", "--editor", "-e"])
        && has(&[
            "--title",
            "-t",
            "--add-assignee",
            "--remove-assignee",
            "--add-label",
            "--remove-label",
            "--add-project",
            "--remove-project",
            "--milestone",
            "--remove-milestone",
            "--base",
            "-B",
            "--add-reviewer",
            "--remove-reviewer",
        ])
    {
        return Ok(());
    }
    // A bodyless approval has only its target, repository selector and approval
    // flag. A grouped shorthand with additional options still needs a body.
    if verb == "review"
        && flags.iter().any(|arg| matches!(*arg, "--approve" | "-a"))
        && flags.iter().all(|arg| {
            !arg.starts_with('-')
                || matches!(*arg, "--approve" | "-a" | "--repo")
                || arg.starts_with("--repo=")
                || arg.starts_with("-R")
        })
    {
        return Ok(());
    }
    if verb == "comment"
        && flags.contains(&"--delete-last")
        && !has(&["--body", "-b", "--body-file", "-F"])
    {
        return Ok(());
    }
    ensure!(
        !compound,
        "run the GitHub publishing command in its own tool call after signing the body"
    );
    ensure!(
        !has(&[
            "--body",
            "-b",
            "--editor",
            "-e",
            "--web",
            "-w",
            "--fill",
            "--fill-first",
            "--fill-verbose",
            "--template",
            "--recover",
            "--attach"
        ]),
        "publish the final signed body using --body-file with a literal absolute path"
    );
    let file = option(flags, &["--body-file", "-F"])?
        .context("use --body-file with the absolute path to the signed body")?;
    check_body(&read_file(file)?)
}

// Only the REST endpoints which publish issue/PR bodies, reviews, or comments.
// Body JSON allows one request to carry both a review and inline findings.
fn gh_api(args: &[&str], compound: bool) -> Result<()> {
    let Some(path) = args
        .iter()
        .find_map(|arg| arg.trim_start_matches('/').strip_prefix("repos/"))
    else {
        return Ok(());
    };
    let segments: Vec<_> = path
        .split('?')
        .next()
        .unwrap_or(path)
        .trim_end_matches('/')
        .split('/')
        .skip(2)
        .collect();
    let must_have_body = match segments.as_slice() {
        ["issues" | "pulls"] => true,
        ["issues" | "pulls", _, "comments"] | ["pulls", _, "comments", _, "replies"] => true,
        ["issues" | "pulls", "comments", _] => true,
        ["issues" | "pulls", _]
        | ["pulls", _, "reviews"]
        | ["pulls", _, "reviews", _]
        | ["pulls", _, "reviews", _, "events"] => false,
        _ => return Ok(()),
    };
    let method = option(args, &["--method", "-X"])?;
    let input = option(args, &["--input"])?;
    let fields = values(args, &["--field", "-F", "--raw-field", "-f"])?;
    if matches!(method, Some("GET" | "HEAD" | "DELETE"))
        || (method.is_none() && fields.is_empty() && input.is_none())
    {
        return Ok(());
    }
    let writes_body = fields.iter().any(|field| {
        let key = field.split('=').next().unwrap_or(field);
        key == "body"
            || key == "comments"
            || key.starts_with("comments[")
            || key.starts_with("body[")
    });
    if input.is_none() && !must_have_body && !writes_body {
        return Ok(());
    }
    ensure!(
        !compound,
        "run the GitHub publishing command in its own tool call after signing the body"
    );
    ensure!(
        !writes_body,
        "send the final JSON object with --input and an absolute path so Gitmoji can verify its bodies"
    );
    let Some(input) = input else {
        ensure!(
            !must_have_body,
            "provide the signed body in a JSON file using --input"
        );
        return Ok(());
    };
    let payload: Value =
        serde_json::from_str(&read_file(input)?).context("invalid GitHub request JSON")?;
    ensure!(payload.is_object(), "expected a GitHub request object");
    check_field(&payload, must_have_body)?;
    if let Some(comments) = payload.get("comments") {
        for comment in comments
            .as_array()
            .context("review comments must be an array")?
        {
            check_field(comment, true)?;
        }
    }
    Ok(())
}

// GitHub CLI accepts long options with = and short value options with an attached value.
fn flag_matches(arg: &str, name: &str) -> bool {
    arg == name
        || arg.strip_prefix(name).is_some_and(|tail| {
            tail.starts_with('=') || (name.len() == 2 && !tail.is_empty() && !tail.starts_with('-'))
        })
}

fn values<'a>(args: &[&'a str], names: &[&str]) -> Result<Vec<&'a str>> {
    let mut result = Vec::new();
    let mut words = args.iter().copied();
    while let Some(arg) = words.next() {
        if let Some(&name) = names.iter().find(|name| flag_matches(arg, name)) {
            let tail = &arg[name.len()..];
            result.push(if tail.is_empty() {
                words.next().context("missing GitHub CLI option value")?
            } else {
                tail.strip_prefix('=').unwrap_or(tail)
            });
        }
    }
    Ok(result)
}

fn option<'a>(args: &[&'a str], names: &[&str]) -> Result<Option<&'a str>> {
    let matches = values(args, names)?;
    ensure!(
        matches.len() <= 1,
        "use one unambiguous GitHub option for the body or method"
    );
    Ok(matches.first().copied())
}

const MAX_FILE_BYTES: usize = 1024 * 1024;

fn read_file(name: &str) -> Result<String> {
    let path = Path::new(name);
    ensure!(
        path.is_absolute()
            && !name.contains(['$', '`', '*', '?', '[', '{', '!', '<', '>', '\n', '\r']),
        "provide a literal absolute path to the prepared file"
    );
    let meta = fs::metadata(path).context("cannot read the GitHub body file")?;
    ensure!(
        meta.is_file(),
        "expected a regular file containing the final body"
    );
    let file = fs::File::open(path).context("cannot open the GitHub body file")?;
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .context("cannot read the GitHub body file")?;
    ensure!(
        bytes.len() <= MAX_FILE_BYTES,
        "GitHub body file exceeds 1 MiB"
    );
    String::from_utf8(bytes).context("cannot read the GitHub body as UTF-8 text")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_at_most_one_mib_of_post_text() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let file = temp.path().join("body.md");
        let name = file.to_str().context("expected a UTF-8 temp path")?;
        fs::write(&file, vec![b'a'; MAX_FILE_BYTES])?;
        assert_eq!(read_file(name)?.len(), MAX_FILE_BYTES);
        fs::write(&file, vec![b'a'; MAX_FILE_BYTES + 1])?;
        assert_eq!(
            read_file(name)
                .err()
                .context("oversized input was accepted")?
                .to_string(),
            "GitHub body file exceeds 1 MiB"
        );
        Ok(())
    }

    #[test]
    fn ordinary_commands_and_metadata_are_untouched() -> Result<()> {
        for command in [
            "cargo test && gh pr view 17",
            "echo 'gh issue create --body example'",
            "gh pr edit 17 --title 'Title; with punctuation'",
            "gh issue close 17",
            "gh pr review 17 --approve",
            "gh api -X GET repos/a/b/pulls/5/comments -f per_page=1",
            "gh api -XPATCH repos/a/b/issues/7 -f title=Changed",
        ] {
            shell(command)?;
        }
        Ok(())
    }

    #[test]
    fn checks_exact_file_before_gh_publication() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let file = temp.path().join("reviewer's text with spaces.md");
        let quoted = shlex::try_quote(file.to_str().context("expected a UTF-8 temp path")?)?;
        let command = format!("gh -R org/project pr comment 17 --body-file {quoted}\n");
        assert!(shell(&command).is_err());
        let signed = super::super::sign("body", Some("custom-agent"), true)?;
        fs::write(&file, &signed)?;
        shell(&command)?;
        for unsafe_form in [
            format!("gh issue comment 7 --body unsigned --body-file {quoted}"),
            format!("gh pr edit 7 --title fixed -bunsigned -F{quoted}"),
            format!("prepare-body && gh issue comment 7 -F{quoted}"),
            "gh issue create --body-file -".into(),
            "gh pr review 7 -abunsigned".into(),
            "gh issue edit 7".into(),
            "gh issue reopen 7 --comment unsigned".into(),
            "GH_HOST=example.org gh pr --repo a/b comment 7 -bunsigned".into(),
            "command gh -Ra/b issue create --body=unsigned".into(),
        ] {
            assert!(shell(&unsafe_form).is_err(), "{unsafe_form}");
        }
        fs::write(&file, signed.replace("body", "edited"))?;
        assert!(shell(&command).is_err());
        Ok(())
    }

    #[test]
    fn checks_all_inline_findings_in_a_rest_review() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let file = temp.path().join("review.json");
        let signed = super::super::sign("Review", Some("custom-agent"), true)?;
        let quoted = shlex::try_quote(file.to_str().context("expected a UTF-8 temp path")?)?;
        let command = format!("gh api --method POST repos/a/b/pulls/1/reviews --input {quoted}");
        let mut req = json!({"body": signed, "comments": [{"body":signed},{"body":"unmarked"}]});
        fs::write(&file, serde_json::to_vec(&req)?)?;
        assert!(shell(&command).is_err());
        req["comments"][1]["body"] = json!(signed);
        fs::write(&file, serde_json::to_vec(&req)?)?;
        shell(&command)?;
        assert!(shell("gh api -fbody=unsigned repos/a/b/pulls/1/comments/9/replies").is_err());
        Ok(())
    }

    #[test]
    fn mcp_supported_body_is_checked_without_requiring_authorship_for_a_reaction() -> Result<()> {
        let event = |name: &str, input: Value| Event {
            hook_event_name: "PreToolUse".into(),
            tool_name: format!("mcp__github__{name}"),
            tool_input: input,
        };
        assert!(check_event(event("issue_write", json!({"method":"create"}))).is_err());
        check_event(event(
            "issue_write",
            json!({"method":"update","title":"Title"}),
        ))?;
        check_event(event(
            "add_reply_to_pull_request_comment",
            json!({"reaction":"heart"}),
        ))?;
        assert!(
            check_event(event(
                "add_comment_to_pending_review",
                json!({"body":"unsigned"})
            ))
            .is_err()
        );
        let signed = super::super::sign("Please change this", Some("custom-agent"), true)?;
        check_event(event(
            "add_reply_to_pull_request_comment",
            json!({"body":signed}),
        ))?;
        Ok(())
    }
}
