# Skill scripts

Use the [PR guide](../references/pull-requests.md) for `github-merge.py` and the
[history upgrade guide](../references/upgrade-history.md) for `gitmoji-upgrade-history.py`.

## Signature scripts, one per harness

A commit made by an agent ends with a signature that says what went into it:

```
_
<model> (<effort>), <agent name, only if a person gave one>
<harness> (<session id, if there is one>)
Sub-agents: <count> × <model> (<effort>), ...      only if sub-agents did part of the work
Team: <other agents>                               only if it exchanged messages with others
Research: <n> searches, <n> pages, <n> doc lookups only if it looked something up
```

The scripts print the underscore. When the commit is made, the hook replaces it by `△` and,
between backticks, the version of the vocabulary the message was checked against.

Every value comes from the harness's own record of the session, never from the agent's
memory. An agent can be wrong about its own model and effort, and a sub-agent can run on
a different model than the agent that started it. The last three lines cover the time
since the session's last commit in the repository. On its first commit here, that
window can include earlier tasks or work in another checkout. Check optional counts
against the work in this commit.

Each signature script prints that block for one harness. Run yours and paste the output at the
end of the commit body:

| Harness | Script | Where the values come from |
|---|---|---|
| Claude Code | `claude-code.py` | `CLAUDE_EFFORT`, `CLAUDE_CODE_SESSION_ID`, and the session log under `~/.claude/projects/`: model per reply, `custom-title.json`, `subagents/`, and tool calls for team and research. A shell command such as `curl` is not counted as a page read |
| Codex | `codex.py` | `CODEX_THREAD_ID` (or `CODEX_SESSION_ID`) selects the rollout in `$CODEX_HOME` or `~/.codex`. Turn contexts give model and effort, a naming message gives the name, and spawn records identify direct helpers. Native sends and agmsg receipts give teammates. Completed web actions, Context7 docs commands and documentation tool calls give research counts for the parent and helpers. |
| Antigravity | `antigravity.py` | `ANTIGRAVITY_CONVERSATION_ID`, `ANTIGRAVITY_APP_DATA_DIR` (`transcript_full.jsonl` model/effort/team/research, `history.jsonl` `/rename`, and `conversation_summaries.db` sub-agents) |

## If your harness is not listed

Work it out once, then add a script here so the next agent does not have to.

1. Find where your harness records, for this session: the model of each reply, the
   effort level, the session id, the name a person gave you, and each sub-agent you
   started with its model and effort. Look at environment variables, the session or
   transcript log, and per-turn context records.
2. Write a script that prints the block above from those records. It must not take the
   values as arguments or guess a default. When a value cannot be found, it prints
   `unknown` in its place.
3. Cover the time since your session's last commit in this repository. Count the
   sub-agents that ran, grouped by model and effort. List the other agents you sent
   messages to. Count web searches, web pages read and documentation lookups, yours and
   your sub-agents'. Leave out a line when there is nothing to report.
4. Add a row to the table, and say in one line where each value comes from.
