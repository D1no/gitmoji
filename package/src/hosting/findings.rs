//! Transfer explicitly selected review findings after recording their source.
use super::{Client, provisional::follow_up, valid_pr};
use anyhow::{Context, Result, ensure};
use clap::Args;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Args)]
pub(crate) struct FindingsArgs {
    #[arg(long)]
    repo: String,
    #[arg(long)]
    pr: u64,
    #[arg(long, default_value = "github.com")]
    hostname: String,
    /// Required label on the existing follow-up issue.
    #[arg(long, default_value = "provisional")]
    label: String,
    /// Select a thread ID from the preview. Repeat to select multiple findings.
    #[arg(long = "thread", required_if_eq("apply", "true"))]
    threads: Vec<String>,
    /// Record each selected finding, reply to its thread, then resolve it.
    #[arg(long)]
    apply: bool,
}

const THREADS: &str = r#"query($owner:String!,$repo:String!,$pr:Int!,$cursor:String){
 viewer{login} repository(owner:$owner,name:$repo){pullRequest(number:$pr){
 state baseRefOid headRefOid reviewThreads(first:100,after:$cursor){pageInfo{endCursor hasNextPage}
 nodes{id isResolved viewerCanResolve path line originalLine comments(first:1){nodes{databaseId body}}}}}}}"#;
const RESOLVE: &str =
    "mutation($id:ID!){resolveReviewThread(input:{threadId:$id}){thread{id isResolved}}}";
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReviewThread {
    id: String,
    is_resolved: bool,
    viewer_can_resolve: bool,
    path: String,
    line: Option<u64>,
    original_line: Option<u64>,
    comments: Nodes<ReviewComment>,
}
#[derive(Deserialize)]
struct Nodes<T> {
    nodes: Vec<T>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReviewComment {
    database_id: u64,
    body: String,
}
struct Snapshot {
    viewer: String,
    base: String,
    head: String,
    threads: BTreeMap<String, ReviewThread>,
}
fn field<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .with_context(|| format!("missing GitHub field {key}"))
}
fn snapshot(client: &Client, pr: u64) -> Result<Snapshot> {
    let (owner, repo) = client.repo.split_once('/').context("invalid repository")?;
    let mut result = Snapshot {
        viewer: String::new(),
        base: String::new(),
        head: String::new(),
        threads: BTreeMap::new(),
    };
    let mut cursor = None;
    let mut seen = BTreeSet::new();
    loop {
        let data = client.graphql(
            THREADS,
            &json!({ "repo":repo,"owner":owner,"pr":pr,"cursor":cursor }),
        )?;
        let pull = &data["repository"]["pullRequest"];
        ensure!(pull["state"] == "OPEN", "select an open pull request");
        let base = field(pull, "baseRefOid")?;
        let head = field(pull, "headRefOid")?;
        ensure!(
            result.base.is_empty() || (base == result.base && head == result.head),
            "pull request changed; preview again"
        );
        result.base = base.into();
        result.head = head.into();
        result.viewer = field(&data["viewer"], "login")?.into();
        let connection = &pull["reviewThreads"];
        let threads: Nodes<ReviewThread> = serde_json::from_value(connection.clone())?;
        for thread in threads.nodes {
            ensure!(
                result.threads.insert(thread.id.clone(), thread).is_none(),
                "review threads changed; preview again"
            );
        }
        let info = &connection["pageInfo"];
        if !info["hasNextPage"]
            .as_bool()
            .context("missing review thread pagination")?
        {
            return Ok(result);
        }
        let next = field(info, "endCursor")?.to_owned();
        ensure!(seen.insert(next.clone()), "repeated review thread page");
        cursor = Some(next);
    }
}
fn marker(pr: u64, issue: u64, comment: u64) -> String {
    format!("<!-- gitmoji-deferred pr={pr} issue={issue} finding={comment} -->")
}
// Read only the authenticated actor's transfer receipts. A quoted marker or one
// posted by a different participant cannot stand in for recording the finding.
fn receipts(
    client: &Client,
    path: &str,
    viewer: &str,
    replies: bool,
) -> Result<BTreeSet<(String, Option<u64>)>> {
    let mut found = BTreeSet::new();
    for page in 1.. {
        let response = client.request(
            "GET",
            &format!("{}/{path}?per_page=100&page={page}", client.rest),
            None,
        )?;
        let values = response.as_array().context("expected a page of comments")?;
        for value in values {
            if value["user"]["login"]
                .as_str()
                .is_some_and(|login| login.eq_ignore_ascii_case(viewer))
                && let Some(last) = value["body"].as_str().and_then(|body| body.lines().last())
                && last.starts_with("<!-- gitmoji-deferred ")
            {
                let parent = if replies {
                    value["in_reply_to_id"].as_u64()
                } else {
                    None
                };
                found.insert((last.to_owned(), parent));
            }
        }
        if values.len() < 100 {
            break;
        }
    }
    Ok(found)
}
fn evidence(thread: &ReviewThread, source: &str, receipt: &str) -> Result<String> {
    let comment = thread
        .comments
        .nodes
        .first()
        .context("thread has no finding")?;
    let line = thread
        .line
        .or(thread.original_line)
        .map(|n| format!(":{n}"))
        .unwrap_or_default();
    // A fence longer than any run in the excerpt keeps the original wording
    // literal even if the reviewer pasted Markdown or a fenced code sample.
    let text = format!("{}{line}\n\n{}", thread.path, comment.body);
    let mut chars = text.chars();
    let mut excerpt: String = chars.by_ref().take(10000).collect();
    if chars.next().is_some() {
        excerpt.push_str("\n[Excerpt ends here. See the linked finding for the rest.]");
    }
    let longest = excerpt.split(|c| c != '~').map(str::len).max().unwrap_or(0);
    let fence = "~".repeat((longest + 1).max(3));
    Ok(format!(
        "- [ ] Address [deferred review finding]({source})\n\n{fence}\n{excerpt}\n{fence}\n\n{receipt}"
    ))
}
pub(crate) fn defer_findings(args: FindingsArgs) -> Result<()> {
    valid_pr(args.pr)?;
    ensure!(
        !args.apply || !args.threads.is_empty(),
        "--apply requires at least one --thread from the preview"
    );
    let client = Client::new(&args.hostname, &args.repo)?;
    let snap = snapshot(&client, args.pr)?;
    let current = client
        .pull_refs(args.pr, &client.get_pull(args.pr)?)?
        .context("the pull request is closed")?;
    ensure!(
        current.head == snap.head && current.base == snap.base,
        "pull request changed; preview again"
    );
    let issue = follow_up(&client, args.pr, &snap.base, &snap.head, &args.label)?
        .context("this PR has no provisional work")?;
    let selected: BTreeSet<_> = args.threads.iter().collect();
    ensure!(
        selected.len() == args.threads.len(),
        "duplicate thread selection"
    );
    for id in &selected {
        let thread = snap
            .threads
            .get(*id)
            .context("selected thread does not belong to this pull request")?;
        ensure!(
            !args.apply || thread.is_resolved || thread.viewer_can_resolve,
            "cannot resolve selected thread {id}"
        );
        ensure!(
            !thread.comments.nodes.is_empty(),
            "selected thread has no finding"
        );
    }
    let threads: Vec<_> = snap
        .threads
        .values()
        .filter(|t| !t.is_resolved && (selected.is_empty() || selected.contains(&t.id)))
        .collect();
    println!("Follow-up #{}", issue);
    if !args.apply {
        for thread in &threads {
            let comment = thread
                .comments
                .nodes
                .first()
                .context("thread has no finding")?;
            println!(
                "{}  {}  {}\n{}",
                thread.id,
                thread.path,
                format_args!(
                    "{}/{}/pull/{}#discussion_r{}",
                    client.server, client.repo, args.pr, comment.database_id
                ),
                comment.body
            );
        }
        return Ok(());
    }
    if threads.is_empty() {
        return Ok(());
    }
    let issues = format!("issues/{issue}/comments");
    let pulls = format!("pulls/{}/comments", args.pr);
    let recorded = receipts(&client, &issues, &snap.viewer, false)?;
    let replied = receipts(&client, &pulls, &snap.viewer, true)?;
    for thread in threads {
        let live = client
            .pull_refs(args.pr, &client.get_pull(args.pr)?)?
            .context("the pull request is closed")?;
        ensure!(
            live.head == snap.head && live.base == snap.base,
            "pull request changed; preview again"
        );
        let id = thread.comments.nodes[0].database_id;
        let receipt = marker(args.pr, issue, id);
        if !recorded.contains(&(receipt.clone(), None)) {
            let source = format!(
                "{}/{}/pull/{}#discussion_r{id}",
                client.server, client.repo, args.pr
            );
            let body = evidence(thread, &source, &receipt)?;
            client.request(
                "POST",
                &format!("{}/{issues}", client.rest),
                Some(&json!({"body":body})),
            )?;
        }
        if !replied.contains(&(receipt.clone(), Some(id))) {
            client.request(
                "POST",
                &format!("{}/{pulls}/{id}/replies", client.rest),
                Some(&json!({"body":format!("Moved to #{issue}.\n\n{receipt}")})),
            )?;
        }
        let result = client.graphql(RESOLVE, &json!({"id":thread.id}))?;
        let resolved = &result["resolveReviewThread"]["thread"];
        ensure!(
            resolved["id"] == thread.id && resolved["isResolved"] == true,
            "host did not confirm resolution of {}",
            thread.id
        );
        println!("{} moved to #{} and resolved", thread.id, issue);
    }
    Ok(())
}
