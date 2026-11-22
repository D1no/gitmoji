//! GitHub evidence for historical landings, membership and reviewed title updates.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::hosting::Client;

use super::evidence::{caption_number, local_caption};
use super::objects::{Commit, git};

const MERGES: &str = r#"query($owner:String!,$name:String!,$endCursor:String) {
    repository(owner:$owner,name:$name) {
        nameWithOwner
        pullRequests(states:MERGED,first:100,after:$endCursor) {
            nodes {number title mergeCommit {oid} headRefName baseRefName headRepositoryOwner {login}}
            pageInfo {hasNextPage endCursor}
        }
    }
}"#;
const ASSOCIATIONS: &str = r#"query($owner:String!,$name:String!,$oid:GitObjectID!,$endCursor:String) {
    repository(owner:$owner,name:$name) {
        object(oid:$oid) {... on Commit {associatedPullRequests(first:100,after:$endCursor) {
            nodes {number repository {nameWithOwner}}
            pageInfo {hasNextPage endCursor}
        }}}
    }
}"#;
const MEMBERSHIP: &str = r#"query($owner:String!,$name:String!,$number:Int!,$endCursor:String) {
    repository(owner:$owner,name:$name) {
        pullRequest(number:$number) {commits(first:100,after:$endCursor) {
            nodes {commit {oid}} pageInfo {hasNextPage endCursor}
        }}
    }
}"#;

#[derive(Clone, Deserialize)]
pub(super) struct Object {
    pub oid: String,
}
#[derive(Clone, Deserialize)]
pub(super) struct Owner {
    pub login: String,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Pull {
    pub number: u64,
    pub title: String,
    pub merge_commit: Option<Object>,
    pub head_ref_name: String,
    pub base_ref_name: String,
    pub head_repository_owner: Option<Owner>,
}
pub(super) struct Pulls {
    client: Client,
    repo: String,
    pub tip: String,
    pub numbers: HashMap<u64, Pull>,
    pub oids: HashMap<String, Vec<u64>>,
    pub claims: HashMap<u64, HashSet<String>>,
    members: HashMap<u64, HashSet<String>>,
    associations: HashMap<String, Option<u64>>,
}

impl Pulls {
    pub fn load(host: &str, repo: &str, tip: &str) -> Result<Self> {
        let mut pulls = Self {
            client: Client::new(host, repo)?,
            repo: repo.into(),
            tip: tip.into(),
            numbers: HashMap::new(),
            oids: HashMap::new(),
            claims: HashMap::new(),
            members: HashMap::new(),
            associations: HashMap::new(),
        };
        for row in pulls.rows(MERGES, json!({}), "/pullRequests")? {
            let pr: Pull = serde_json::from_value(row)?;
            if let Some(m) = &pr.merge_commit {
                ensure!(
                    super::is_oid(&m.oid),
                    "GitHub returned an invalid merge commit ID"
                );
                pulls.oids.entry(m.oid.clone()).or_default().push(pr.number);
            }
            ensure!(
                pulls.numbers.insert(pr.number, pr).is_none(),
                "duplicate merged pull request in GitHub metadata"
            );
        }
        let log = git(&[
            "log",
            "--no-show-signature",
            "--no-notes",
            "--merges",
            "--format=%H%x00%s",
            tip,
            "--",
        ])?;
        for row in log.split(|&c| c == b'\n').filter(|s| !s.is_empty()) {
            let (oid, subject) = super::objects::partition(row, b"\0");
            let Some(number) = caption_number(subject) else {
                continue;
            };
            if !pulls.numbers.contains_key(&number) {
                continue;
            }
            let oid = std::str::from_utf8(oid)?;
            if !subject.windows(6).any(|s| s == b" into ")
                || !local_caption(&Commit::read(oid)?.message)
            {
                pulls.claims.entry(number).or_default().insert(oid.into());
            }
        }
        Ok(pulls)
    }

    fn rows(&self, query: &str, mut vars: Value, pointer: &str) -> Result<Vec<Value>> {
        let (owner, name) = self.repo.split_once('/').context("use OWNER/REPO")?;
        vars["owner"] = json!(owner);
        vars["name"] = json!(name);
        let mut rows = Vec::new();
        let mut seen = HashSet::new();
        loop {
            let page = self.client.graphql(query, &vars)?;
            let repo = page
                .get("repository")
                .filter(|r| !r.is_null())
                .context("GitHub could not read repository PR metadata")?;
            if query == MERGES {
                ensure!(
                    repo["nameWithOwner"]
                        .as_str()
                        .is_some_and(|s| s.eq_ignore_ascii_case(&self.repo)),
                    "GitHub returned a different repository"
                );
            } else if query == ASSOCIATIONS && repo.get("object").is_some_and(Value::is_null) {
                break;
            }
            let connection = repo
                .pointer(pointer)
                .context("missing GitHub PR connection")?;
            rows.extend(
                connection["nodes"]
                    .as_array()
                    .context("missing GitHub PR nodes")?
                    .iter()
                    .cloned(),
            );
            let info = &connection["pageInfo"];
            if !info["hasNextPage"]
                .as_bool()
                .context("missing GitHub pagination metadata")?
            {
                break;
            }
            let cursor = info["endCursor"]
                .as_str()
                .filter(|c| !c.is_empty())
                .context("missing GitHub pagination cursor")?;
            ensure!(
                seen.insert(cursor.to_owned()),
                "repeated GitHub pagination cursor"
            );
            vars["endCursor"] = json!(cursor);
        }
        Ok(rows)
    }
    pub fn containing(&mut self, oid: &str) -> Result<Option<u64>> {
        if let Some(number) = self.associations.get(oid) {
            return Ok(*number);
        }
        let mut matches = HashSet::new();
        for row in self.rows(
            ASSOCIATIONS,
            json!({"oid":oid}),
            "/object/associatedPullRequests",
        )? {
            let n = row["number"]
                .as_u64()
                .context("missing associated PR number")?;
            if !row
                .pointer("/repository/nameWithOwner")
                .and_then(Value::as_str)
                .is_some_and(|s| s.eq_ignore_ascii_case(&self.repo))
                || !self.numbers.contains_key(&n)
            {
                continue;
            }
            if !self.members.contains_key(&n) {
                let mut oids = HashSet::new();
                for entry in self.rows(MEMBERSHIP, json!({"number":n}), "/pullRequest/commits")? {
                    let id = entry
                        .pointer("/commit/oid")
                        .and_then(Value::as_str)
                        .context("missing PR member commit")?;
                    oids.insert(id.to_owned());
                }
                self.members.insert(n, oids);
            }
            if self.members[&n].contains(oid) {
                matches.insert(n);
            }
        }
        let found = if matches.len() == 1 {
            matches.into_iter().next()
        } else {
            None
        };
        self.associations.insert(oid.to_owned(), found);
        Ok(found)
    }
    pub fn apply_titles(&self, reviewed: &Path, fresh: &Value) -> Result<Vec<u64>> {
        let plan: Value = serde_json::from_slice(&std::fs::read(reviewed)?)?;
        for key in ["repository", "source_tip", "base", "candidate_tip"] {
            ensure!(
                plan.get(key).is_some() && plan[key] == fresh[key],
                "reviewed {key} changed; generate a fresh PR title preview"
            );
        }
        let mut expected: HashMap<u64, &Value> = fresh["pr_title_updates"]
            .as_array()
            .context("missing fresh title plan")?
            .iter()
            .map(|r| Ok((r["number"].as_u64().context("invalid PR number")?, r)))
            .collect::<Result<_>>()?;
        let mut seen = HashSet::new();
        let mut pending = Vec::new();
        for row in plan["pr_title_updates"]
            .as_array()
            .context("use a reviewed --update-pr-titles JSON preview")?
        {
            let n = row["number"]
                .as_u64()
                .context("invalid PR number in title plan")?;
            let pr = self
                .numbers
                .get(&n)
                .context("unknown merged PR in title plan")?;
            ensure!(seen.insert(n), "duplicate merged PR in title plan");
            ensure!(
                pr.merge_commit
                    .as_ref()
                    .is_some_and(|obj| row["merge_commit"] == obj.oid),
                "PR #{n} landing changed; generate a new preview"
            );
            if row["after"] == pr.title && !expected.contains_key(&n) {
                continue;
            }
            ensure!(
                expected.remove(&n) == Some(row),
                "PR #{n} title plan changed; generate a new preview"
            );
            pending.push((n, row));
        }
        ensure!(
            expected.is_empty(),
            "additional PR title changes found; generate a new preview"
        );
        let mut updated = vec![];
        for (n, row) in pending {
            let apply = || -> Result<bool> {
                let live = self.client.get_pull(n)?;
                ensure!(
                    live["number"] == n
                        && live["state"] == "closed"
                        && live["merged_at"].as_str().is_some_and(|s| !s.is_empty())
                        && live
                            .pointer("/base/repo/full_name")
                            .and_then(Value::as_str)
                            .is_some_and(|s| s.eq_ignore_ascii_case(&self.repo))
                        && live["merge_commit_sha"] == row["merge_commit"],
                    "merged PR identity changed"
                );
                if live["title"] == row["after"] {
                    return Ok(false);
                }
                ensure!(
                    live["title"] == row["before"],
                    "title changed since preview"
                );
                let after = row["after"].as_str().context("missing new title")?;
                let result = self.client.update_title(n, after)?;
                ensure!(
                    result["number"] == n && result["title"] == after,
                    "server returned a different title"
                );
                Ok(true)
            };
            match apply() {
                Ok(true) => updated.push(n),
                Ok(false) => (),
                Err(err) => bail!(
                    "PR #{n}: {err:#}. Inspect the PR and rerun the preview; earlier verified updates: {updated:?}"
                ),
            }
        }
        Ok(updated)
    }
}
