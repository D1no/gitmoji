#!/usr/bin/env python3
"""Print this Codex thread's commit signature using its local rollout records."""

import json
import os
import re
import shlex
import subprocess
from collections import Counter
from datetime import datetime
from pathlib import Path
from urllib.parse import unquote, urlparse

UNKNOWN = ("unknown", "unknown")
RENAME = re.compile(
    r"^(?:/rename\s+|(?:alright[, ]+|okay[, ]+)?(?:I (?:have )?renamed you to |"
    r"your name is |call yourself ))([\w][\w '\-]*?)(?:[.!](?:\s.*)?|$)", re.I
)
COMMIT_COMMAND = re.compile(r"(?:^|[;&|\n])\s*git\s+commit(?:\s|$)")
RECEIPT = re.compile(r"^\[[^\]\n]+ ([0-9a-f]{7,64})\] ", re.M)
SEND_COMMAND = re.compile(
    r"""(?:^|[;&|\n])\s*(?:do\s+)?(?:bash\s+)?["']?(?:[^\s"']*/)?"""
    r"""(?:send\.sh["']?\s|agmsg["']?\s+send\s)"""
)
SENT_TO = re.compile(r"^Sent to ([\w./-]+) in team [\w.-]+$", re.M)
DOC_TOOLS = {"query_docs", "get_library_docs", "fetch_openai_doc", "search_openai_docs"}


def rows(path):
    try:
        with path.open("rb") as log:
            for line in log:
                try:
                    record = json.loads(line)
                except (ValueError, UnicodeError):
                    continue
                if isinstance(record, dict) and isinstance(record.get("payload"), dict):
                    yield record
    except OSError:
        return


def rollouts(home, thread):
    """Only a rollout's first header identifies its owner after a full-history fork."""
    own, children = None, {}
    for directory in ("archived_sessions", "sessions"):
        for path in sorted((home / directory).rglob("rollout-*.jsonl")):
            header = next(rows(path), {})
            if header.get("type") != "session_meta":
                continue
            meta = header["payload"]
            if meta.get("id") == thread:
                own = path
            source = meta.get("source")
            if isinstance(source, dict):
                spawn = source.get("subagent", {})
                if isinstance(spawn, dict):
                    edge = spawn.get("thread_spawn")
                    if isinstance(edge, dict) and edge.get("parent_thread_id") == thread and meta.get("id"):
                        children[meta["id"]] = path
    return own, children


def field(value):
    if isinstance(value, str) and value.strip() and value.isprintable() and not re.search(r"[(),:]", value):
        return value.strip()
    return ""


def model_and_effort(turn):
    return field(turn.get("model")) or "unknown", field(turn.get("effort")) or "unknown"


def git(*args):
    result = subprocess.run(["git", *args], capture_output=True, text=True, errors="replace")
    return result.stdout.strip() if result.returncode == 0 else ""


def timestamp(value):
    try:
        return datetime.fromisoformat(value.replace("Z", "+00:00")).timestamp()
    except (AttributeError, ValueError, OverflowError):
        return 0


def parent_records(path, thread):
    model, name, receipts = UNKNOWN, "", []
    for row in rows(path):
        p = row["payload"]
        if row.get("type") == "turn_context":
            model = model_and_effort(p)
        if row.get("type") != "event_msg" or p.get("type") != "item_completed" or p.get("thread_id") != thread:
            continue
        item = p["item"]
        if item["type"] == "UserMessage":
            for part in item.get("content", []):
                if part.get("type") == "text":
                    match = RENAME.fullmatch(part["text"].strip())
                    if match:
                        name = field(match[1])
        if item["type"] == "CommandExecution":
            # Before signatures contained a thread id, Git's own receipt identified the commit.
            command = item.get("command", [])
            if command and (command[:2] == ["git", "commit"] or COMMIT_COMMAND.search(command[-1])):
                for sha in RECEIPT.findall(item.get("aggregated_output") or ""):
                    receipts.append((item.get("cwd"), sha, timestamp(row.get("timestamp"))))
    return model, name, receipts


def last_commit(thread, receipts):
    # The heading is case-sensitive. Git uses basic regexps, in which parentheses are literal.
    escaped = re.sub(r"([][.^$*\\])", r"\\\1", thread)
    signed = git("log", "--all", "HEAD", "-1", "--format=%ct", f"--grep=^Codex ({escaped})$")
    cutoff = int(signed or 0)
    repository = git("rev-parse", "--path-format=absolute", "--git-common-dir")
    for cwd, sha, completed in reversed(receipts):
        if not repository or not cwd:
            continue
        if cwd.startswith("file://"):
            cwd = unquote(urlparse(cwd).path)
        if git("-C", cwd, "rev-parse", "--path-format=absolute", "--git-common-dir") != repository:
            continue
        date = git("show", "-s", "--format=%ct", f"{sha}^{{commit}}")
        if date:
            committed = int(date)
            # Git dates have only seconds. Use the finer receipt time when both agree.
            return max(cutoff, completed if int(completed) == committed else committed)
    return cutoff


def context7_lookup(command):
    """Recognize a direct docs query. Commands quoted inside other tools are examples."""
    try:
        words = shlex.split(command)
    except ValueError:
        return False
    if not words:
        return False
    if Path(words[0]).name == "npx":
        words = words[1:]
        if words[:1] in (["-y"], ["--yes"]):
            words = words[1:]
    return (len(words) >= 3 and Path(words[0]).name.split("@")[0] == "ctx7"
            and words[1] == "docs")


def record_work(item, team, research):
    kind = item["type"]
    if team is not None and kind == "SubAgentActivity" and item.get("kind") == "interacted":
        recipient = field(item.get("agent_path")) or field(item.get("agent_thread_id"))
        if recipient:
            team.add(recipient)
    elif team is not None and kind == "CollabAgentToolCall":
        if item.get("tool") in {"sendInput", "send_input"} and item.get("status") == "completed":
            team.update(target for target in item.get("receiver_thread_ids", []) if field(target))
    elif kind == "CommandExecution":
        commands = [entry["cmd"] for entry in item.get("parsed_cmd", []) if entry.get("cmd")]
        if not commands and item.get("command"):
            argv = item["command"]
            commands = [argv[-1] if "-c" in argv or "-lc" in argv else shlex.join(argv)]
        # The body of a here-document is data, even when it quotes an entire send command.
        if team is not None and any(SEND_COMMAND.search(cmd.split("<<", 1)[0]) for cmd in commands):
            team.update(SENT_TO.findall(item.get("aggregated_output") or ""))
        if item.get("exit_code") == 0:
            research["doc lookups"] += sum(context7_lookup(cmd) for cmd in commands)
    elif kind == "WebSearch" or (kind == "Extension" and item.get("kind") == "web.search"):
        action = item.get("action") or {}
        if action.get("type") == "search":
            research["searches"] += len(action.get("queries") or []) or 1
        # One web call can both search and open several pages. Search hits have other ref IDs.
        views = {r["ref_id"] for r in item.get("results") or []
                 if re.fullmatch(r"turn\d+(?:view|fetch)\d+", r.get("ref_id") or "")}
        if views:
            research["pages"] += len(views)
        elif "results" not in item and action.get("type") in {"openPage", "findInPage"}:
            research["pages"] += 1
    elif kind == "McpToolCall" and item.get("status") == "completed":
        tool = (item.get("tool") or "").lower().replace("-", "_")
        if tool in DOC_TOOLS:
            research["doc lookups"] += 1
        arguments = item.get("arguments")
        if (team is not None and item.get("server") == "codex_tui"
                and tool == "send_message_to_thread" and isinstance(arguments, dict)):
            recipient = field(arguments.get("threadId"))
            if recipient:
                team.add(recipient)


def thread_activity(path, thread, since, research, team=None):
    turns = {}
    active_turn = None
    counted = set()
    for row in rows(path):
        p = row["payload"]
        if row.get("type") == "turn_context" and p.get("turn_id"):
            turns[p["turn_id"]] = model_and_effort(p)
        if (row.get("type") == "event_msg" and p.get("type") == "item_completed"
                and p.get("thread_id") == thread and p["item"]["type"] != "UserMessage"
                and timestamp(row.get("timestamp")) > since):
            active_turn = p.get("turn_id", "")
            item = p["item"]
            item_id = item.get("id")
            if not item_id or item_id not in counted:
                record_work(item, team, research)
                counted.add(item_id)
    return turns.get(active_turn, UNKNOWN) if active_turn is not None else None


def main():
    home = Path(os.environ.get("CODEX_HOME") or Path.home() / ".codex")
    thread = field(os.environ.get("CODEX_THREAD_ID")) or field(os.environ.get("CODEX_SESSION_ID"))
    own, children = rollouts(home, thread) if thread else (None, {})
    (model, effort), name, receipts = parent_records(own, thread) if own else (UNKNOWN, "", [])
    print("_")
    print(f"{model} ({effort})" + (f", {name}" if name else ""))
    print("Codex" + (f" ({thread})" if thread else ""))
    since = last_commit(thread, receipts) if thread else 0
    team, research, groups = set(), Counter(), Counter()
    if own:
        thread_activity(own, thread, since, research, team)
    for child, path in children.items():
        pair = thread_activity(path, child, since, research)
        if pair:
            groups[pair] += 1
    if groups:
        print("Sub-agents: " + ", ".join(
            f"{count} × {m} ({e})" for (m, e), count in sorted(groups.items())
        ))
    if team:
        print("Team: " + ", ".join(sorted(team)))
    totals = [f"{research[k]} {k}" for k in ("searches", "pages", "doc lookups") if research[k]]
    if totals:
        print("Research: " + ", ".join(totals))


if __name__ == "__main__":
    main()
