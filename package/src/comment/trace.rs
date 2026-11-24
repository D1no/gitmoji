//! Trace the raw Markdown of one GitHub post and the edit evidence visible to the caller.

use anyhow::{Context, Result, bail, ensure};
use clap::Args;
use serde::Serialize;
use serde_json::{Value, json};

use super::inspect;
use crate::hosting::Client;

#[derive(Args)]
pub(super) struct TraceArgs {
    /// HTTPS permalink to an issue, PR, conversation comment, inline comment or submitted review.
    url: String,
    /// Trusted GitHub hostname for credentials (default: GH_HOST or github.com).
    #[arg(long)]
    hostname: Option<String>,
    /// Emit compact JSON instead of indented JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Post {
    Issue,
    PullRequest,
    Conversation(u64),
    Inline(u64),
    Review(u64),
}

impl Post {
    fn type_name(self) -> &'static str {
        match self {
            Self::Issue => "Issue",
            Self::PullRequest => "PullRequest",
            Self::Conversation(_) => "IssueComment",
            Self::Inline(_) => "PullRequestReviewComment",
            Self::Review(_) => "PullRequestReview",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Target {
    repo: String,
    number: u64,
    post: Post,
    description_id: Option<u64>,
}

fn number(text: &str) -> Result<u64> {
    ensure!(
        !text.starts_with('0') && text.bytes().all(|c| c.is_ascii_digit()),
        "expected a positive numeric GitHub identifier"
    );
    text.parse().context("invalid GitHub identifier")
}

impl Target {
    fn parse(url: &str, hostname: &str) -> Result<Self> {
        let (host, path) = url
            .strip_prefix("https://")
            .and_then(|url| url.split_once('/'))
            .context("use an HTTPS GitHub permalink")?;
        ensure!(
            host.eq_ignore_ascii_case(hostname),
            "permalink host differs from the configured GitHub host; for Enterprise, set --hostname or GH_HOST"
        );
        let (path, fragment) = path
            .split_once('#')
            .map_or((path, None), |(path, fragment)| (path, Some(fragment)));
        let parts: Vec<_> = path.trim_end_matches('/').split('/').collect();
        let [owner, repo, section, number_str, tail @ ..] = parts.as_slice() else {
            bail!("expected an issue or pull request permalink");
        };
        // Client::new independently validates OWNER/REPO before any credential lookup.
        ensure!(!owner.is_empty() && !repo.is_empty(), "expected OWNER/REPO");
        let number_id = number(number_str)?;
        // GitHub's Copy link on a PR description uses the numeric *issue* ID.
        let description_id = fragment
            .and_then(|tag| tag.strip_prefix("issue-"))
            .map(number)
            .transpose()?;
        let fragment = fragment.filter(|_| description_id.is_none());
        let post = match (*section, tail, fragment) {
            ("issues", [], None) => Post::Issue,
            ("pull", [], None) => Post::PullRequest,
            ("issues" | "pull", [], Some(tag)) if tag.starts_with("issuecomment-") => {
                Post::Conversation(number(&tag["issuecomment-".len()..])?)
            }
            ("pull", [] | ["files"] | ["changes"], Some(tag))
                if tag.starts_with("discussion_r") =>
            {
                Post::Inline(number(&tag["discussion_r".len()..])?)
            }
            ("pull", ["files"] | ["changes"], Some(tag)) if tag.starts_with('r') => {
                Post::Inline(number(&tag[1..])?)
            }
            ("pull", [], Some(tag)) if tag.starts_with("pullrequestreview-") => {
                Post::Review(number(&tag["pullrequestreview-".len()..])?)
            }
            _ => bail!("unsupported GitHub post path or fragment; use the post's permalink"),
        };
        Ok(Self {
            repo: format!("{owner}/{repo}").to_ascii_lowercase(),
            number: number_id,
            post,
            description_id,
        })
    }

    fn endpoint(&self) -> String {
        let n = self.number;
        if self.description_id.is_some() {
            return format!("issues/{n}");
        }
        match self.post {
            Post::Issue => format!("issues/{n}"),
            Post::PullRequest => format!("pulls/{n}"),
            Post::Conversation(id) => format!("issues/comments/{id}"),
            Post::Inline(id) => format!("pulls/comments/{id}"),
            Post::Review(id) => format!("pulls/{n}/reviews/{id}"),
        }
    }
}

const EDITS: &str = "query GitmojiPostEdits($id:ID!) {
    node(id:$id) {
        id __typename
        ... on Comment { body createdAt updatedAt lastEditedAt editor { login }
                         userContentEdits(first:20) { totalCount pageInfo { hasNextPage }
                            nodes { editedAt editor { login } deletedAt } } }
    }
}";

#[derive(Serialize)]
struct Trace<'a> {
    url: &'a str,
    kind: &'static str,
    created_at: Option<&'a str>,
    updated_at: Option<&'a str>,
    submitted_at: Option<&'a str>,
    edit_history: Value,
    #[serde(flatten)]
    inspection: super::Inspection,
}

fn verified_url<'a>(target: &Target, host: &str, post: &'a Value) -> Result<&'a str> {
    // The comment lookup is repository-wide. Bind both its ID and its parent number
    // to the requested link before attributing anyone's post.
    let url = post["html_url"]
        .as_str()
        .context("missing GitHub post permalink")?;
    let mut found = Target::parse(url, host)?;
    if let Some(id) = target.description_id {
        ensure!(post["id"].as_u64() == Some(id), "different description ID");
        found.description_id = Some(id);
    }
    ensure!(
        found == *target,
        "GitHub returned a different post or parent"
    );
    Ok(url)
}

fn edit_history(node: &Value) -> Value {
    let connection = &node["userContentEdits"];
    let (Some(count), Some(entries), Some(has_more)) = (
        connection["totalCount"].as_u64(),
        connection["nodes"].as_array(),
        connection["pageInfo"]["hasNextPage"].as_bool(),
    ) else {
        return json!({"available": false, "last_edited_at": node["lastEditedAt"],
                      "editor_account": node["editor"]["login"]});
    };
    let revisions: Vec<_> = entries
        .iter()
        .filter(|entry| entry.is_object())
        .map(|entry| {
            json!({"edited_at": entry["editedAt"], "editor_account": entry["editor"]["login"],
               "deleted_at": entry["deletedAt"]})
        })
        .collect();
    json!({"available": true, "last_edited_at": node["lastEditedAt"], "editor_account": node["editor"]["login"],
           "visible_entry_count": count, "revisions": revisions, "has_more": has_more})
}

fn trace<'a>(
    target: &Target,
    url: &'a str,
    post: &'a Value,
    edits: &'a Value,
) -> Result<Trace<'a>> {
    let rest_body = match post.get("body") {
        Some(Value::String(body)) => body.as_str(),
        Some(Value::Null) => "",
        _ => bail!("GitHub returned no raw Markdown body"),
    };
    let edit_node = edits.get("node").filter(|node| {
        node["id"].as_str().is_some()
            && node["id"] == post["node_id"]
            && node["__typename"] == target.post.type_name()
            && node["body"].is_string()
    });
    let (body, created_at, updated_at, edit_history) = if let Some(node) = edit_node {
        // Use body and edit evidence from the same observation if a post changed
        // between the REST and GraphQL reads.
        (
            node["body"].as_str().unwrap_or_default(),
            node["createdAt"].as_str(),
            node["updatedAt"].as_str(),
            edit_history(node),
        )
    } else {
        (
            rest_body,
            post["created_at"].as_str(),
            post["updated_at"].as_str(),
            json!({"available":false}),
        )
    };
    Ok(Trace {
        url,
        kind: target.post.type_name(),
        created_at,
        updated_at,
        submitted_at: post["submitted_at"].as_str(),
        edit_history,
        inspection: inspect(body),
    })
}

pub(super) fn run(args: TraceArgs) -> Result<u8> {
    // The supplied URL identifies a post, never a new destination for an ambient token.
    let host = args
        .hostname
        .or_else(|| std::env::var("GH_HOST").ok())
        .unwrap_or_else(|| "github.com".to_owned())
        .to_ascii_lowercase();
    let target = Target::parse(&args.url, &host)?;
    let client = Client::new(&host, &target.repo)?;
    let post = client.get(&target.endpoint())?;
    let url = verified_url(&target, &host, &post)?;
    let edits = post["node_id"]
        .as_str()
        .and_then(|id| client.graphql(EDITS, &json!({"id":id})).ok())
        .unwrap_or(Value::Null);
    let result = trace(&target, url, &post, &edits)?;
    let output = if args.json {
        serde_json::to_string(&result)?
    } else {
        serde_json::to_string_pretty(&result)?
    };
    println!("{output}");
    Ok(0)
}
