//! Check the GitHub issue that tracks deferred work on a provisional pull request.

use std::collections::{BTreeMap, BTreeSet};
use std::process::ExitCode;

use anyhow::{Context, Result, bail, ensure};
use clap::Args;
use gitmoji_triangle::{
    caption::{self, Caption},
    vocabulary::Vocabulary,
};
use serde_json::{Value, json};

use super::title::{Pull, fetch_incoming, oid};
use super::{Client, valid_pr};
use crate::history::{IncomingCommits, incoming_commits};

const START: &str = "<!-- gitmoji:provisional:start -->";
const END: &str = "<!-- gitmoji:provisional:end -->";
const DESCRIPTION: &str = "### Provisional work\n\nFollow-up issue: #";
const MARKERS: [&str; 2] = ["🚧", "🧪"];
const STATUS: &str = "gitmoji/provisional";

#[derive(Args)]
pub(crate) struct ProvisionalArgs {
    /// Repository that owns the pull request and follow-up issue, OWNER/REPO.
    #[arg(long)]
    repo: String,
    /// Number of the pull request in the base repository.
    #[arg(long)]
    pr: u64,
    /// GitHub host serving the repository.
    #[arg(long, default_value = "github.com")]
    hostname: String,
    /// Required label on the open follow-up issue.
    #[arg(long, default_value = "provisional")]
    label: String,
    /// Validate and report the current policy without writing to GitHub.
    #[arg(long, conflicts_with_all = ["backlink", "publish_status"])]
    check: bool,
    /// Print a machine-readable result. A failed check exits with status 1.
    #[arg(long)]
    json: bool,
    /// Leave a backlink comment on the follow-up issue once.
    #[arg(long)]
    backlink: bool,
    /// Report the policy result as a commit status on the verified PR head.
    #[arg(long)]
    publish_status: bool,
    /// Require this exact PR head, for jobs that consume an event snapshot.
    #[arg(long)]
    expect_head: Option<String>,
}

fn issue_number(ticket: &str) -> Result<u64> {
    let digits = ticket.strip_prefix('#').context(
        "each provisional or exploration commit needs a repository-local GitHub issue such as #42",
    )?;
    ensure!(
        !digits.is_empty()
            && !digits.starts_with('0')
            && digits.bytes().all(|b| b.is_ascii_digit()),
        "expected a repository-local GitHub issue such as #42"
    );
    digits
        .parse()
        .context("GitHub issue number is out of range")
}

fn target(modifiers: &[String], tickets: &BTreeMap<String, Vec<String>>) -> Result<Option<u64>> {
    let mut numbers = BTreeSet::new();
    for marker in MARKERS {
        if modifiers.iter().any(|m| m == marker) {
            let values = tickets
                .get(marker)
                .filter(|v| !v.is_empty())
                .context("every provisional or exploration commit needs a follow-up issue")?;
            for ticket in values {
                numbers.insert(issue_number(ticket)?);
            }
        }
    }
    ensure!(
        numbers.len() <= 1,
        "use one follow-up issue for all provisional and exploration commits in this PR"
    );
    Ok(numbers.into_iter().next())
}

fn validate_issue(issue: &Value, number: u64, label: &str) -> Result<()> {
    ensure!(
        issue["number"].as_u64() == Some(number) && issue.get("pull_request").is_none(),
        "#{number} must be an issue in the base repository"
    );
    ensure!(
        issue["state"] == "open",
        "follow-up issue #{number} must be open"
    );
    let labelled = issue["labels"].as_array().is_some_and(|labels| {
        labels.iter().any(|item| {
            item["name"]
                .as_str()
                .is_some_and(|name| name.eq_ignore_ascii_case(label))
        })
    });
    ensure!(
        labelled,
        "follow-up issue #{number} needs the label {label:?}"
    );
    Ok(())
}

pub(super) fn check_issue(client: &Client, number: u64, label: &str) -> Result<()> {
    let issue = client
        .request("GET", &format!("{}/issues/{number}", client.rest), None)
        .with_context(|| {
            format!("could not read follow-up issue #{number} in the base repository")
        })?;
    validate_issue(&issue, number, label)
}

fn resolve(client: &Client, caption: &Caption, label: &str) -> Result<Option<u64>> {
    ensure!(
        !label.trim().is_empty() && !label.chars().any(char::is_control),
        "the follow-up issue label must be a nonempty, single-line name"
    );
    let number = target(&caption.modifiers, &caption.modifier_tickets)?;
    if let Some(number) = number {
        check_issue(client, number, label)?;
    }
    Ok(number)
}

/// Validate a follow-up using the already-fetched, caller-verified revision range.
pub(super) fn follow_up(
    client: &Client,
    pr: u64,
    base: &str,
    head: &str,
    label: &str,
) -> Result<Option<u64>> {
    let vocab = Vocabulary::builtin()?;
    let incoming = incoming_commits(base, &[head], &vocab)?;
    let caption = evidence(pr, &incoming, &vocab)?;
    resolve(client, &caption, label)
}

fn evidence(pr: u64, incoming: &IncomingCommits, vocab: &Vocabulary) -> Result<Caption> {
    // The aggregate caption de-duplicates tickets. Check each marked subject
    // as well, so a valid ticket cannot mask a second commit lacking one.
    for subject in &incoming.subjects {
        if caption::heading(subject, vocab)
            .and_then(|heading| heading.modifier)
            .is_some_and(|marker| MARKERS.contains(&marker))
        {
            issue_number(subject.split_whitespace().nth(1).unwrap_or_default())?;
        }
    }
    incoming.summarize_pr(pr, "Follow-up check", vocab)
}

fn block(ticket: u64) -> String {
    format!("{START}\n{DESCRIPTION}{ticket}\n{END}")
}

fn reconcile(body: &str, ticket: Option<u64>) -> Result<String> {
    let Some(start) = body.find(START) else {
        ensure!(
            !body.contains(END),
            "PR description has an incomplete Gitmoji provisional block"
        );
        return Ok(match ticket {
            Some(n) if body.is_empty() => block(n),
            Some(n) => format!("{body}\n\n{}", block(n)),
            None => body.to_owned(),
        });
    };
    let before = &body[..start];
    let (inside, after) = body[start + START.len()..]
        .split_once(END)
        .context("PR description has an incomplete Gitmoji provisional block")?;
    ensure!(
        !before.contains(END) && !after.contains(START) && !after.contains(END),
        "PR description has conflicting Gitmoji provisional blocks"
    );
    let inside = inside.replace("\r\n", "\n");
    let owned = inside
        .strip_prefix('\n')
        .and_then(|s| s.strip_prefix(DESCRIPTION))
        .and_then(|s| s.strip_suffix('\n'));
    ensure!(
        owned.is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
            && (before.is_empty() || before.ends_with('\n'))
            && (after.is_empty() || after.starts_with('\n') || after.starts_with("\r\n")),
        "move author text outside the reserved Gitmoji provisional block before rerunning"
    );
    Ok(match ticket {
        Some(n) if owned == Some(n.to_string().as_str()) => body.to_owned(),
        Some(n) => format!("{before}{}{after}", block(n)),
        None if after.is_empty() => before
            .strip_suffix("\r\n\r\n")
            .or_else(|| before.strip_suffix("\n\n"))
            .unwrap_or(before)
            .to_owned(),
        None => format!("{before}{after}"),
    })
}

#[derive(PartialEq, Eq)]
struct Snapshot {
    refs: Pull,
    body: String,
}

fn snapshot(client: &Client, number: u64) -> Result<Snapshot> {
    let record = client.get_pull(number)?;
    let refs = client
        .pull_refs(number, &record)?
        .context("the pull request is closed")?;
    let body = match record.get("body") {
        Some(Value::Null) => "",
        Some(Value::String(body)) => body,
        _ => bail!("GitHub returned an invalid pull request description"),
    };
    Ok(Snapshot {
        refs,
        body: body.to_owned(),
    })
}

fn backlink(
    client: &Client,
    pr: u64,
    issue: u64,
    before_post: impl FnOnce() -> Result<()>,
) -> Result<()> {
    let url = format!("{}/issues/{issue}/comments", client.rest);
    let body = format!(
        "Provisional work tracked here was linked from #{pr}.\n\n<!-- gitmoji:provisional-pr:{pr} -->"
    );
    for page in 1u32.. {
        let comments = client.request("GET", &format!("{url}?per_page=100&page={page}"), None)?;
        let comments = comments
            .as_array()
            .context("invalid issue comment response")?;
        if comments.iter().any(|entry| entry["body"] == body) {
            return Ok(());
        }
        if comments.len() < 100 {
            before_post()?;
            client.request("POST", &url, Some(&json!({"body": body})))?;
            return Ok(());
        }
    }
    bail!("the issue comment history is too large to verify a backlink")
}

fn status(client: &Client, pr: u64, head: &str, state: &str, description: &str) -> Result<()> {
    let target_url = format!("{}/{}/pull/{pr}", client.server, client.repo);
    client.request("POST", &format!("{}/statuses/{head}", client.rest), Some(&json!({
        "state":state, "context":STATUS, "description":format!("PR #{pr}: {description}"), "target_url":target_url
    })))?;
    Ok(())
}

fn evaluate(client: &Client, args: &ProvisionalArgs, remote: &str, head: &str) -> Result<Value> {
    let initial = snapshot(client, args.pr)?;
    ensure!(
        initial.refs.head == head,
        "the PR head has changed; rerun on its current commit"
    );
    let vocab = Vocabulary::builtin()?;
    let incoming = fetch_incoming(remote, &client.token, args.pr, &initial.refs, &vocab)?;
    let caption = evidence(args.pr, &incoming, &vocab)?;
    let issue = resolve(client, &caption, &args.label)?;
    let edited = reconcile(&initial.body, issue)?;
    let current = |body: &str| -> Result<()> {
        let fresh = snapshot(client, args.pr)?;
        ensure!(
            fresh.refs.same_refs(&initial.refs) && fresh.body == body,
            "the PR changed during validation; rerun on the current description and commits"
        );
        Ok(())
    };
    current(&initial.body)?;
    if !args.check && edited != initial.body {
        let response = client.request(
            "PATCH",
            &format!("{}/pulls/{}", client.rest, args.pr),
            Some(&json!({"body":edited})),
        )?;
        ensure!(
            response["body"] == edited,
            "GitHub returned a different PR description; rerun the policy check"
        );
    }
    if let Some(issue) = issue
        && args.backlink
    {
        backlink(client, args.pr, issue, || {
            current(&edited)?;
            resolve(client, &caption, &args.label)?;
            Ok(())
        })?;
    }
    if !args.check && (args.publish_status || edited != initial.body || args.backlink) {
        current(&edited)?;
        resolve(client, &caption, &args.label)?;
    }
    Ok(
        json!({"policy": if issue.is_some() {"provisional"} else {"ordinary"}, "modifiers":caption.modifiers,
        "follow_up_issue":issue, "base":initial.refs.base, "head":head, "pr":args.pr}),
    )
}

fn run(args: &ProvisionalArgs) -> Result<Value> {
    valid_pr(args.pr)?;
    let client = Client::new(&args.hostname, &args.repo)?;
    let remote = format!("{}/{}.git", client.server, client.repo);
    let head = match &args.expect_head {
        Some(head) => head.clone(),
        None => {
            let pr = client.get_pull(args.pr)?;
            ensure!(
                pr["number"].as_u64() == Some(args.pr) && pr["state"] == "open",
                "expected the open PR in the base repository"
            );
            pr["head"]["sha"]
                .as_str()
                .context("GitHub returned no PR head")?
                .to_owned()
        }
    };
    ensure!(oid(&head), "expected a full hexadecimal PR head commit ID");
    if args.publish_status {
        // For pull_request_target the trusted event supplies the head. Mark it
        // pending before reading further, including when an API read could fail.
        status(
            &client,
            args.pr,
            &head,
            "pending",
            "checking the provisional follow-up issue",
        )?;
    }
    let result = evaluate(&client, args, &remote, &head);
    if args.publish_status {
        match &result {
            Ok(_) => status(
                &client,
                args.pr,
                &head,
                "success",
                "provisional follow-up policy satisfied",
            )?,
            Err(error) => status(
                &client,
                args.pr,
                &head,
                "failure",
                "provisional follow-up policy failed",
            )
            .with_context(|| {
                format!("policy failed ({error:#}) and the failure status could not be published")
            })?,
        }
    }
    result
}

pub(crate) fn provisional_pr(args: ProvisionalArgs) -> Result<ExitCode> {
    match run(&args) {
        Ok(report) => {
            if args.json {
                println!("{}", serde_json::to_string(&report)?);
            } else if let Some(issue) = report["follow_up_issue"].as_u64() {
                println!("Provisional work follows issue #{issue}.");
            } else {
                println!("Ordinary pull request.");
            }
            Ok(ExitCode::SUCCESS)
        }
        Err(error) if args.json => {
            println!(
                "{}",
                json!({"policy":"error","pr":args.pr,"error":format!("{error:#}")})
            );
            Ok(ExitCode::FAILURE)
        }
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use gitmoji_triangle::caption::summarize;

    #[test]
    fn mixed_prs_share_one_repository_local_ticket() {
        let vocab = Vocabulary::builtin().unwrap();
        let caption = |subjects: &[&str]| {
            summarize(
                42,
                "Preview",
                &subjects.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>(),
                &vocab,
            )
            .unwrap()
        };
        let policy = |subjects: &[&str]| {
            let c = caption(subjects);
            target(&c.modifiers, &c.modifier_tickets)
        };
        assert_eq!(
            policy(&[
                "🧩✨ #10 (app): add a view",
                "🧩✨💥 #20 (app): replace an API"
            ])
            .unwrap(),
            None
        );
        assert_eq!(
            policy(&[
                "🧩✨ #10 (app): add a view",
                "🎨💄🚧 #20 (app): mock up the feed",
                "🧩✨🧪 #20 (app): compare variants"
            ])
            .unwrap(),
            Some(20)
        );
        assert!(policy(&["🎨💄🚧 #10 (ui): mock up", "🧩✨🧪 #20 (app): compare"]).is_err());
        for bad in ["—", "JIRA-42", "other/repo#42", "#0", "#01", "#+1", "#"] {
            assert!(issue_number(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_related_entity_must_be_an_open_issue_with_the_required_label() {
        let issue = json!({"number":42,"state":"open","labels":[{"name":"Board Risk"}]});
        assert!(validate_issue(&issue, 42, "board risk").is_ok());
        assert!(validate_issue(&issue, 43, "Board Risk").is_err());
        assert!(validate_issue(&issue, 42, "other").is_err());
        let mut closed = issue.clone();
        closed["state"] = json!("closed");
        assert!(validate_issue(&closed, 42, "Board Risk").is_err());
        let mut pr = issue;
        pr["pull_request"] = json!({});
        assert!(validate_issue(&pr, 42, "Board Risk").is_err());
    }

    #[test]
    fn the_owned_block_can_change_or_disappear_without_consuming_author_text() {
        for before in [
            "",
            "Original PR",
            "Title\r\n\r\n- [ ] author task\r\n",
            "trailing \n\n",
        ] {
            let inserted = reconcile(before, Some(12)).unwrap();
            assert_eq!(reconcile(&inserted, Some(12)).unwrap(), inserted);
            let updated = reconcile(&inserted, Some(13)).unwrap();
            assert!(updated.starts_with(before) && updated.contains("Follow-up issue: #13"));
            assert_eq!(reconcile(&updated, None).unwrap(), before);
        }
        let middle = format!("intro\n\n{}\n\ntail", block(10));
        assert_eq!(
            reconcile(&middle, Some(12)).unwrap(),
            format!("intro\n\n{}\n\ntail", block(12))
        );
        for bad in [
            format!("{START} author paragraph {END}"),
            format!("{}\n{}", block(12), block(12)),
            START.into(),
            END.into(),
        ] {
            assert!(reconcile(&bad, Some(12)).is_err());
        }
    }
}
