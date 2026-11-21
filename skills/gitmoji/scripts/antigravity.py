#!/usr/bin/env python3
"""Print the commit signature for the Antigravity session this runs in.

Every value is read from what Antigravity records about the session:
  session id  ANTIGRAVITY_CONVERSATION_ID
  model       last PLANNER_RESPONSE "model" field in
              $ANTIGRAVITY_APP_DATA_DIR/brain/<session>/.system_generated/logs/transcript_full.jsonl
  effort      trailing -minimal/-low/-medium/-high suffix on that model string (or "unknown")
  name        last "/rename <name>" slash_command for this session in
              $ANTIGRAVITY_APP_DATA_DIR/history.jsonl (omitted if never renamed)
  sub-agents  descendant conversations from transcript_full.jsonl and
              $ANTIGRAVITY_APP_DATA_DIR/conversation_summaries.db active since this
              session's last commit in the current repository
  team        other agents this session sent a message to since that commit
              (peer send_message calls, and agmsg send.sh commands)
  research    web searches, web pages read and documentation lookups since that
              commit, by this session and its sub-agents

A value that cannot be found is printed as "unknown". Nothing is guessed.
"""

import collections
from datetime import datetime, timezone
import json
import os
import re
import sqlite3
import subprocess
import sys


EFFORT_SUFFIX_RE = re.compile(r"^(?P<model>.+)-(?P<effort>minimal|low|medium|high)$")
SUBAGENT_RE = re.compile(
    r"^Created At: [^\n]+\nCompleted At: [^\n]+\nCreated the following subagent:\n"
    r"\{\s*\n\s*\"conversationId\"\s*:\s*\"([0-9a-fA-F-]+)\"",
    re.M,
)
COMMIT_COMMAND = re.compile(r"(?:^|[;&|\n])\s*git(?:\s+-C\s+\S+)?\s+commit(?:\s|$)")
RECEIPT_RE = re.compile(r"^\[[^\]\n]+ ([0-9a-f]{7,64})\] ", re.M)
AGMSG_SEND = re.compile(r"agmsg/scripts/send\.sh\s+\w+\s+[\w-]+\s+([A-Za-z][\w.-]*)")
AGMSG_LOOP = re.compile(
    r"for\s+(\w+)\s+in\s+([^;]+);\s*do[^\n]*agmsg/scripts/send\.sh\s+\w+\s+[\w-]+\s+\$\1"
)


def split_model_effort(raw_model):
    """Split 'gemini-4-hydrogen-medium' into ('gemini-4-hydrogen', 'medium')."""
    if not raw_model:
        return "unknown", "unknown"
    m = EFFORT_SUFFIX_RE.match(raw_model.strip())
    if m:
        return m.group("model"), m.group("effort")
    return raw_model.strip(), "unknown"


def transcript_path(app_data_dir, session):
    if not app_data_dir or not session:
        return ""
    logs_dir = os.path.join(app_data_dir, "brain", session, ".system_generated", "logs")
    full = os.path.join(logs_dir, "transcript_full.jsonl")
    if os.path.exists(full):
        return full
    return os.path.join(logs_dir, "transcript.jsonl")


def rows(path):
    """Yield JSON objects from a JSONL file, skipping malformed lines."""
    if not path:
        return
    try:
        with open(path, encoding="utf-8", errors="replace") as f:
            for line in f:
                try:
                    yield json.loads(line)
                except json.JSONDecodeError:
                    continue
    except OSError:
        return


def git_commit_iso(rev_args):
    """Return UTC ISO timestamp (YYYY-MM-DDTHH:MM:SSZ) for a git log/show query, or ''."""
    try:
        proc = subprocess.run(
            ["git", *rev_args], capture_output=True, text=True, check=False
        )
    except OSError:
        return ""
    out = proc.stdout.strip() if proc.returncode == 0 else ""
    if not out:
        return ""
    try:
        return datetime.fromisoformat(out).astimezone(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    except ValueError:
        return ""


def last_commit_time(session, receipt_shas):
    """Latest UTC timestamp of this session's commits in the current repository."""
    cutoff = ""
    if session:
        cutoff = git_commit_iso(
            ["log", "--all", "HEAD", "-1", "--fixed-strings", f"--grep=({session})", "--format=%cI"]
        )
    for sha, step_ts in reversed(receipt_shas):
        commit_ts = git_commit_iso(["show", "-s", "--format=%cI", f"{sha}^{{commit}}"])
        if commit_ts:
            cutoff = max(cutoff, step_ts if step_ts[:19] == commit_ts[:19] else commit_ts)
            break
    return cutoff


def given_name(app_data_dir, session):
    """Return the last explicit '/rename <name>' for this session in history.jsonl."""
    if not app_data_dir or not session:
        return ""
    name = ""
    for row in rows(os.path.join(app_data_dir, "history.jsonl")):
        if row.get("type") == "slash_command" and row.get("conversationId") == session:
            display = row.get("display", "")
            if display.startswith("/rename "):
                candidate = display[len("/rename "):].strip()
                if candidate:
                    name = candidate
    return name


def db_descendants(app_data_dir, root_session):
    """Collect all descendant conversation IDs from conversation_summaries.db."""
    if not app_data_dir or not root_session:
        return set()
    db_path = os.path.join(app_data_dir, "conversation_summaries.db")
    if not os.path.exists(db_path):
        return set()
    children = collections.defaultdict(list)
    try:
        conn = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True)
        try:
            for cid, pid in conn.execute(
                "SELECT conversation_id, parent_conversation_id FROM conversation_summaries"
            ):
                if pid and cid:
                    children[pid].append(cid)
        finally:
            conn.close()
    except sqlite3.Error:
        return set()

    descendants = set()
    queue = collections.deque([root_session])
    while queue:
        cur = queue.popleft()
        for child in children.get(cur, ()):
            if child != root_session and child not in descendants:
                descendants.add(child)
                queue.append(child)
    return descendants


def tool_calls_since(log_path, since):
    """Yield (tool_name, args_dict) for PLANNER_RESPONSE calls after `since`."""
    for row in rows(log_path):
        if since and row.get("created_at", "") <= since:
            continue
        if row.get("type") != "PLANNER_RESPONSE":
            continue
        for tc in row.get("tool_calls") or []:
            if isinstance(tc, dict):
                yield tc.get("name") or "", tc.get("args") or {}


def count_research(calls, research):
    """Add web search, page read, and doc lookup tool calls to `research`."""
    for tool, args in calls:
        lower = tool.lower()
        if tool == "search_web":
            research["searches"] += 1
        elif tool in ("read_url_content", "web_fetch", "fetch_url"):
            research["pages"] += 1
        elif "docs" in lower or "context7" in lower:
            research["doc lookups"] += 1
        elif tool == "run_command":
            cmd = str(args.get("CommandLine", ""))
            if "ctx7" in cmd and " docs " in cmd:
                research["doc lookups"] += 1


def subagent_info(log_path, since):
    """Return (model, effort, active_since) from a subagent's transcript."""
    raw = ""
    active = False
    for row in rows(log_path):
        if row.get("type") == "PLANNER_RESPONSE" and row.get("model"):
            raw = row["model"]
        if not since or row.get("created_at", "") > since:
            active = True
    m, e = split_model_effort(raw)
    return m, e, active


def main():
    session = os.environ.get("ANTIGRAVITY_CONVERSATION_ID", "").strip()
    app_data_dir = os.environ.get("ANTIGRAVITY_APP_DATA_DIR", "").strip()
    log_path = transcript_path(app_data_dir, session)

    raw_model = ""
    transcript_subs = set()
    receipt_shas = []
    pending_commit = False
    for row in rows(log_path):
        rtype = row.get("type")
        if rtype == "PLANNER_RESPONSE":
            if row.get("model"):
                raw_model = row["model"]
            tcs = row.get("tool_calls") or []
            pending_commit = any(
                isinstance(tc, dict)
                and tc.get("name") == "run_command"
                and COMMIT_COMMAND.search(str((tc.get("args") or {}).get("CommandLine", "")))
                for tc in tcs
            )
        elif rtype == "GENERIC":
            content = row.get("content", "")
            if content.startswith("Created At: "):
                m = SUBAGENT_RE.search(content)
                if m:
                    transcript_subs.add(m.group(1))
            if pending_commit:
                for sha in RECEIPT_RE.findall(content):
                    receipt_shas.append((sha, row.get("created_at", "")))

    model, effort = split_model_effort(raw_model)
    name = given_name(app_data_dir, session)
    since = last_commit_time(session, receipt_shas)
    all_subs = transcript_subs | db_descendants(app_data_dir, session)

    team = set()
    research = collections.Counter()
    parent_calls = list(tool_calls_since(log_path, since))
    for tool, args in parent_calls:
        if tool == "send_message" and args.get("Recipient"):
            rec = str(args["Recipient"]).strip().strip('"')
            if rec and rec not in all_subs:
                peer_name = given_name(app_data_dir, rec)
                team.add(peer_name or rec)
        elif tool == "run_command":
            cmd = str(args.get("CommandLine", ""))
            team.update(AGMSG_SEND.findall(cmd))
            for _, names in AGMSG_LOOP.findall(cmd):
                team.update(names.split())
    count_research(parent_calls, research)

    helpers = collections.Counter()
    for sub_id in sorted(all_subs):
        sub_log = transcript_path(app_data_dir, sub_id)
        sm, se, active = subagent_info(sub_log, since)
        if active:
            helpers[(sm, se)] += 1
            count_research(tool_calls_since(sub_log, since), research)

    print("_")
    print(f"{model} ({effort})" + (f", {name}" if name else ""))
    print("Antigravity" + (f" ({session})" if session else " (unknown)"))
    if helpers:
        parts = [f"{count} × {m} ({e})" for (m, e), count in sorted(helpers.items())]
        print("Sub-agents: " + ", ".join(parts))
    if team:
        print("Team: " + ", ".join(sorted(team)))
    if research:
        order = ["searches", "pages", "doc lookups"]
        print("Research: " + ", ".join(f"{research[k]} {k}" for k in order if research[k]))
    return 0


if __name__ == "__main__":
    sys.exit(main())
