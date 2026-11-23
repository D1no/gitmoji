//! Track authored provisional and exploration work through the physical Git graph.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::process::ExitCode;
use std::rc::Rc;

use anyhow::{Context, Result};
use gitmoji_triangle::config::Config;
use gitmoji_triangle::subject::{self, Subject, Ticket};
use gitmoji_triangle::vocabulary::Vocabulary;
use serde::Serialize;

use crate::history::git;

#[derive(Serialize, PartialOrd, Ord, PartialEq, Eq)]
pub(super) struct Opening {
    modifier: String,
    commit: String,
}

#[derive(Serialize)]
pub(super) struct OpenTicket {
    pub(super) ticket: String,
    openings: BTreeSet<Opening>,
}

impl OpenTicket {
    pub(super) fn modifiers(&self) -> Vec<&str> {
        self.openings
            .iter()
            .map(|o| o.modifier.as_str())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
}

/// Visit children before parents. A closer covers each of its ancestors, including
/// a shared ancestor on multiple branches, but never an independent sibling.
pub(super) fn tickets(span: &str, config: &Config, vocab: &Vocabulary) -> Result<Vec<OpenTicket>> {
    let log = git(&[
        "log",
        "--topo-order",
        "--no-patch",
        "--no-notes",
        "--no-color",
        "--no-show-signature",
        "--encoding=UTF-8",
        "-z",
        "--format=%H%x00%P%x00%s",
        span,
        "--",
    ])?;
    let mut fields = log.split_terminator('\0');
    let mut history_config = config.clone();
    // A scope renamed since the work began must not hide the ticket's opening.
    history_config.scopes_allowed.clear();
    history_config.lone_second = true;
    let mut pending: HashMap<&str, Rc<HashSet<String>>> = HashMap::new();
    let mut result: BTreeMap<String, BTreeSet<Opening>> = BTreeMap::new();
    let empty = Rc::new(HashSet::new());
    while let Some(hash) = fields.next() {
        let parents = fields
            .next()
            .context("missing parents in provisional history")?;
        let subject = fields
            .next()
            .context("missing subject in provisional history")?;
        let mut closed = pending.remove(hash).unwrap_or_else(|| Rc::clone(&empty));
        if !parents.contains(' ')
            && let Ok(Subject::Tagged {
                tags,
                ticket: Ticket::Id(ticket),
                ..
            }) = subject::parse(vocab, &history_config, subject)
        {
            if tags.second.1.closes {
                Rc::make_mut(&mut closed).insert(ticket);
            } else if let Some(marker) = tags.modifier
                && matches!(marker.emoji.as_str(), "🚧" | "🧪")
                && !closed.contains(&ticket)
            {
                result.entry(ticket).or_default().insert(Opening {
                    modifier: marker.emoji.clone(),
                    commit: hash.to_owned(),
                });
            }
        }
        if !closed.is_empty() {
            for parent in parents.split_ascii_whitespace() {
                pending
                    .entry(parent)
                    .and_modify(|existing| {
                        if !Rc::ptr_eq(existing, &closed) {
                            Rc::make_mut(existing).extend(closed.iter().cloned());
                        }
                    })
                    .or_insert_with(|| Rc::clone(&closed));
            }
        }
    }
    Ok(result
        .into_iter()
        .map(|(ticket, openings)| OpenTicket { ticket, openings })
        .collect())
}

pub(super) fn run(
    range: &str,
    json: bool,
    fail_if_any: bool,
    config: &Config,
    vocab: &Vocabulary,
) -> Result<ExitCode> {
    let (base, tip) = crate::changelog::endpoints(range)?;
    let span = base.map_or_else(|| tip.clone(), |b| format!("{b}..{tip}"));
    let rows = tickets(&span, config, vocab)?;
    if json {
        println!("{}", serde_json::to_string(&rows)?);
    } else {
        for row in &rows {
            println!("{} {}", row.modifiers().join(" "), row.ticket);
        }
    }
    Ok(if fail_if_any && !rows.is_empty() {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}
