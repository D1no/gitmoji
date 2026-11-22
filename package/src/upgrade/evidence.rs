//! Only propose a historical operation when its recorded Git and PR evidence agree.

use std::{collections::HashSet, sync::LazyLock};

use anyhow::{Context, Result, bail};
use gitmoji_triangle::{
    caption::{self, Caption},
    vocabulary::Vocabulary,
};
use regex::bytes::Regex;

use super::{
    objects::{Commit, command, git, line, partition, tree_changes},
    pulls::{Pull, Pulls},
};

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub(super) struct Review(pub String);

macro_rules! review {
    ($($args:tt)*) => {return Err(Review(format!($($args)*)).into())};
}
fn pattern(s: &str) -> Regex {
    Regex::new(s).unwrap_or_else(|err| unreachable!("{err}"))
}
static UNDO: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?i-u)^(?:revert(?:ed)?\b|reapply\b|rewind\b|\xE2\x8F\xAA)"));
static GENERATED: LazyLock<Regex> = LazyLock::new(|| {
    pattern(r"(?i-u)^(?:merge(?:d)?\b|sync\b|cherry-pick\b|(?:squash|fixup|amend)!)")
});
static TARGET: LazyLock<Regex> = LazyLock::new(|| {
    pattern(r"(?m-u)^This reverts commit ([0-9a-f]{40}|[0-9a-f]{64})(?:\.|, reversing)$")
});
static DONE_UNDO: LazyLock<Regex> = LazyLock::new(|| {
    pattern(r"(?-u)^(?:Revert|Reapply) #[0-9]+ [^\n]+\n\nSource commit: [0-9a-f]+\nTarget:")
});
static LEGACY: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?-u)^Merge #([1-9][0-9]{0,9}) \[\x60.+?\] .+$"));
static LANDED: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?-u)^\[\x60.+?\] .+ \(#([1-9][0-9]{0,9})\)$"));

pub(super) fn target(message: &[u8]) -> Option<&str> {
    let mut found = TARGET.captures_iter(message);
    let first = found.next()?.get(1)?;
    if found.next().is_some() {
        return None;
    }
    std::str::from_utf8(first.as_bytes()).ok()
}

pub(super) fn caption_number(subject: &[u8]) -> Option<u64> {
    let m = LEGACY
        .captures(subject)
        .or_else(|| LANDED.captures(subject))?;
    std::str::from_utf8(m.get(1)?.as_bytes()).ok()?.parse().ok()
}

pub(super) fn local_caption(message: &[u8]) -> bool {
    let (subject, body) = partition(message, b"\n\n");
    let subject = if subject.starts_with(b"[`") {
        partition(subject, b"] ").1
    } else {
        subject
    };
    let (route, summary) = partition(subject, b": ");
    let (src, dst) = partition(route, b" into ");
    if src.is_empty() || dst.is_empty() || summary.is_empty() {
        return false;
    }
    let Some(count) = partition(body, b"\n")
        .0
        .strip_prefix(b"Arrived from ")
        .and_then(|v| v.strip_prefix(src))
        .and_then(|v| v.strip_prefix(b" ("))
        .and_then(|v| v.strip_suffix(b"):"))
    else {
        return false;
    };
    !count.is_empty() && count.iter().all(u8::is_ascii_digit)
}

fn checked_title<'a>(title: &'a str, label: &str) -> Result<&'a str> {
    if title.trim().is_empty()
        || title
            .chars()
            .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'))
    {
        review!("{label} needs a nonempty, single-line subject without controls");
    }
    Ok(title)
}

fn target_title(obj: &Commit) -> Result<String> {
    let raw = obj.subject();
    let encoding = obj.header(b"encoding ").unwrap_or(b"utf-8");
    let error = || Review("cannot decode the target subject with its declared encoding".into());
    let subject = if encoding.eq_ignore_ascii_case(b"ISO-8859-1")
        || encoding.eq_ignore_ascii_case(b"latin1")
    {
        raw.iter().map(|&c| char::from(c)).collect()
    } else if encoding.eq_ignore_ascii_case(b"ascii") || encoding.eq_ignore_ascii_case(b"us-ascii")
    {
        if !raw.is_ascii() {
            return Err(error().into());
        }
        std::str::from_utf8(raw)?.to_owned()
    } else {
        let enc = encoding_rs::Encoding::for_label(encoding).ok_or_else(error)?;
        enc.decode_without_bom_handling_and_without_replacement(raw)
            .ok_or_else(error)?
            .into_owned()
    };
    Ok(checked_title(&subject, "target commit")?.into())
}

fn merge_caption(
    pr: &Pull,
    parents: &[String],
    source: &str,
    vocab: &Vocabulary,
) -> Result<Caption> {
    checked_title(source, "pull request")?;
    let incoming = crate::history::incoming_commits_with(&parents[0], &[&parents[1]], git_text)?;
    caption::summarize_pr(
        pr.number,
        source,
        &incoming.subjects,
        incoming.contains_merge,
        vocab,
    )
    .map_err(anyhow::Error::msg)
}

fn rewritten_evidence(original: &str, oid: &str, parents: &[String]) -> Result<&'static str> {
    let previous = Commit::read(original)?;
    if previous.parents.len() != 2 {
        review!(
            "original landing {original} has {} parents; expected two",
            previous.parents.len()
        );
    }
    let old = [original, &previous.parents[0], &previous.parents[1]];
    let current = [oid, &parents[0], &parents[1]];
    let labels = ["merge result", "first parent", "second parent"];
    let mut differences = Vec::new();
    for ((a, b), label) in old.iter().zip(current).zip(labels) {
        if line(&["rev-parse", &format!("{a}^{{tree}}")])?
            != line(&["rev-parse", &format!("{b}^{{tree}}")])?
        {
            differences.push(label);
        }
    }
    if differences.is_empty() {
        return Ok("matching merge and ordered parent trees");
    }
    let cause = format!(
        "original landing {original} has different trees ({})",
        differences.join(", ")
    );
    let mut nonempty = false;
    for i in 1..3 {
        let previous_delta = tree_changes(&[old[i], original])?;
        if previous_delta != tree_changes(&[current[i], oid])? {
            review!("{cause}; changes from the {} differ", labels[i]);
        }
        nonempty |= !previous_delta.is_empty();
    }
    if !nonempty {
        review!("{cause}; both parent changes are empty");
    }
    for i in 0..3 {
        let old = Commit::read(old[i])?;
        let current = Commit::read(current[i])?;
        let authors = |obj: Commit| {
            obj.headers
                .into_iter()
                .filter(|f| f.starts_with(b"author "))
                .collect::<Vec<_>>()
        };
        let old = authors(old);
        if old.len() != 1 || old != authors(current) {
            review!(
                "{cause}; author or author date differs at the {}",
                labels[i]
            );
        }
    }
    Ok("matching changes from both ordered parents and matching author records")
}

fn verified_landing(
    oid: &str,
    obj: &Commit,
    prs: &Pulls,
    vocab: &Vocabulary,
) -> Result<Option<(u64, Caption, String)>> {
    let old = obj.subject();
    let number = if local_caption(&obj.message) {
        None
    } else {
        caption_number(old)
    };
    if let Some(ids) = prs.oids.get(oid) {
        let [n] = ids.as_slice() else {
            review!("multiple merged PRs claim this commit");
        };
        if number.is_some_and(|other| other != *n) {
            review!("caption number conflicts with the exact merged PR");
        }
        let pr = &prs.numbers[n];
        let untagged = !old.starts_with(b"Merge ")
            && old.ends_with(format!(" (#{n})").as_bytes())
            && !local_caption(&obj.message);
        let title = if number.is_some() || untagged {
            std::str::from_utf8(old).map_err(|_| Review("cannot decode the landed title".into()))?
        } else {
            &pr.title
        };
        return Ok(Some((
            *n,
            merge_caption(pr, &obj.parents, title, vocab)?,
            "exact merged PR commit".into(),
        )));
    }
    let Some(n) = number else {
        return Ok(None);
    };
    let Some(pr) = prs.numbers.get(&n).filter(|p| p.merge_commit.is_some()) else {
        review!("caption has no recorded merged PR in this repository");
    };
    if !prs
        .claims
        .get(&n)
        .is_some_and(|claims| claims.len() == 1 && claims.contains(oid))
    {
        review!("multiple local merges carry this PR caption");
    }
    let original = &pr
        .merge_commit
        .as_ref()
        .context("missing original landing")?
        .oid;
    if prs.oids.get(original).is_none_or(|ids| ids.len() != 1) {
        review!("multiple merged PRs claim the original landing");
    }
    let mut evidence = "unique caption matches merged PR title and incoming inventory".to_string();
    if command()
        .args(["cat-file", "-e", &format!("{original}^{{commit}}")])
        .output()?
        .status
        .success()
    {
        if command()
            .args(["merge-base", "--is-ancestor", original, &prs.tip])
            .output()?
            .status
            .success()
        {
            review!("GitHub’s actual merge is already in the source history");
        }
        evidence.push_str("; ");
        evidence.push_str(rewritten_evidence(original, oid, &obj.parents)?);
    }
    let caption = merge_caption(pr, &obj.parents, &pr.title, vocab)?;
    if caption.recognized == 0 {
        review!("caption has no corroborating commit inventory");
    }
    let old =
        std::str::from_utf8(old).map_err(|_| Review("cannot decode the landed title".into()))?;
    let local = old
        .strip_prefix(&format!("Merge #{n} "))
        .unwrap_or(old)
        .trim_end_matches(&format!(" (#{n})")[..]);
    if local != caption.title && caption.previous_title.as_deref() != Some(local) {
        review!("caption does not match the merged PR title and incoming inventory");
    }
    Ok(Some((n, caption, evidence)))
}

#[derive(Default)]
pub(super) struct Proposal {
    pub subject: Option<Vec<u8>>,
    pub reason: Option<String>,
    pub context: Option<Vec<u8>>,
    pub landing: Option<u64>,
}
impl Proposal {
    pub fn review(reason: impl Into<String>) -> Self {
        Self {
            reason: Some(reason.into()),
            ..Self::default()
        }
    }
}

// Match the live caption renderer’s treatment of legacy subjects while using
// the offline Git reader for every object lookup in a candidate history.
fn git_text(args: &[&str]) -> Result<String> {
    Ok(String::from_utf8_lossy(&git(args)?).into_owned())
}

fn branch(oid: &str, obj: &Commit, vocab: &Vocabulary) -> Result<Option<Proposal>> {
    if local_caption(&obj.message) {
        return Ok(Some(Proposal {
            subject: Some(obj.subject().to_vec()),
            ..Proposal::default()
        }));
    }
    if !obj.subject().starts_with(b"Merge ") {
        return Ok(None);
    }
    let Some(rendered) = crate::local_merge::historical_with(oid, vocab, git_text)? else {
        return Ok(None);
    };
    let (title, body) = rendered
        .trim_end_matches('\n')
        .split_once("\n\n")
        .context("incomplete branch caption")?;
    if !local_caption(rendered.as_bytes()) {
        bail!("invalid branch inventory");
    }
    Ok(Some(Proposal {
        subject: Some(checked_title(title, "branch merge")?.as_bytes().to_vec()),
        reason: Some("recorded branch direction and incoming Git parents".into()),
        context: Some(body.as_bytes().to_vec()),
        landing: None,
    }))
}

fn undo(
    mut oid: String,
    mut parent: String,
    mut msg: Vec<u8>,
    prs: &mut Pulls,
    vocab: &Vocabulary,
) -> Result<(u64, bool, String, &'static str)> {
    let mut seen = HashSet::new();
    let mut revert = false;
    loop {
        let Some(target) = target(&msg) else {
            review!("undo needs one full Git revert target ID");
        };
        if !seen.insert(target.to_owned())
            || !command()
                .args(["merge-base", "--is-ancestor", target, &parent])
                .output()?
                .status
                .success()
        {
            review!("undo target is outside its parent history");
        }
        let obj = Commit::read(target)?;
        if obj.parents.len() > 1 {
            let mainline = format!(
                "This reverts commit {target}, reversing\nchanges made to {}.",
                obj.parents[0]
            );
            if obj.parents.len() != 2
                || !msg
                    .windows(mainline.len())
                    .any(|w| w == mainline.as_bytes())
            {
                review!("undo needs the first parent of a two-parent merge as its mainline");
            }
        }
        let inverse = if let Some(first) = obj.parents.first() {
            tree_changes(&[target, first])?
        } else {
            tree_changes(&["--root", "-R", target])?
        };
        if inverse.is_empty() || inverse != tree_changes(&[&parent, &oid])? {
            review!("changed paths, modes or blobs do not prove an exact inverse");
        }
        revert = !revert;
        let candidates = prs.oids.get(target);
        if candidates.is_some_and(|m| m.len() > 1) {
            review!("multiple merged PRs claim the undo target");
        }
        if obj.parents.len() == 2
            && let Some((n, c, _)) = verified_landing(target, &obj, prs, vocab)?
        {
            return Ok((n, revert, c.title, "merge"));
        }
        if obj.parents.len() == 1 && UNDO.is_match(&obj.message) && TARGET.is_match(&obj.message) {
            if candidates.is_some() {
                review!("undo chain includes an ambiguous one-parent PR landing");
            }
            oid = target.to_owned();
            parent.clone_from(&obj.parents[0]);
            msg = obj.message;
            continue;
        }
        if let Some(ids) = candidates {
            let n = ids[0];
            let original = target_title(&obj)?;
            let mut title = &original[..];
            let suffix = format!(" (#{n})");
            while let Some(before) = title.strip_suffix(&suffix).filter(|s| !s.trim().is_empty()) {
                title = before;
            }
            if let Some(legacy) = title.strip_prefix(&format!("Merge #{n} ")) {
                // The released empty-range renderer strips only canonical decorations.
                if legacy == prs.numbers[&n].title
                    || caption::summarize(n, title, &[], vocab)
                        .map_err(anyhow::Error::msg)?
                        .title
                        != title
                {
                    title = legacy;
                }
            }
            return Ok((
                n,
                revert,
                checked_title(title, "landing commit")?.into(),
                "landing commit",
            ));
        }
        if obj.parents.len() <= 1
            && let Some(n) = prs.containing(target)?
        {
            return Ok((n, revert, target_title(&obj)?, "member commit"));
        }
        review!("undo target has no unique verified merged PR");
    }
}

pub(super) fn propose(
    oid: &str,
    obj: &Commit,
    prs: Option<&mut Pulls>,
    vocab: &Vocabulary,
) -> Result<Proposal> {
    let old = obj.subject();
    let Some(prs) = prs else {
        if obj.parents.len() >= 2 {
            if let Some(p) = branch(oid, obj, vocab)? {
                return Ok(p);
            }
            if old.starts_with(b"Merge branch") || old.starts_with(b"Merge remote-tracking branch")
            {
                return Ok(Proposal::review(
                    "branch direction is missing or ambiguous in the recorded heading",
                ));
            }
        }
        return Ok(Proposal::default());
    };
    if obj.parents.len() == 2
        && let Some((n, c, why)) = verified_landing(oid, obj, prs, vocab)?
    {
        let pr = &prs.numbers[&n];
        let owner = pr
            .head_repository_owner
            .as_ref()
            .map_or(String::new(), |o| format!("{}:", o.login));
        let route = format!(
            "Merged from {owner}{} into {} (PR #{n}).",
            pr.head_ref_name, pr.base_ref_name
        );
        return Ok(Proposal {
            subject: Some(c.subject.into_bytes()),
            reason: Some(why),
            context: Some(route.into_bytes()),
            landing: Some(n),
        });
    }
    if let Some(ids) = prs.oids.get(oid) {
        return Ok(Proposal::review(if ids.len() > 1 {
            "multiple merged PRs claim this commit"
        } else {
            "one-parent or nonstandard PR landing requires review of the original operation"
        }));
    }
    if obj.parents.len() >= 2 {
        return Ok(branch(oid, obj, vocab)?.unwrap_or_else(|| {
            Proposal::review(
                "merge without a unique two-parent PR landing or recorded branch direction",
            )
        }));
    }
    if obj.parents.len() == 1 && UNDO.is_match(old) {
        if DONE_UNDO.is_match(&obj.message) {
            return Ok(Proposal::review(
                "already captioned undo; retain its original provenance",
            ));
        }
        let (n, revert, title, kind) = undo(
            oid.into(),
            obj.parents[0].clone(),
            obj.message.clone(),
            prs,
            vocab,
        )?;
        let verb = if revert { "Revert" } else { "Reapply" };
        let mut route = format!("Target: {kind} of PR #{n}.");
        if let Some(other) = prs.containing(oid)?.filter(|&x| x != n) {
            route.push_str(&format!(" Introduced by PR #{other}."));
        }
        return Ok(Proposal {
            subject: Some(format!("{verb} #{n} {title}").into_bytes()),
            reason: Some("exact inverse and verified target PR".into()),
            context: Some(route.into_bytes()),
            landing: None,
        });
    }
    Ok(
        if UNDO.is_match(old) || GENERATED.is_match(old) || TARGET.is_match(&obj.message) {
            Proposal::review("requires review of the original operation")
        } else {
            Proposal::default()
        },
    )
}
