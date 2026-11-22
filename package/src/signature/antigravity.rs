use super::*;
use rusqlite::{Connection, OpenFlags};

static CHILD: LazyLock<Regex> = LazyLock::new(|| {
    regex(
        r#"(?m)^Created At: [^\n]+\nCompleted At: [^\n]+\nCreated the following subagent:\n\{\s*\n\s*"conversationId"\s*:\s*"([0-9a-fA-F-]+)""#,
    )
});
static COMMIT: LazyLock<Regex> =
    LazyLock::new(|| regex(r"(?:^|[;&|\n])\s*git(?:\s+-C\s+\S+)?\s+commit(?:\s|$)"));

fn transcript(root: &Path, id: &str) -> PathBuf {
    let dir = root.join("brain").join(id).join(".system_generated/logs");
    let full = dir.join("transcript_full.jsonl");
    if full.is_file() {
        full
    } else {
        dir.join("transcript.jsonl")
    }
}

fn model_from(raw: &str) -> Model {
    if let Some((name, effort)) = raw.rsplit_once('-')
        && !name.is_empty()
        && matches!(effort, "minimal" | "low" | "medium" | "high")
    {
        return Model::new(name, effort);
    }
    Model::new(raw, "")
}

fn descendants(root: &Path, session: &str) -> rusqlite::Result<BTreeSet<String>> {
    // UNION also terminates a malformed cycle. Read only the requested lineage.
    let conn = Connection::open_with_flags(
        root.join("conversation_summaries.db"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let mut query = conn.prepare(
        "WITH RECURSIVE children(id) AS (
           SELECT conversation_id FROM conversation_summaries WHERE parent_conversation_id = ?1
           UNION
           SELECT c.conversation_id FROM conversation_summaries c JOIN children ON c.parent_conversation_id = children.id
         ) SELECT id FROM children WHERE id != ?1 AND typeof(id) = 'text'")?;
    query
        .query_map([session], |row| row.get::<_, String>(0))?
        .filter(|item| item.as_ref().is_ok_and(|id| !session_id(id).is_empty()))
        .collect()
}

fn calls(row: &Value) -> impl Iterator<Item = (&str, &Value)> {
    array(&row["tool_calls"])
        .iter()
        .filter(move |_| row["type"] == "PLANNER_RESPONSE")
        .map(|c| (string(&c["name"]), &c["args"]))
}

fn record(tool: &str, args: &Value, research: &mut Research) {
    match tool {
        "search_web" => research.searches += 1,
        "read_url_content" | "web_fetch" | "fetch_url" => research.pages += 1,
        "run_command" => {
            let cmd = string(&args["CommandLine"]);
            if cmd.contains("ctx7") && cmd.contains(" docs ") {
                research.docs += 1;
            }
        }
        _ => {
            let lower = tool.to_ascii_lowercase();
            if lower.contains("docs") || lower.contains("context7") {
                research.docs += 1;
            }
        }
    }
}

pub(super) fn read() -> Signature {
    let id = env("ANTIGRAVITY_CONVERSATION_ID");
    let id = session_id(&id);
    let mut sig = Signature::new("Antigravity", id);
    let Some(root) = std::env::var_os("ANTIGRAVITY_APP_DATA_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
    else {
        return sig;
    };
    if id.is_empty() {
        return sig;
    }

    let log = transcript(&root, id);
    let mut children = descendants(&root, id).unwrap_or_default();
    let mut receipts = Vec::new();
    let mut pending_commit = false;
    for row in rows(&log) {
        match string(&row["type"]) {
            "PLANNER_RESPONSE" => {
                if let Some(raw) = field(string(&row["model"])) {
                    sig.model = model_from(raw);
                }
                pending_commit = calls(&row).any(|(tool, args)| {
                    tool == "run_command" && COMMIT.is_match(string(&args["CommandLine"]))
                });
            }
            "GENERIC" => {
                let content = string(&row["content"]);
                if content.starts_with("Created At: ")
                    && let Some(found) = CHILD.captures(content)
                    && &found[1] != id
                {
                    children.insert(found[1].to_owned());
                }
                if pending_commit {
                    let time = timestamp(string(&row["created_at"]));
                    receipts.extend(
                        RECEIPT
                            .captures_iter(content)
                            .map(|found| (found[1].to_owned(), time)),
                    );
                }
            }
            _ => {}
        }
    }
    let receipt = receipts
        .iter()
        .rev()
        .map(|(sha, time)| receipt_cutoff(sha, *time))
        .find(|ts| *ts != 0)
        .unwrap_or_default();
    let since = last_signed("Antigravity", id).max(receipt);
    let mut peer_ids = BTreeSet::new();
    for row in rows(&log).filter(|r| since == 0 || timestamp(string(&r["created_at"])) > since) {
        for (tool, args) in calls(&row) {
            if tool == "send_message" {
                let peer = string(&args["Recipient"]).trim().trim_matches('"');
                if !children.contains(peer) {
                    add_peer(&mut peer_ids, peer);
                }
            } else if tool == "run_command" {
                agmsg_peers(string(&args["CommandLine"]), &mut sig.team);
            }
            record(tool, args, &mut sig.research);
        }
    }
    let mut peer_names = BTreeMap::new();
    for row in rows(&root.join("history.jsonl")) {
        if row["type"] != "slash_command" {
            continue;
        }
        let Some(name) = string(&row["display"])
            .strip_prefix("/rename ")
            .and_then(field)
        else {
            continue;
        };
        let owner = string(&row["conversationId"]);
        if owner == id {
            sig.name = name.to_owned();
        }
        if peer_ids.contains(owner) {
            peer_names.insert(owner.to_owned(), name.to_owned());
        }
    }
    for peer in peer_ids {
        sig.team.insert(peer_names.remove(&peer).unwrap_or(peer));
    }
    for child in children {
        let mut model = Model::new("", "");
        let mut active = false;
        for row in rows(&transcript(&root, &child)) {
            if row["type"] == "PLANNER_RESPONSE"
                && let Some(raw) = field(string(&row["model"]))
            {
                model = model_from(raw);
            }
            if since == 0 || timestamp(string(&row["created_at"])) > since {
                active = true;
                for (tool, args) in calls(&row) {
                    record(tool, args, &mut sig.research);
                }
            }
        }
        if active {
            *sig.helpers.entry(model).or_default() += 1;
        }
    }
    sig
}
