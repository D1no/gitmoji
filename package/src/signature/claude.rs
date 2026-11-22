use super::*;

fn tools(row: &Value) -> impl Iterator<Item = (&str, &Value)> {
    array(&row["message"]["content"])
        .iter()
        .filter(|v| v["type"] == "tool_use")
        .map(|v| (string(&v["name"]), &v["input"]))
}

fn research_call(name: &str, research: &mut Research) {
    match name {
        "WebSearch" => research.searches += 1,
        "WebFetch" => research.pages += 1,
        _ => {
            let lower = name.to_ascii_lowercase();
            if lower.contains("docs") || lower.contains("context7") {
                research.docs += 1;
            }
        }
    }
}

pub(super) fn read() -> Signature {
    let id = env("CLAUDE_CODE_SESSION_ID");
    let id = session_id(&id);
    let mut sig = Signature::new("Claude Code", id);
    let effort = env("CLAUDE_EFFORT");
    sig.model = Model::new("", &effort);
    let Some(home) = std::env::home_dir() else {
        return sig;
    };
    if id.is_empty() {
        return sig;
    }
    let name = format!("{id}.jsonl");
    let Some(log) = entries(&home.join(".claude/projects"))
        .iter()
        .map(|dir| dir.join(&name))
        .find(|path| path.is_file())
    else {
        return sig;
    };
    let folder = log.with_extension("");
    if let Some(title) = File::open(folder.join("custom-title.json"))
        .ok()
        .and_then(|file| serde_json::from_reader::<_, Value>(file).ok())
    {
        sig.name = string(&title["customTitle"]).to_owned();
    }

    let signed = last_signed("Claude Code", id);
    let mut since = signed;
    for row in rows(&log) {
        if row["type"] == "assistant"
            && row["isSidechain"] != true
            && let Some(model) = field(string(&row["requestedModel"]))
        {
            sig.model = Model::new(model, &effort);
        }
        if signed == 0
            && tools(&row).any(|(name, input)| {
                let cmd = string(&input["command"]);
                name == "Bash"
                    && cmd.contains("git commit")
                    && !cmd.contains("claude-code.py")
                    && !cmd.contains("gitmoji signature")
            })
        {
            since = since.max(timestamp(string(&row["timestamp"])) / 1_000_000 * 1_000_000);
        }
    }
    for row in rows(&log).filter(|row| timestamp(string(&row["timestamp"])) > since) {
        for (tool, input) in tools(&row) {
            match tool {
                "SendMessage" => {
                    let to = string(&input["to"]);
                    add_peer(&mut sig.team, to.split(" [").next().unwrap_or_default());
                }
                "Bash" => agmsg_peers(string(&input["command"]), &mut sig.team),
                _ => {}
            }
            research_call(tool, &mut sig.research);
        }
    }
    for child in entries(&folder.join("subagents"))
        .into_iter()
        .filter(|p| filename(p).starts_with("agent-") && filename(p).ends_with(".jsonl"))
    {
        let mut model = Model::new("", "");
        let mut active = false;
        for row in rows(&child) {
            if !string(&row["requestedModel"]).is_empty() {
                model = Model::new(string(&row["requestedModel"]), string(&row["effort"]));
            }
            if timestamp(string(&row["timestamp"])) > since {
                active = true;
                for (tool, _) in tools(&row) {
                    research_call(tool, &mut sig.research);
                }
            }
        }
        if active {
            *sig.helpers.entry(model).or_default() += 1;
        }
    }
    sig
}
