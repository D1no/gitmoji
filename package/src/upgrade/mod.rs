//! Rebuild Git operation messages in a separately reviewed candidate history.

mod evidence;
mod objects;
mod pulls;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use clap::Args;
use gitmoji_triangle::vocabulary::Vocabulary;
use serde_json::{Value, json};

use evidence::{Proposal, Review};
use objects::{
    Commit, branch_available, candidate_inventory, command, git, hash_commit, line, partition,
    resolve,
};
use pulls::Pulls;

#[derive(Args, Default, Debug)]
#[group(id = "upgrade_options", multiple = true, requires = "upgrade")]
pub(crate) struct Options {
    /// Look up the original landings in a GitHub repository.
    #[arg(long, value_name = "OWNER/REPO", conflicts_with = "local_merges_only")]
    repo: Option<String>,
    /// Repair local branch merges using only local Git objects.
    #[arg(long, conflicts_with_all=["hostname","update_pr_titles","apply_pr_titles"])]
    local_merges_only: bool,
    /// GitHub hostname (default: GH_HOST or github.com).
    #[arg(long)]
    hostname: Option<String>,
    /// Source tip for the candidate (default: HEAD).
    #[arg(long = "ref", value_name = "REVISION")]
    source: Option<String>,
    /// Exclude this ancestor and all of its history.
    #[arg(long)]
    base: Option<String>,
    /// Create a new local candidate branch after inspecting the preview.
    #[arg(long, value_name = "NEW_BRANCH", conflicts_with = "apply_pr_titles")]
    apply: Option<String>,
    /// Require the candidate to match this reviewed object ID.
    #[arg(long, requires = "apply", value_name = "OID")]
    expect_candidate: Option<String>,
    /// Permit invalidated commit signatures to be removed from the candidate.
    #[arg(long)]
    allow_unsigned: bool,
    /// Include proposed changes to merged PR titles in the preview.
    #[arg(long)]
    update_pr_titles: bool,
    /// Apply hosted titles from this reviewed JSON plan.
    #[arg(long, value_name = "REVIEWED_JSON")]
    apply_pr_titles: Option<PathBuf>,
}

pub(super) fn is_oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64)
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[derive(Default)]
struct Candidate {
    ids: HashMap<String, String>,
    subjects: HashMap<String, String>,
    changes: Vec<Value>,
    signed: Vec<String>,
    blocked: Vec<String>,
    titles: Vec<Value>,
    correct: usize,
    unmatched: usize,
}
impl Candidate {
    fn record_title(
        &mut self,
        prs: &Pulls,
        oid: &str,
        obj: &Commit,
        change: &Proposal,
    ) -> Result<()> {
        if let Some(n) = change.landing {
            let title = std::str::from_utf8(
                change
                    .subject
                    .as_deref()
                    .context("missing generated PR subject")?,
            )?
            .strip_suffix(&format!(" (#{n})"))
            .context("invalid PR subject")?;
            let pr = &prs.numbers[&n];
            if pr.title == title {
                self.correct += 1;
            } else {
                let original = &pr
                    .merge_commit
                    .as_ref()
                    .context("missing PR merge object")?
                    .oid;
                self.titles.push(json!({
                    "number": n,
                    "merge_commit": original,
                    "source_commit": oid,
                    "before": pr.title,
                    "after": title,
                    "evidence": change.reason
                }));
            }
        } else if change.subject.is_none() && (obj.parents.len() >= 2 || prs.oids.contains_key(oid))
        {
            self.unmatched += 1;
        }
        Ok(())
    }
    fn recaption(
        &self,
        oid: &str,
        obj: &Commit,
        change: &Proposal,
        title: &[u8],
    ) -> Result<Vec<u8>> {
        let mut msg = [title, b"\n\n"].concat();
        if let Some(route) = &change.context
            && obj.parents.len() >= 2
            && change.landing.is_none()
        {
            if self.ids.is_empty() {
                msg.extend(route);
            } else {
                msg.extend(candidate_inventory(
                    route,
                    &obj.parents,
                    &self.ids,
                    &self.subjects,
                )?);
            }
            msg.extend(b"\n\n");
        }
        msg.extend(format!("Source commit: {oid}\n").bytes());
        if let Some(route) = &change.context
            && (change.landing.is_some() || obj.parents.len() < 2)
        {
            msg.extend(route);
            msg.push(b'\n');
        }
        if let Some(id) = evidence::target(&obj.message).and_then(|target| self.ids.get(target)) {
            msg.extend(format!("Candidate undo target: {id}\n").bytes());
        }
        msg.push(b'\n');
        msg.extend(&obj.message);
        Ok(msg)
    }
    fn update(
        &mut self,
        oid: &str,
        mut obj: Commit,
        change: Proposal,
        opts: &Options,
    ) -> Result<()> {
        let original_subject = obj.subject();
        let mut entry = json!({"old":oid,"subject":String::from_utf8_lossy(original_subject)});
        let title_changed = change
            .subject
            .as_deref()
            .is_some_and(|s| s != original_subject);
        let mut inventory_changed = false;
        if let Some(title) = &change.subject
            && title_changed
        {
            let subject = std::str::from_utf8(title)?.to_owned();
            entry["subject_new"] = json!(subject);
            entry["evidence"] = json!(change.reason);
            self.subjects.insert(oid.into(), subject);
            obj.message = self.recaption(oid, &obj, &change, title)?;
        } else if !obj.parents.iter().any(|p| self.ids.contains_key(p)) {
            if change.subject.is_none()
                && let Some(reason) = change.reason
            {
                entry["review"] = json!(reason);
                self.changes.push(entry);
            }
            return Ok(());
        } else if change.subject.is_some() && change.landing.is_none() && obj.parents.len() >= 2 {
            let (head, body) = partition(&obj.message, b"\n\n");
            let inventory = partition(body, b"\n\n").0;
            let updated = candidate_inventory(inventory, &obj.parents, &self.ids, &self.subjects)?;
            if updated != inventory {
                obj.message =
                    [head, b"\n\n".as_slice(), &updated, &body[inventory.len()..]].concat();
                inventory_changed = true;
                entry["inventory_updated"] = Value::Bool(true);
                entry["evidence"] = json!("arriving commit rewritten in this candidate");
            }
        } else if change.subject.is_none()
            && let Some(reason) = change.reason
        {
            entry["review"] = json!(reason);
        }
        let mut headers = vec![];
        let mut signed = false;
        let mut blocked = false;
        for field in obj.headers {
            let (key, value) = partition(&field, b" ");
            match key {
                b"gpgsig" | b"gpgsig-sha256" => {
                    signed = true;
                    continue;
                }
                b"mergetag" => blocked = true,
                b"encoding"
                    if (title_changed || inventory_changed)
                        && !value.trim_ascii().eq_ignore_ascii_case(b"utf8")
                        && !value.trim_ascii().eq_ignore_ascii_case(b"utf-8") =>
                {
                    blocked = true
                }
                b"parent" => {
                    if let Some(new) = self.ids.get(std::str::from_utf8(value)?) {
                        headers.push(format!("parent {new}").into_bytes());
                        continue;
                    }
                }
                _ => (),
            }
            headers.push(field);
        }
        if signed {
            self.signed.push(oid.into());
        }
        if blocked {
            self.blocked.push(oid.into());
        }
        let mut raw = headers.join(&b'\n');
        raw.extend(b"\n\n");
        raw.extend(obj.message);
        // Preview never writes. Failed applications can leave only unreachable objects.
        let write = opts.apply.is_some()
            && self.blocked.is_empty()
            && (self.signed.is_empty() || opts.allow_unsigned);
        let new = hash_commit(&raw, write)?;
        entry["candidate"] = json!(new);
        self.ids.insert(oid.into(), new);
        self.changes.push(entry);
        Ok(())
    }
}

fn plan(opts: &Options, vocab: &Vocabulary) -> Result<Value> {
    ensure!(
        opts.repo.is_some() != opts.local_merges_only,
        "select --repo OWNER/REPO or --local-merges-only"
    );
    ensure!(
        line(&["rev-parse", "--is-shallow-repository"])? != "true",
        "history is shallow; use a full clone"
    );
    let grafts = line(&["rev-parse", "--git-path", "info/grafts"])?;
    ensure!(
        !Path::new(&grafts).is_file(),
        "use a complete checkout without grafted history"
    );
    let tip = resolve(opts.source.as_deref().unwrap_or("HEAD"))?;
    let base = opts.base.as_deref().map(resolve).transpose()?;
    let mut revisions = vec!["rev-list", "--reverse", "--topo-order", &tip];
    let excluded = base.as_ref().map(|b| format!("^{b}"));
    if let Some(b) = &base {
        ensure!(
            command()
                .args(["merge-base", "--is-ancestor", b, &tip])
                .output()?
                .status
                .success(),
            "--base must be an ancestor of the source tip"
        );
    }
    if let Some(excluded) = &excluded {
        revisions.push(excluded);
    }
    let branch = opts.apply.as_ref().map(|b| format!("refs/heads/{b}"));
    if let Some(reference) = &branch {
        git(&["check-ref-format", reference])?;
        ensure!(
            !opts.apply.as_ref().is_some_and(|b| b.starts_with('-')),
            "candidate branch name cannot begin with a dash"
        );
        branch_available(reference)?;
    }
    let hostname = opts
        .hostname
        .clone()
        .or_else(|| std::env::var("GH_HOST").ok())
        .unwrap_or_else(|| "github.com".into());
    let mut pulls = opts
        .repo
        .as_ref()
        .map(|r| Pulls::load(&hostname, r, &tip))
        .transpose()?;
    let title_preview = opts.update_pr_titles || opts.apply_pr_titles.is_some();
    let mut c = Candidate::default();
    for oid in line(&revisions)?.lines() {
        let obj = Commit::read(oid)?;
        let change = match evidence::propose(oid, &obj, pulls.as_mut(), vocab) {
            Ok(change) => change,
            Err(err) => Proposal::review(err.downcast::<Review>()?.0),
        };
        if title_preview && let Some(prs) = pulls.as_ref() {
            c.record_title(prs, oid, &obj, &change)?;
        }
        c.update(oid, obj, change, opts)?;
    }
    let new = c.ids.get(&tip).unwrap_or(&tip);
    if let Some(reference) = &branch {
        ensure!(
            c.blocked.is_empty(),
            "candidate touches mergetags or non-UTF-8 captions; review the dry run"
        );
        ensure!(
            c.signed.is_empty() || opts.allow_unsigned,
            "candidate invalidates signatures; review the dry run and --allow-unsigned"
        );
        ensure!(!c.ids.is_empty(), "no verified changes to apply");
        ensure!(
            opts.expect_candidate.as_ref().is_none_or(|s| s == new),
            "candidate tip differs from --expect-candidate; run a new preview"
        );
        branch_available(reference)?;
        git(&[
            "update-ref",
            "--no-deref",
            reference,
            new,
            &"0".repeat(tip.len()),
        ])?;
    }
    let mut result = json!({
        "repository": opts.repo.as_ref().map(|r| format!("{hostname}/{r}")),
        "source_tip": tip,
        "candidate_tip": new,
        "created_branch": opts.apply,
        "review_or_changed_commits": c.changes,
        "invalidated_signatures": c.signed,
        "blocked_mergetags_or_encodings": c.blocked
    });
    if pulls.is_none() {
        result["mode"] = json!("local-merges-only");
    }
    if title_preview {
        result["pr_title_summary"] = json!({
            "to_change": c.titles.len(),
            "already_correct": c.correct,
            "unmatched": c.unmatched
        });
        result["pr_title_updates"] = json!(c.titles);
        result["base"] = json!(base);
    }
    if let Some(path) = &opts.apply_pr_titles {
        let updated = pulls
            .as_ref()
            .context("hosted titles need --repo")?
            .apply_titles(path, &result)?;
        result["updated_pr_titles"] = json!(updated);
    }
    Ok(result)
}

/// Emit a UTF-8 review document. Only an explicit apply flag changes a ref or a hosted title.
pub(crate) fn run(opts: &Options, vocab: &Vocabulary) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(&plan(opts, vocab)?)?);
    Ok(())
}
