//! Draft release notes from landed pull requests and their authored Git history.

use std::collections::{BTreeSet, HashMap, HashSet};

use anyhow::{Context, Result, bail};
use gitmoji_triangle::caption;
use gitmoji_triangle::config::{Audience, Config};
use gitmoji_triangle::vocabulary::{Slot, Vocabulary};

use crate::history::{dependabot_message, git, pr_landing};

const OTHER: &str = "Other changes";

fn default_groups() -> Vec<Audience> {
    [
        ("People-facing", &["🎨"][..]),
        ("System", &["🧩", "🛰️", "🗃️", "🧠"]),
        ("Behind the scenes", &["🧱", "👷", "🧑‍💻", "🤖"]),
    ]
    .into_iter()
    .map(|(name, areas)| Audience {
        name: name.to_owned(),
        areas: areas.iter().map(|area| (*area).to_owned()).collect(),
    })
    .collect()
}

fn area_groups<'a>(groups: &[Audience], vocab: &'a Vocabulary) -> Result<HashMap<&'a str, usize>> {
    let mut names = HashSet::new();
    let mut areas = HashMap::new();
    for (rank, group) in groups.iter().enumerate() {
        let name = &group.name;
        if name.trim().is_empty()
            || name.trim() != name
            || name.chars().any(char::is_control)
            || name == OTHER
            || !names.insert(name)
        {
            bail!("expected a unique, single-line changelog group name, found `{name}`");
        }
        if group.areas.is_empty() {
            bail!("changelog group `{name}` needs at least one area");
        }
        for symbol in &group.areas {
            let Some((slot, entry)) = vocab.find(symbol) else {
                bail!("unknown changelog area `{symbol}` in `{name}`");
            };
            if slot.slot != Slot::Area {
                bail!("changelog group `{name}` needs an area, found `{symbol}`");
            }
            if areas.insert(entry.emoji.as_str(), rank).is_some() {
                bail!("changelog area `{symbol}` belongs to more than one group");
            }
        }
    }
    Ok(areas)
}

fn endpoints(range: &str) -> Result<(Option<String>, String)> {
    let (base, tip) = match range.split_once("..") {
        Some((base, tip))
            if base.is_empty() || tip.is_empty() || tip.starts_with('.') || tip.contains("..") =>
        {
            bail!("expected FROM..TO with two explicit revisions, or a single TO revision")
        }
        Some((base, tip)) => (Some(base), tip),
        None => (None, range),
    };
    if git(&["rev-parse", "--is-shallow-repository"])?.trim() == "true" {
        bail!("release notes need complete history; fetch the full history and tags, then retry");
    }
    let resolve = |revision: &str| -> Result<String> {
        Ok(git(&[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{revision}^{{commit}}"),
        ])
        .with_context(|| format!("cannot resolve changelog revision `{revision}`"))?
        .trim_end()
        .to_owned())
    };
    Ok((base.map(resolve).transpose()?, resolve(tip)?))
}

fn arriving_log(parents: &str, base: Option<&str>, format: &str) -> Result<String> {
    let mut parents = parents.split_ascii_whitespace();
    let target = parents
        .next()
        .context("missing first parent of a PR merge")?;
    let excluded_target = format!("^{target}");
    let excluded_base = base.map(|id| format!("^{id}"));
    let mut args = vec![
        "log",
        "--topo-order",
        "--reverse",
        "--no-merges",
        "--no-patch",
        "--no-color",
        "--no-notes",
        "--no-show-signature",
        "--encoding=UTF-8",
        "-z",
        format,
        &excluded_target,
    ];
    args.extend(excluded_base.as_deref());
    args.extend(parents);
    args.push("--");
    git(&args)
}

/// Exclude decisions shipped before this release and decisions already
/// attributed to a nested PR in the draft.
fn authored(
    parents: &str,
    base: Option<&str>,
    claimed: &mut HashSet<String>,
    vocab: &Vocabulary,
) -> Result<Option<Vec<String>>> {
    let log = arriving_log(parents, base, "--format=%H%x00%an <%ae>%x00%s")?;
    if log.is_empty() && base.is_some() {
        // The earlier tag can point at the PR's own head. The landing still
        // belongs to this range, but a release-only landing must stay omitted.
        let original = arriving_log(parents, None, "--format=%s")?;
        if !original.is_empty()
            && original
                .split_terminator('\0')
                .all(|s| caption::heading(s, vocab).is_some_and(|h| h.activity == "🔖"))
        {
            return Ok(None);
        }
    }
    let mut fields = log.split_terminator('\0');
    let mut subjects = Vec::new();
    while let Some(hash) = fields.next() {
        let author = fields.next().context("missing author of arriving commit")?;
        let subject = fields
            .next()
            .context("missing subject of arriving commit")?;
        if claimed.insert(hash.to_owned()) {
            subjects.push(dependabot_message(vocab, author, subject).into_owned());
        }
    }
    Ok(Some(subjects))
}

// A PR title on GitHub is plain text. Keep links, images and HTML in that
// text from becoming markup when the draft is used for a GitHub release.
fn markdown_text(text: &str) -> String {
    let mut escaped = String::new();
    for ch in text.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '\\' | '[' | ']' | '*' | '_' | '`' | '~' => {
                escaped.push('\\');
                escaped.push(ch);
            }
            ch if ch.is_control() || matches!(ch, '\u{2028}' | '\u{2029}') => escaped.push(' '),
            ch => escaped.push(ch),
        }
    }
    escaped
}

struct Entry {
    group: usize,
    line: String,
}

fn summarize(
    incoming: &[String],
    number: &str,
    title: &str,
    groups: &HashMap<&str, usize>,
    fallback: usize,
    config: &Config,
    vocab: &Vocabulary,
) -> Option<Entry> {
    let mut rank = fallback;
    let mut tickets = BTreeSet::new();
    let mut areas = Vec::new();
    let mut breaking = false;
    let mut release_only = !incoming.is_empty();
    for subject in incoming {
        let Some(heading) = caption::heading(subject, vocab) else {
            release_only = false;
            continue;
        };
        if heading.activity == "🔖" {
            continue;
        }
        release_only = false;
        breaking |= heading.modifier == Some("💥");
        if let Some(area) = heading.area {
            if !areas.contains(&area) {
                areas.push(area);
            }
            rank = rank.min(*groups.get(area).unwrap_or(&fallback));
        }
        if let Some(ticket) = subject.split_whitespace().nth(1)
            && config.is_ticket(ticket)
        {
            tickets.insert(ticket);
        }
    }
    if release_only {
        return None;
    }
    let mut references = format!("#{number}");
    tickets.remove(references.as_str());
    if !tickets.is_empty() {
        references.push_str("; ");
        references.push_str(
            &tickets
                .into_iter()
                .map(markdown_text)
                .collect::<Vec<_>>()
                .join(", "),
        );
    }
    let also = areas
        .into_iter()
        .filter(|area| groups.get(area).copied().unwrap_or(fallback) != rank)
        .collect::<Vec<_>>();
    let title = markdown_text(title);
    let mut line = format!(
        "- {}{title} ({references})",
        if breaking {
            "**💥 Breaking** — "
        } else {
            ""
        }
    );
    if !also.is_empty() {
        line.push_str(&format!(" — also: {}", also.join(" ")));
    }
    Some(Entry { group: rank, line })
}

/// A squash or rebase can have the same graph shape as a direct commit. Count
/// the single-parent objects on the release line without guessing PR numbers.
fn unlisted_first_parent(
    span: &str,
    claimed: &HashSet<String>,
    vocab: &Vocabulary,
) -> Result<usize> {
    let log = git(&[
        "log",
        "--first-parent",
        "--min-parents=1",
        "--max-parents=1",
        "--no-patch",
        "--no-notes",
        "--no-color",
        "--no-show-signature",
        "--encoding=UTF-8",
        "-z",
        "--format=%H%x00%s",
        span,
        "--",
    ])?;
    let mut fields = log.split_terminator('\0');
    let mut count = 0;
    while let Some(hash) = fields.next() {
        let subject = fields
            .next()
            .context("missing first-parent commit subject")?;
        let marker =
            caption::heading(subject, vocab).is_some_and(|h| matches!(h.activity, "🔖" | "🔺"));
        if !marker && !claimed.contains(hash) {
            count += 1;
        }
    }
    Ok(count)
}

/// Return a release-note draft in stable section order, oldest landings first.
pub(super) fn render(range: &str, config: &Config, vocab: &Vocabulary) -> Result<String> {
    let groups = config
        .changelog_groups
        .clone()
        .unwrap_or_else(default_groups);
    let areas = area_groups(&groups, vocab)?;
    let (base, tip) = endpoints(range)?;
    if base.as_ref() == Some(&tip) {
        eprintln!("gitmoji changelog: empty range; both endpoints are the same commit.");
        return Ok(String::new());
    }
    let span = base
        .as_ref()
        .map_or_else(|| tip.clone(), |base| format!("{base}..{tip}"));
    // Every unequal TO contributes at least its tip unless FROM already reaches it.
    if base.is_some() && git(&["rev-list", "-1", &span, "--"])?.trim().is_empty() {
        bail!("reversed release range: TO is an ancestor of FROM; swap the endpoints");
    }

    let log = git(&[
        "log",
        "--topo-order",
        "--reverse",
        "--merges",
        "--no-patch",
        "--no-color",
        "--no-notes",
        "--no-show-signature",
        "--encoding=UTF-8",
        "-z",
        "--format=%P%x00%s%x00%b",
        &span,
        "--",
    ])?;
    let mut sections = vec![Vec::new(); groups.len() + 1];
    let mut seen = HashSet::new();
    let mut claimed = HashSet::new();
    let mut fields = log.split_terminator('\0');
    while let Some(parents) = fields.next() {
        let subject = fields.next().context("missing subject in Git log")?;
        let body = fields.next().context("missing body in Git log")?;
        let Some((number, title)) = pr_landing(subject, body, vocab) else {
            continue;
        };
        if seen.insert(number)
            && let Some(subjects) = authored(parents, base.as_deref(), &mut claimed, vocab)?
            && let Some(entry) = summarize(
                &subjects,
                number,
                title,
                &areas,
                groups.len(),
                config,
                vocab,
            )
        {
            sections[entry.group].push(entry.line);
        }
    }
    let mut markdown = String::new();
    for (name, lines) in groups
        .iter()
        .map(|g| g.name.as_str())
        .chain([OTHER])
        .zip(sections)
    {
        if !lines.is_empty() {
            if !markdown.is_empty() {
                markdown.push('\n');
            }
            markdown.push_str(&format!(
                "## {}\n\n{}\n",
                markdown_text(name),
                lines.join("\n")
            ));
        }
    }
    let omitted = unlisted_first_parent(&span, &claimed, vocab)?;
    if markdown.is_empty() {
        eprintln!("gitmoji changelog: no qualifying pull request landings in this range.");
    }
    if omitted > 0 {
        let noun = if omitted == 1 { "commit" } else { "commits" };
        eprintln!(
            "gitmoji changelog: {omitted} single-parent {noun} on the first-parent history outside the draft. Review direct, squash and rebased changes for release notes."
        );
    }
    Ok(markdown)
}
