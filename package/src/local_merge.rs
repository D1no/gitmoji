//! Captions for real branch merges, using the first parent as the receiving side.

use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use anyhow::{Context, Result};
use regex::Regex;

use crate::history::{git, incoming_commits};
use gitmoji_triangle::{caption, vocabulary::Vocabulary};

const BODY_LIMIT: usize = 20;

// Git quotes each name. Refs cannot contain whitespace, so an apostrophe within
// a name is unambiguous. The remote after "of" is only provenance, never a name.
static SOURCE_GROUP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(remote-tracking branch(?:es)?|branch(?:es)?|tags?|commits?) ('[^\s]+'(?:(?:, | and )'[^\s]+')*)").unwrap_or_else(|e| unreachable!("{e}"))
});

struct Direction {
    sources: String,
    target: String,
    // For live Git messages verify the names in the heading really point to the
    // incoming parents. A hand-written -m message can use default-looking prose.
    source_refs: Vec<String>,
    fetched: bool,
}

fn branch_name(raw: &str, remote: bool) -> &str {
    let name = raw.strip_prefix("refs/heads/").unwrap_or(raw);
    if remote {
        let name = name.strip_prefix("refs/remotes/").unwrap_or(name);
        name.split_once('/').map_or(name, |(_, branch)| branch)
    } else {
        name
    }
}

/// Accept the complete Git heading. Ordinary English starting with "Merge" is
/// common in hand-written messages. The current branch only supplies a missing
/// destination while the merge is in progress.
fn direction(subject: &str, current: Option<&str>, n_heads: usize) -> Option<Direction> {
    let title = subject.strip_prefix("Merge ")?;
    let (source, target) = match title.rsplit_once(" into ") {
        Some((source, target)) => {
            if current.is_some_and(|current| current != target) {
                return None;
            }
            (source, target)
        }
        None => (title, current?),
    };
    if !valid_name(target) {
        return None;
    }
    let mut groups = source.split_once(" of ").map_or(source, |(names, _)| names);
    let mut sources = Vec::new();
    let mut source_refs = Vec::new();
    while !groups.is_empty() {
        let matched = SOURCE_GROUP.captures(groups)?;
        let kind = matched.get(1)?.as_str();
        let raw = matched.get(2)?.as_str();
        for quoted in raw.split(", ").flat_map(|names| names.split(" and ")) {
            let name = quoted.strip_prefix('\'')?.strip_suffix('\'')?;
            if !valid_name(name) {
                return None;
            }
            let remote = kind.starts_with("remote-tracking");
            sources.push(branch_name(name, remote));
            let qualified = if name.starts_with("refs/") || kind.starts_with("commit") {
                name.to_owned()
            } else if remote {
                format!("refs/remotes/{name}")
            } else if kind.starts_with("tag") {
                format!("refs/tags/{name}")
            } else {
                format!("refs/heads/{name}")
            };
            source_refs.push(qualified);
        }
        let rest = groups.get(matched.get(0)?.end()..)?;
        groups = if rest.is_empty() {
            ""
        } else {
            rest.strip_prefix(", ")?
        };
    }
    if sources.len() != n_heads {
        return None;
    }
    let labels: Vec<_> = sources
        .iter()
        .enumerate()
        .map(|(i, &name)| {
            if name == target || sources.iter().filter(|&&other| other == name).count() > 1 {
                if source.contains(" of ") {
                    return format!("fetched {name}");
                }
                if let Some(remote) = source_refs[i].strip_prefix("refs/remotes/") {
                    return remote.to_owned();
                }
            }
            name.to_owned()
        })
        .collect();
    Some(Direction {
        sources: labels.join(", "),
        target: branch_name(target, false).to_owned(),
        source_refs,
        fetched: source.contains(" of "),
    })
}

impl Direction {
    // Pull formats the heading from FETCH_HEAD. A local branch of the same name
    // often points to the receiving side. Compare the fetched names and IDs.
    fn matches_heads(&self, heads: &[&str]) -> bool {
        let mut unmatched = heads.to_vec();
        let fetched = if self.fetched {
            git(&["rev-parse", "--git-path", "FETCH_HEAD"])
                .ok()
                .and_then(|file| fs::read_to_string(file.trim_end()).ok())
        } else {
            None
        };
        for reference in &self.source_refs {
            let id = if self.fetched {
                let name = reference.strip_prefix("refs/heads/").unwrap_or(reference);
                let expected = format!("branch '{name}' of ");
                fetched
                    .as_deref()
                    .unwrap_or("")
                    .lines()
                    .filter_map(|line| line.split_once("\t\t"))
                    .find(|(id, desc)| unmatched.contains(id) && desc.starts_with(&expected))
                    .map(|(id, _)| id.to_owned())
            } else {
                git(&["rev-parse", "--verify", "--end-of-options", reference])
                    .ok()
                    .map(|id| id.trim_end().to_owned())
            };
            let Some(index) = unmatched
                .iter()
                .position(|&head| Some(head) == id.as_deref())
            else {
                return false;
            };
            unmatched.swap_remove(index);
        }
        unmatched.is_empty()
    }
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && !name
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || "~^:?*[\\".contains(c))
}

struct Arrival<'a> {
    short: &'a str,
    parents: &'a str,
    subject: &'a str,
    body: &'a str,
}

impl Arrival<'_> {
    fn human_title(&self, vocab: &Vocabulary) -> String {
        let title = caption::without_inventory(self.subject, vocab);
        if let Some((number, _)) = self
            .subject
            .strip_prefix("Merge pull request #")
            .and_then(|rest| rest.split_once(" from "))
            && number.parse::<u64>().is_ok_and(|n| n > 0)
            && let Some(line) = self.body.lines().find(|line| !line.trim().is_empty())
        {
            let human = caption::without_inventory(line, vocab);
            let suffix = format!("(#{number})");
            return if human.ends_with(&suffix) {
                human.to_owned()
            } else {
                format!("{human} {suffix}")
            };
        }
        if self.parents.split_whitespace().count() <= 1
            && let Some((heading, summary)) = title.split_once("): ")
            && heading.contains(" (")
            && let Some(cluster) = heading.split_whitespace().next()
            && vocab.tokens(cluster).is_ok()
            && !summary.trim().is_empty()
        {
            return summary.to_owned();
        }
        title.to_owned()
    }

    fn is_pr(&self, vocab: &Vocabulary) -> bool {
        self.parents.split_whitespace().count() > 1
            && crate::history::is_pr_merge(self.subject, self.body, vocab)
    }
}

/// Inventory walks all newly reachable authored commits. The human narrative
/// follows just the arriving first-parent lines, once across all incoming tips.
fn render(direction: &Direction, base: &str, heads: &[&str], vocab: &Vocabulary) -> Result<String> {
    let incoming = incoming_commits(base, heads)?;
    let excluded = format!("^{base}");
    let mut args = vec![
        "log",
        "--first-parent",
        "--topo-order",
        "--no-patch",
        "--no-color",
        "--no-notes",
        "--no-show-signature",
        "--encoding=UTF-8",
        "--format=%h%x00%P%x00%s%x00%b",
        "-z",
        "--end-of-options",
        &excluded,
    ];
    args.extend(heads);
    args.push("--");
    let log = git(&args)?;
    let mut fields = log.split_terminator('\0');
    let mut rows = Vec::new();
    while let Some(short) = fields.next() {
        rows.push(Arrival {
            short,
            parents: fields.next().context("missing incoming parents")?,
            subject: fields.next().context("missing incoming subject")?,
            body: fields.next().context("missing incoming body")?,
        });
    }
    let summary = match rows.as_slice() {
        [one] => one.human_title(vocab),
        [first, second, rest @ ..] if rows.iter().all(|row| row.is_pr(vocab)) => {
            let titles = format!(
                "{}; {}",
                first.human_title(vocab),
                second.human_title(vocab)
            );
            if rest.is_empty() {
                titles
            } else {
                format!("{titles}; and {} more", rest.len())
            }
        }
        _ => format!("{} commits", rows.len()),
    };
    let mut message = String::new();
    if let Some(prefix) = caption::merge_inventory(&incoming.subjects, vocab) {
        message.push_str(&prefix);
        message.push(' ');
    }
    message.push_str(&format!(
        "{} into {}: {}\n\nArrived from {} ({}):",
        direction.sources,
        direction.target,
        summary,
        direction.sources,
        rows.len()
    ));
    for row in rows.iter().take(BODY_LIMIT) {
        message.push_str(&format!("\n- {} {}", row.short, row.subject));
    }
    if rows.len() > BODY_LIMIT {
        message.push_str(&format!("\n… and {} more", rows.len() - BODY_LIMIT));
    }
    Ok(message)
}

/// Move Git's conflict list above its comment/scissors area. Git writes it with
/// its configured comment prefix (and may be using an automatically chosen one).
fn conflict_list<'a>(
    text: &'a str,
    configured: &str,
) -> Option<(std::ops::Range<usize>, Vec<&'a str>)> {
    let mut lines = text.split_inclusive('\n');
    let mut start = 0;
    while let Some(line) = lines.next() {
        let end = start + line.len();
        if let Some(prefix) = line
            .trim_end_matches(['\r', '\n'])
            .strip_suffix(" Conflicts:")
        {
            let matches = match configured {
                "" => prefix == "#",
                "auto" => prefix.len() == 1,
                _ => configured == prefix,
            };
            if matches {
                let entry = format!("{prefix}\t");
                let mut after = end;
                let paths: Vec<_> = lines
                    .by_ref()
                    .map_while(|row| {
                        let path = row.strip_prefix(&entry)?.trim_end_matches(['\r', '\n']);
                        after += row.len();
                        Some(path)
                    })
                    .collect();
                if !paths.is_empty() {
                    return Some((start..after, paths));
                }
                return None;
            }
        }
        start = end;
    }
    None
}

pub(super) fn prepare(file: &Path, vocab: &Vocabulary) -> Result<()> {
    let original = fs::read(file)?;
    let Ok(original) = std::str::from_utf8(&original) else {
        return Ok(());
    };
    let (subject, body) = original.split_once('\n').unwrap_or((original, ""));
    if !subject.starts_with("Merge ") {
        return Ok(());
    }
    let dir = file.parent().context("missing Git message directory")?;
    let head_file = fs::read_to_string(dir.join("MERGE_HEAD"))?;
    let heads = head_file.lines().collect::<Vec<_>>();
    let target = git(&["symbolic-ref", "--quiet", "HEAD"]).ok();
    let current = target
        .as_deref()
        .and_then(|name| name.trim_end().strip_prefix("refs/heads/"));
    let Some(mut flow) = direction(subject, current, heads.len()) else {
        return Ok(());
    };
    if !flow.matches_heads(&heads) {
        return Ok(());
    }
    let comment = git(&["config", "--get", "core.commentString"])
        .or_else(|_| git(&["config", "--get", "core.commentChar"]))
        .unwrap_or_default();
    let configured = comment.trim_end();
    let comment = match configured {
        "" | "auto" => "#",
        other => other,
    };
    // A branch may start with the comment character, e.g. #42. Quote its
    // display label in both the route and the provenance paragraph.
    if flow.sources.starts_with(comment) {
        flow.sources = format!("'{}'", flow.sources);
    }
    let base = git(&["rev-parse", "--verify", "HEAD"])?;
    let mut rewritten = render(&flow, base.trim(), &heads, vocab)?;
    if rewritten.lines().any(|line| line.starts_with(comment)) {
        eprintln!(
            "gitmoji: Git's comment prefix would remove part of the branch caption; keeping the original merge message"
        );
        return Ok(());
    }
    let mut original_body = body.to_owned();
    if let Some((block, conflicts)) = conflict_list(body, configured)
        && !"Conflicted paths:".starts_with(comment)
    {
        // With --no-edit Git may keep comment lines; move the block instead of
        // copying it twice. Leave its surrounding editor notes and Git logs alone.
        original_body.replace_range(block, "");
        rewritten.push_str("\n\nConflicted paths:");
        for path in conflicts {
            rewritten.push_str(&format!("\n- {path}"));
        }
    }
    if !original_body.trim().is_empty() {
        rewritten.push_str("\n\n");
        rewritten.push_str(original_body.trim_start_matches(['\r', '\n']));
    }
    if !rewritten.ends_with('\n') {
        rewritten.push('\n');
    }
    fs::write(file, rewritten).with_context(|| format!("cannot write {}", file.display()))
}

/// A historical object has no current branch. Only its recorded default subject
/// can supply direction. The caller owns preservation of its original message.
pub(super) fn historical(revision: &str, vocab: &Vocabulary) -> Result<Option<String>> {
    let revision = git(&[
        "rev-parse",
        "--verify",
        "--end-of-options",
        &format!("{revision}^{{commit}}"),
    ])?;
    let record = git(&[
        "show",
        "-s",
        "--no-notes",
        "--no-show-signature",
        "--format=%P%x00%s",
        revision.trim(),
        "--",
    ])?;
    let (parents, subject) = record
        .split_once('\0')
        .context("missing stored merge subject")?;
    let parents = parents.split_whitespace().collect::<Vec<_>>();
    let [base, heads @ ..] = parents.as_slice() else {
        return Ok(None);
    };
    if heads.is_empty() {
        return Ok(None);
    }
    let Some(flow) = direction(subject.trim_end_matches('\n'), None, heads.len()) else {
        return Ok(None);
    };
    Ok(Some(render(&flow, base, heads, vocab)?))
}
