#!/usr/bin/env python3
"""Print the commit signature for the Claude Code session this runs in.

Every value is read from what Claude Code records about the session:
  effort      CLAUDE_EFFORT
  session id  CLAUDE_CODE_SESSION_ID
  model       the last reply in ~/.claude/projects/<project>/<session>.jsonl
  name        <session>/custom-title.json, written when a person renames the session
  sub-agents  <session>/subagents/agent-*.jsonl, those active since this session's
              last commit in the current repository. That commit is found by its
              signature in git log. A session that has not signed a commit yet
              falls back to its last `git commit` command in the session log
  team        other agents this session sent a message to since that commit
              (SendMessage calls, and agmsg send.sh commands)
  research    web searches, web pages read and documentation lookups since that
              commit, by this session and its sub-agents

A value that cannot be found is printed as "unknown". Nothing is guessed.
"""

import collections
import glob
import json
import os
import re
import subprocess
import sys

# `send.sh <team> <from> <to>`, and the same inside `for x in a b c; do ... $x`.
AGMSG_SEND = re.compile(r"agmsg/scripts/send\.sh\s+\w+\s+[\w-]+\s+([A-Za-z][\w.-]*)")
AGMSG_LOOP = re.compile(r"for\s+(\w+)\s+in\s+([^;]+);\s*do[^\n]*agmsg/scripts/send\.sh\s+\w+\s+[\w-]+\s+\$\1")


def tool_calls(log, since):
    """Yield (name, input) for each tool call in a session log made after `since`."""
    for row in rows(log):
        if row.get("timestamp", "") <= since:
            continue
        content = (row.get("message") or {}).get("content")
        if not isinstance(content, list):
            continue
        for item in content:
            if isinstance(item, dict) and item.get("type") == "tool_use":
                yield item.get("name") or "", item.get("input") or {}


def count_research(calls, research):
    """Add the web and documentation tool calls among `calls` to `research`."""
    for tool, _ in calls:
        lower = tool.lower()
        if tool == "WebSearch":
            research["searches"] += 1
        elif tool == "WebFetch":
            research["pages"] += 1
        elif "docs" in lower or "context7" in lower:
            research["doc lookups"] += 1


def rows(path):
    """Yield the JSON objects of a session log, skipping lines that are not JSON."""
    try:
        with open(path, encoding="utf-8") as log:
            for line in log:
                try:
                    yield json.loads(line)
                except json.JSONDecodeError:
                    continue
    except OSError:
        return


def last_commit_time(session):
    """Time of this session's last commit here, as an ISO string, or "" if none."""
    try:
        out = subprocess.run(
            ["git", "log", "-1", "--fixed-strings", f"--grep=({session})", "--format=%cI"],
            capture_output=True, text=True, check=False,
        ).stdout.strip()
    except OSError:
        return ""
    if not out:
        return ""
    # Session logs use UTC with a Z; compare in the same form.
    try:
        from datetime import datetime, timezone
        return datetime.fromisoformat(out).astimezone(timezone.utc).strftime("%Y-%m-%dT%H:%M:%S")
    except ValueError:
        return ""


def main():
    session = os.environ.get("CLAUDE_CODE_SESSION_ID", "")
    effort = os.environ.get("CLAUDE_EFFORT", "") or "unknown"
    logs = glob.glob(os.path.expanduser(f"~/.claude/projects/*/{session}.jsonl")) if session else []
    log = logs[0] if logs else ""
    folder = log[: -len(".jsonl")] if log else ""

    model = "unknown"
    for row in rows(log):
        if row.get("type") == "assistant" and not row.get("isSidechain") and row.get("requestedModel"):
            model = row["requestedModel"]

    name = ""
    try:
        with open(os.path.join(folder, "custom-title.json"), encoding="utf-8") as title:
            name = json.load(title).get("customTitle", "")
    except (OSError, json.JSONDecodeError):
        pass

    since = last_commit_time(session) if session else ""
    if not since:
        # No signed commit yet: fall back to this session's last `git commit` command.
        # The command that runs this script is left out, or the window would be empty.
        for row in rows(log):
            content = (row.get("message") or {}).get("content")
            for item in content if isinstance(content, list) else []:
                if not isinstance(item, dict) or item.get("name") != "Bash":
                    continue
                command = str((item.get("input") or {}).get("command", ""))
                if "git commit" in command and "claude-code.py" not in command:
                    since = max(since, row.get("timestamp", "")[:19])
    helpers = collections.Counter()
    research = collections.Counter()
    team = set()
    for tool, given in tool_calls(log, since):
        if tool == "SendMessage" and given.get("to"):
            team.add(str(given["to"]).split(" [")[0])
        elif tool == "Bash":
            command = str(given.get("command", ""))
            team.update(AGMSG_SEND.findall(command))
            for _, names in AGMSG_LOOP.findall(command):
                team.update(names.split())
    count_research(tool_calls(log, since), research)
    for path in sorted(glob.glob(os.path.join(folder, "subagents", "agent-*.jsonl"))) if folder else []:
        used, active = None, False
        for row in rows(path):
            if row.get("requestedModel"):
                used = (row["requestedModel"], row.get("effort") or "unknown")
            if row.get("timestamp", "") > since:
                active = True
        if active:
            helpers[used or ("unknown", "unknown")] += 1
            count_research(tool_calls(path, since), research)

    print("_")
    print(f"{model} ({effort})" + (f", {name}" if name else ""))
    print("Claude Code" + (f" ({session})" if session else ""))
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
