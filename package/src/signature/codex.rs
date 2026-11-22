use super::*;
use percent_encoding::percent_decode_str;

static RENAME: LazyLock<Regex> = LazyLock::new(|| {
    regex(
        r"(?i)^(?:/rename\s+|(?:alright[, ]+|okay[, ]+)?(?:I (?:have )?renamed you to |your name is |call yourself ))([\w][\w '\-]*?)(?:[.!](?:\s.*)?|$)$",
    )
});
static COMMIT: LazyLock<Regex> = LazyLock::new(|| regex(r"(?:^|[;&|\n])\s*git\s+commit(?:\s|$)"));
static SEND: LazyLock<Regex> = LazyLock::new(|| {
    regex(
        r#"(?:^|[;&|\n])\s*(?:do\s+)?(?:bash\s+)?["']?(?:[^\s"']*/)?(?:send\.sh["']?\s|agmsg["']?\s+send\s)"#,
    )
});
static SENT: LazyLock<Regex> = LazyLock::new(|| regex(r"(?m)^Sent to ([\w./-]+) in team [\w.-]+$"));
static WEB_PAGE: LazyLock<Regex> = LazyLock::new(|| regex(r"^turn\d+(?:view|fetch)\d+$"));

fn rollouts(home: &Path, thread: &str) -> (Option<PathBuf>, BTreeMap<String, PathBuf>) {
    let mut own = None;
    let mut children = BTreeMap::new();
    for folder in ["archived_sessions", "sessions"] {
        let mut dirs = vec![home.join(folder)];
        let mut files = Vec::new();
        while let Some(dir) = dirs.pop() {
            for path in entries(&dir) {
                let Ok(ty) = fs::symlink_metadata(&path).map(|m| m.file_type()) else {
                    continue;
                };
                if ty.is_dir() {
                    dirs.push(path);
                } else if ty.is_file()
                    && filename(&path).starts_with("rollout-")
                    && filename(&path).ends_with(".jsonl")
                {
                    files.push(path);
                }
            }
        }
        files.sort();
        for path in files {
            // A history fork can contain later headers copied from its parent.
            // Only the first rollout header records which thread owns this file.
            let Some(row) = rows(&path).find(|r| r["payload"].is_object()) else {
                continue;
            };
            if row["type"] != "session_meta" {
                continue;
            }
            let p = &row["payload"];
            if p["id"] == thread {
                own = Some(path);
            } else if p
                .pointer("/source/subagent/thread_spawn/parent_thread_id")
                .and_then(Value::as_str)
                == Some(thread)
                && let Some(id) = field(string(&p["id"]))
            {
                children.insert(id.to_owned(), path);
            }
        }
    }
    (own, children)
}

fn context7(command: &str) -> bool {
    let Some(args) = shlex::split(command) else {
        return false;
    };
    let mut args = args.as_slice();
    if args
        .first()
        .is_some_and(|w| filename(Path::new(w)) == "npx")
    {
        args = &args[1..];
        if args
            .first()
            .is_some_and(|w| matches!(w.as_str(), "-y" | "--yes"))
        {
            args = &args[1..];
        }
    }
    args.len() >= 3
        && filename(Path::new(&args[0])).split('@').next() == Some("ctx7")
        && args[1] == "docs"
}

fn command_activity(item: &Value, peers: Option<&mut BTreeSet<String>>, research: &mut Research) {
    let mut commands = array(&item["parsed_cmd"])
        .iter()
        .filter_map(|entry| entry["cmd"].as_str())
        .filter(|cmd| !cmd.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if commands.is_empty() {
        let argv = array(&item["command"])
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>();
        if argv.iter().any(|w| matches!(*w, "-c" | "-lc")) {
            if let Some(last) = argv.last() {
                commands.push((*last).to_owned());
            }
        } else if let Ok(raw) = shlex::try_join(argv) {
            commands.push(raw);
        }
    }
    if let Some(peers) = peers
        && commands
            .iter()
            .any(|cmd| SEND.is_match(cmd.split("<<").next().unwrap_or_default()))
    {
        for m in SENT.captures_iter(string(&item["aggregated_output"])) {
            add_peer(peers, &m[1]);
        }
    }
    if item["exit_code"] == 0 {
        research.docs += commands.iter().filter(|cmd| context7(cmd)).count();
    }
}

fn activity(item: &Value, peers: Option<&mut BTreeSet<String>>, research: &mut Research) {
    match string(&item["type"]) {
        "SubAgentActivity" if item["kind"] == "interacted" => {
            if let Some(peers) = peers {
                let recipient =
                    field(string(&item["agent_path"])).unwrap_or(string(&item["agent_thread_id"]));
                add_peer(peers, recipient);
            }
        }
        "CollabAgentToolCall"
            if item["status"] == "completed"
                && matches!(string(&item["tool"]), "sendInput" | "send_input") =>
        {
            if let Some(peers) = peers {
                for target in array(&item["receiver_thread_ids"]) {
                    add_peer(peers, string(target));
                }
            }
        }
        "CommandExecution" => command_activity(item, peers, research),
        "WebSearch" | "Extension"
            if item["type"] == "WebSearch" || item["kind"] == "web.search" =>
        {
            let action = &item["action"];
            if action["type"] == "search" {
                research.searches += array(&action["queries"]).len().max(1);
            }
            let pages = array(&item["results"])
                .iter()
                .map(|r| string(&r["ref_id"]))
                .filter(|r| WEB_PAGE.is_match(r))
                .collect::<BTreeSet<_>>();
            research.pages += if pages.is_empty()
                && item.get("results").is_none()
                && matches!(string(&action["type"]), "openPage" | "findInPage")
            {
                1
            } else {
                pages.len()
            };
        }
        "McpToolCall" if item["status"] == "completed" => {
            let tool = string(&item["tool"]).to_ascii_lowercase().replace('-', "_");
            if matches!(
                tool.as_str(),
                "query_docs" | "get_library_docs" | "fetch_openai_doc" | "search_openai_docs"
            ) {
                research.docs += 1;
            }
            if let Some(peers) = peers
                && item["server"] == "codex_tui"
                && tool == "send_message_to_thread"
            {
                add_peer(peers, string(&item["arguments"]["threadId"]));
            }
        }
        _ => {}
    }
}

fn completed_for(p: &Value, row: &Value, thread: &str) -> bool {
    row["type"] == "event_msg" && p["type"] == "item_completed" && p["thread_id"] == thread
}

fn thread_activity(
    path: &Path,
    thread: &str,
    since: i64,
    mut peers: Option<&mut BTreeSet<String>>,
    research: &mut Research,
) -> Option<Model> {
    let mut turns = BTreeMap::new();
    let mut counted = BTreeSet::new();
    let mut active_turn = None;
    for row in rows(path) {
        let p = &row["payload"];
        let turn = string(&p["turn_id"]);
        if row["type"] == "turn_context" && !turn.is_empty() {
            turns.insert(
                turn.to_owned(),
                Model::new(string(&p["model"]), string(&p["effort"])),
            );
        }
        if completed_for(p, &row, thread)
            && p["item"]["type"] != "UserMessage"
            && timestamp(string(&row["timestamp"])) > since
        {
            active_turn = Some(turn.to_owned());
            let item = &p["item"];
            let id = string(&item["id"]);
            if id.is_empty() || counted.insert(id.to_owned()) {
                activity(item, peers.as_deref_mut(), research);
            }
        }
    }
    active_turn.map(|turn| turns.remove(&turn).unwrap_or_else(|| Model::new("", "")))
}

fn receipt_time(cwd: &str, sha: &str, completed: i64, repo: &str) -> i64 {
    if cwd.is_empty() || repo.is_empty() {
        return 0;
    }
    let path = if let Some(uri) = cwd.strip_prefix("file://") {
        let Some((_, path)) = uri.split_once('/') else {
            return 0;
        };
        format!("/{}", percent_decode_str(path).decode_utf8_lossy())
    } else {
        cwd.to_owned()
    };
    #[cfg(windows)]
    let path = if path.starts_with('/') && path.as_bytes().get(2) == Some(&b':') {
        path[1..].to_owned()
    } else {
        path
    };
    if git(&[
        "-C",
        &path,
        "rev-parse",
        "--path-format=absolute",
        "--git-common-dir",
    ]) == repo
    {
        receipt_cutoff(sha, completed)
    } else {
        0
    }
}

pub(super) fn read() -> Signature {
    let primary = env("CODEX_THREAD_ID");
    let alternate = env("CODEX_SESSION_ID");
    let id = field(&primary)
        .or_else(|| field(&alternate))
        .unwrap_or_default();
    let mut sig = Signature::new("Codex", id);
    if id.is_empty() {
        return sig;
    }
    let Some(home) = std::env::var_os("CODEX_HOME")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::home_dir().map(|h| h.join(".codex")))
    else {
        return sig;
    };
    let (own, children) = rollouts(&home, id);
    let mut receipts = Vec::new();
    if let Some(path) = &own {
        for row in rows(path) {
            let p = &row["payload"];
            if row["type"] == "turn_context" {
                sig.model = Model::new(string(&p["model"]), string(&p["effort"]));
            }
            if !completed_for(p, &row, id) {
                continue;
            }
            let item = &p["item"];
            if item["type"] == "UserMessage" {
                for part in array(&item["content"]) {
                    if part["type"] == "text"
                        && let Some(m) = RENAME.captures(string(&part["text"]).trim())
                    {
                        sig.name = field(&m[1]).unwrap_or_default().to_owned();
                    }
                }
            } else if item["type"] == "CommandExecution" {
                let argv = array(&item["command"]);
                if (argv.first().is_some_and(|v| v == "git")
                    && argv.get(1).is_some_and(|v| v == "commit"))
                    || argv.last().is_some_and(|v| COMMIT.is_match(string(v)))
                {
                    let time = timestamp(string(&row["timestamp"]));
                    receipts.extend(
                        RECEIPT
                            .captures_iter(string(&item["aggregated_output"]))
                            .map(|m| (string(&item["cwd"]).to_owned(), m[1].to_owned(), time)),
                    );
                }
            }
        }
    }
    let repo = git(&["rev-parse", "--path-format=absolute", "--git-common-dir"]);
    let cutoff = receipts
        .iter()
        .rev()
        .map(|(cwd, sha, time)| receipt_time(cwd, sha, *time, &repo))
        .find(|t| *t != 0)
        .unwrap_or_default();
    let since = cutoff.max(last_signed("Codex", id));
    if let Some(path) = own {
        thread_activity(&path, id, since, Some(&mut sig.team), &mut sig.research);
    }
    for (child, path) in children {
        if let Some(model) = thread_activity(&path, &child, since, None, &mut sig.research) {
            *sig.helpers.entry(model).or_default() += 1;
        }
    }
    sig
}
