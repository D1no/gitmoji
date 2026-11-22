//! Preview a host-provided merge message and bind the direct merge to its reviewed inputs.
use std::fmt::Write;
use std::path::PathBuf;

use anyhow::{Context, Result, bail, ensure};
use clap::Args;
use gitmoji_triangle::vocabulary::Vocabulary;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::{Client, valid_pr};
use crate::history::incoming_commits;

const QUERY: &str = r#"query($owner: String!, $name: String!, $number: Int!) {
  repository(owner: $owner, name: $name) {
    pullRequest(number: $number) {
      id number state isDraft title
      headRefName headRefOid headRepository { nameWithOwner } headRepositoryOwner { login }
      baseRefName baseRef { target { oid } } baseRepository { nameWithOwner }
      viewerMergeHeadlineText(mergeType: MERGE)
      viewerMergeBodyText(mergeType: MERGE)
      isMergeQueueEnabled stack { number size baseRefName } stackEntry { position }
    }
  }
}"#;

#[derive(Args)]
pub(crate) struct MergeArgs {
    /// Target repository, OWNER/REPO.
    #[arg(long)]
    repo: String,
    /// Positive GitHub pull request number.
    #[arg(long)]
    pr: u64,
    /// GitHub host serving the target repository.
    #[arg(long, default_value = "github.com")]
    hostname: String,
    /// UTF-8 original merge text, including conflict notes, to preserve.
    #[arg(long, value_name = "FILE")]
    original_message: Option<PathBuf>,
    /// Submit this exact preview as a direct merge commit.
    #[arg(long, requires = "expect_preview")]
    merge: bool,
    /// SHA-256 from the reviewed preview.
    #[arg(long, requires = "merge")]
    expect_preview: Option<String>,
}

fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .with_context(|| format!("GitHub returned no {key}"))
}

fn body(pr: &Value, original: Option<&str>) -> Result<String> {
    let base = string(&pr["baseRepository"], "nameWithOwner")?;
    let head = string(pr, "headRefName")?;
    let source = if pr["headRepository"].is_null() {
        let owner = pr["headRepositoryOwner"]["login"]
            .as_str()
            .map(|name| format!("{name}:"))
            .unwrap_or_default();
        format!(
            "{owner}{head} (source repository unavailable; refs/pull/{}/head)",
            pr["number"]
        )
    } else {
        format!("{}:{head}", string(&pr["headRepository"], "nameWithOwner")?)
    };
    let mut text = format!(
        "Source: {source}\nBase: {base}:{}",
        string(pr, "baseRefName")?
    );
    if !pr["stack"].is_null() {
        let stack = &pr["stack"];
        let position = pr["stackEntry"]["position"]
            .as_u64()
            .map(|p| format!(", layer {p} of {}", stack["size"]))
            .unwrap_or_default();
        write!(
            text,
            "\nStack #{}{position}, targeting {base}:{}",
            stack["number"],
            string(stack, "baseRefName")?
        )?;
    }
    if let Some(original) = original {
        if !original.is_empty() {
            write!(text, "\n\n{original}")?;
        }
    } else {
        let headline = string(pr, "viewerMergeHeadlineText")?;
        let details = pr["viewerMergeBodyText"].as_str().unwrap_or_default();
        let mut original = headline.to_owned();
        if !details.is_empty() {
            write!(original, "\n\n{details}")?;
        }
        if !original.is_empty() {
            write!(text, "\n\n{original}")?;
        }
    }
    Ok(text)
}

fn direct(pr: &Value) -> Result<()> {
    ensure!(
        pr["stack"].is_null(),
        "use GitHub's native stacked PR merge flow"
    );
    ensure!(
        pr["isMergeQueueEnabled"] == false,
        "submit this PR through its branch's merge queue"
    );
    ensure!(
        pr["isDraft"] == false,
        "mark the draft PR ready for review before merging"
    );
    ensure!(
        !pr["headRepository"].is_null(),
        "the source repository is unavailable; inspect the PR before merging"
    );
    Ok(())
}

fn snapshot_digest(mut snapshot: Value) -> Result<String> {
    // The previous helper used sorted compact JSON with non-ASCII code points
    // escaped as UTF-16, so an existing preview remains usable across upgrades.
    snapshot.sort_all_objects();
    let compact = serde_json::to_string(&snapshot)?;
    let mut hash = Sha256::new();
    for c in compact.chars() {
        if c < '\u{7f}' {
            hash.update([c as u8]);
        } else {
            for unit in c.encode_utf16(&mut [0; 2]) {
                hash.update(format!("\\u{unit:04x}"));
            }
        }
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn commit_oid(value: &Value) -> Result<&str> {
    value
        .as_str()
        .filter(|s| matches!(s.len(), 40 | 64) && s.bytes().all(|c| c.is_ascii_hexdigit()))
        .context("GitHub returned an invalid PR commit ID")
}

/// Describe a later observation separately from GitHub's original refusal.
fn after_405(client: &Client, number: u64, base: &str, head: &str) -> String {
    let Ok(pr) = client.get_pull(number) else {
        return "The follow-up PR lookup failed. Check merge conflicts, required reviews and checks on the PR.".into();
    };
    if pr["number"].as_u64() != Some(number) {
        return "The follow-up lookup did not identify the requested PR. Inspect it on GitHub."
            .into();
    }
    let (refs, same_commits) = match (
        commit_oid(&pr["base"]["sha"]),
        commit_oid(&pr["head"]["sha"]),
    ) {
        (Ok(b), Ok(h)) => match (!b.eq_ignore_ascii_case(base), !h.eq_ignore_ascii_case(head)) {
            (true, true) => (
                "The base and head changed since the reviewed preview.",
                false,
            ),
            (true, false) => ("The base changed since the reviewed preview.", false),
            (false, true) => ("The head changed since the reviewed preview.", false),
            (false, false) => ("The base and head match the reviewed preview.", true),
        },
        _ => (
            "The lookup could not confirm the reviewed revisions.",
            false,
        ),
    };
    let status = if pr["merged"] == true {
        "The PR is now merged. No further submission is needed."
    } else if pr["state"] == "closed" {
        "The PR is now closed."
    } else if pr["state"] != "open" {
        "GitHub did not report whether the PR is open."
    } else if pr["draft"] == true {
        "The PR is now a draft."
    } else {
        match (pr["mergeable_state"].as_str(), &pr["mergeable"]) {
            (Some("dirty"), _) => "GitHub reports merge conflicts.",
            (Some("behind"), _) => "GitHub reports the head branch is behind the base.",
            (Some("blocked"), _) => {
                "GitHub reports a blocked merge. Inspect the required reviews and checks on the PR."
            }
            (Some("clean"), Value::Bool(true)) if same_commits && pr["draft"] == false => {
                "GitHub now reports a clean, mergeable PR. The earlier refusal may have been temporary. Review a fresh preview before another submission."
            }
            (Some("clean"), Value::Bool(true)) => {
                "GitHub now reports a clean, mergeable PR. Review a fresh preview before submitting."
            }
            (_, Value::Bool(false)) => {
                "GitHub currently marks the PR unmergeable. Inspect its merge requirements."
            }
            (Some("unknown"), _) | (_, Value::Null) => {
                "GitHub has not confirmed mergeability yet. Wait for the PR's mergeability check to finish."
            }
            _ => "Inspect merge eligibility, required reviews and checks on the PR.",
        }
    };
    format!("Follow-up PR state: {refs} {status}")
}

fn submit(
    client: &Client,
    pr: u64,
    payload: &Value,
    url: &str,
    base: &str,
    head: &str,
) -> Result<()> {
    let uncertain = || format!("check {url} for the merge outcome before retrying");
    let mut response = client
        .send(
            "PUT",
            &format!("{}/pulls/{pr}/merge", client.rest),
            Some(payload),
        )
        .with_context(uncertain)?;
    let status = response.status();
    if status.is_client_error() {
        let reason = response
            .body_mut()
            .with_config()
            .limit(8192)
            .read_json::<Value>()
            .ok()
            .as_ref()
            .and_then(|value| super::merge_reason::display(value, &client.token));
        let advice = match status.as_u16() {
            405 => after_405(client, pr, base, head),
            409 => "Fetch the current PR head and review a fresh preview before submitting.".into(),
            422 => "Inspect the proposed merge and the repository's merge rules.".into(),
            401 | 403 => "Check the credential's access to merge this pull request.".into(),
            _ => "Inspect the pull request and its merge requirements.".into(),
        };
        if let Some(reason) = reason {
            bail!(
                "GitHub rejected the merge (HTTP {}): {reason}\n{advice} {url}",
                status.as_u16()
            );
        }
        bail!(
            "GitHub rejected the merge (HTTP {}). {advice} {url}",
            status.as_u16()
        );
    }
    ensure!(
        status.is_success(),
        "GitHub returned HTTP {}. {}",
        status.as_u16(),
        uncertain()
    );
    let outcome: Value = response
        .body_mut()
        .with_config()
        .limit(65536)
        .read_json()
        .context("invalid GitHub merge response")
        .with_context(uncertain)?;
    match outcome["merged"].as_bool() {
        Some(true) => {
            if let Ok(sha) = commit_oid(&outcome["sha"]) {
                println!("Merged {url} at {sha}");
            } else {
                println!("Merged {url}");
            }
            Ok(())
        }
        Some(false) => {
            if let Some(reason) = super::merge_reason::display(&outcome, &client.token) {
                bail!("GitHub did not merge the pull request: {reason}\nCheck {url}");
            }
            bail!("GitHub did not merge the pull request. Check {url}");
        }
        None => bail!("GitHub did not confirm the merge outcome. {}", uncertain()),
    }
}

/// Print a merge preview or submit an identical previously reviewed proposal.
pub(crate) fn merge(args: MergeArgs) -> Result<()> {
    valid_pr(args.pr)?;
    if let Some(digest) = &args.expect_preview {
        ensure!(
            digest.len() == 64
                && digest
                    .bytes()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "use the SHA256 digest from the reviewed preview"
        );
    }
    let client = Client::new(&args.hostname, &args.repo)?;
    let (owner, name) = args.repo.split_once('/').context("invalid repository")?;
    let read = || -> Result<Value> {
        let mut result =
            client.graphql(QUERY, &json!({"owner":owner,"name":name,"number":args.pr}))?;
        let pr = result
            .pointer_mut("/repository/pullRequest")
            .map(Value::take)
            .context("GitHub did not return the requested pull request")?;
        ensure!(
            pr["number"].as_u64() == Some(args.pr),
            "could not read PR #{} in {}",
            args.pr,
            args.repo
        );
        ensure!(pr["state"] == "OPEN", "select an open pull request");
        ensure!(
            string(&pr["baseRepository"], "nameWithOwner")?.eq_ignore_ascii_case(&args.repo),
            "GitHub returned a different base repository"
        );
        Ok(pr)
    };
    let pr = read()?;
    if args.merge {
        direct(&pr)?;
    }
    let original = args
        .original_message
        .as_ref()
        .map(std::fs::read_to_string)
        .transpose()?;
    let base = commit_oid(&pr["baseRef"]["target"]["oid"])?;
    let head = commit_oid(&pr["headRefOid"])?;
    let vocab = Vocabulary::builtin()?;
    let incoming = incoming_commits(base, &[head], &vocab).with_context(|| format!("fetch the base branch and refs/pull/{}/head from the target repository into this checkout",args.pr))?;
    let caption = incoming.summarize_pr(args.pr, string(&pr, "title")?, &vocab)?;
    let body = body(&pr, original.as_deref())?;
    let payload = json!({"sha":head,"merge_method":"merge","commit_title":caption.subject,"commit_message":body});
    let digest = snapshot_digest(json!([
        args.hostname,
        args.repo,
        pr,
        payload,
        caption.recognized,
        caption.total
    ]))?;
    if !args.merge {
        eprintln!(
            "Base {base} / Head {head}\nRecognized tags in {} of {} incoming non-merge commits.\nReviewed preview: --expect-preview {digest}",
            caption.recognized, caption.total
        );
        println!("{}", caption.subject);
        println!();
        print!("{body}");
        if !body.ends_with('\n') {
            println!();
        }
        return Ok(());
    }
    ensure!(
        args.expect_preview.as_deref() == Some(digest.as_str()),
        "the PR or proposed message differs from the reviewed preview; inspect a fresh preview"
    );
    ensure!(
        read()? == pr,
        "the PR changed while preparing the caption; inspect a fresh preview"
    );
    let url = format!("{}/{}/pull/{}", client.server, args.repo, args.pr);
    // The host checks the head atomically. Submit exactly once.
    submit(&client, args.pr, &payload, &url, base, head)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    fn pr() -> Value {
        json!({"number":42,"baseRepository":{"nameWithOwner":"octo/repo"},"baseRefName":"trunk",
            "headRepository":{"nameWithOwner":"peer/fork"},"headRepositoryOwner":{"login":"peer"},
            "headRefName":"work","isDraft":false,"isMergeQueueEnabled":false,"stack":null,
            "viewerMergeHeadlineText":"Default title","viewerMergeBodyText":"Original details"})
    }

    #[test]
    fn digest_is_compatible_with_previously_reviewed_canonical_json() {
        let input = json!({"b":"é\u{7f}💾","a":[{"y":1,"x":"\n"}]});
        assert_eq!(
            snapshot_digest(input).unwrap(),
            "a1fe3ef021fe9dbf50a13b35a325ed4741028217f769d1b9cdec938924761f32"
        );
    }

    #[test]
    fn preview_preserves_hosted_provenance_and_user_text() {
        let mut pull = pr();
        assert_eq!(
            body(&pull, None).unwrap(),
            "Source: peer/fork:work\nBase: octo/repo:trunk\n\nDefault title\n\nOriginal details"
        );
        assert!(direct(&pull).is_ok());
        pull["headRepository"] = Value::Null;
        pull["stack"] = json!({"number":3,"size":2,"baseRefName":"previous"});
        pull["stackEntry"] = json!({"position":1});
        let rendered = body(&pull, Some("preserved\r\nnotes\n")).unwrap();
        assert!(
            rendered
                .contains("Source: peer:work (source repository unavailable; refs/pull/42/head)")
        );
        assert!(rendered.ends_with(
            "Stack #3, layer 1 of 2, targeting octo/repo:previous\n\npreserved\r\nnotes\n"
        ));
    }

    #[test]
    fn direct_merge_refuses_unsupported_remote_states() {
        for (key, value) in [
            ("stack", json!({"number":1})),
            ("isMergeQueueEnabled", json!(true)),
            ("isDraft", json!(true)),
            ("headRepository", Value::Null),
        ] {
            let mut pull = pr();
            pull[key] = value;
            assert!(direct(&pull).is_err());
        }
    }
}
