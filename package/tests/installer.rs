//! Exercise upgrades in repositories that share a tracked skill across harnesses.

#![cfg(unix)]
#![allow(clippy::unwrap_used)]

use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::Command;

fn git(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(repo)
        .args(["-c", "user.name=T", "-c", "user.email=t@example.invalid"])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn shared_skill_upgrade_preserves_index_and_prints_stageable_target_paths() {
    let repo = std::env::temp_dir().join(format!("gitmoji-shared-skill-{}", std::process::id()));
    let _ = fs::remove_dir_all(&repo);
    fs::create_dir_all(repo.join(".claude/skills")).unwrap();
    let shared = "team's [shared] skills";
    let skill = repo.join(shared);
    fs::create_dir_all(skill.join("scripts")).unwrap();
    fs::create_dir_all(skill.join("references")).unwrap();
    symlink(
        format!("../../{shared}"),
        repo.join(".claude/skills/gitmoji"),
    )
    .unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    fs::write(repo.join("lefthook.yml"), "# project-managed hooks\n").unwrap();
    let edit = format!("{shared}/SKILL.md");
    let deletion = format!("{shared}/scripts/claude-code.py");
    let conflict = format!("{shared}/scripts/antigravity.py");
    let retired = format!("{shared}/scripts/README.md");
    let customized_retired = format!("{shared}/scripts/codex.py");
    let user_script = format!("{shared}/scripts/custom.py");
    let clean = format!("{shared}/references/pull-requests.md");
    for path in [
        &edit,
        &deletion,
        &conflict,
        &customized_retired,
        &user_script,
        &clean,
    ] {
        fs::write(repo.join(path), "old version\n").unwrap();
    }
    fs::write(
        repo.join(&retired),
        "# Skill scripts\n\nUse the [PR guide](../references/pull-requests.md) for `github-merge.py` and the\n[history upgrade guide](../references/upgrade-history.md) for `gitmoji-upgrade-history.py`.\n\n## Signature scripts, one per harness\n\nA commit made by an agent ends with a signature that says what went into it:\n\n```\n_\n<model> (<effort>), <agent name, only if a person gave one>\n<harness> (<session id, if there is one>)\nSub-agents: <count> × <model> (<effort>), ...      only if sub-agents did part of the work\nTeam: <other agents>                               only if it exchanged messages with others\nResearch: <n> searches, <n> pages, <n> doc lookups only if it looked something up\n```\n\nThe scripts print the underscore. When the commit is made, the hook replaces it by `△` and,\nbetween backticks, the version of the vocabulary the message was checked against.\n\nEvery value comes from the harness's own record of the session, never from the agent's\nmemory. An agent can be wrong about its own model and effort, and a sub-agent can run on\na different model than the agent that started it. The last three lines cover the time\nsince the session's last commit in the repository. On its first commit here, that\nwindow can include earlier tasks or work in another checkout. Check optional counts\nagainst the work in this commit.\n\nEach signature script prints that block for one harness. Run yours and paste the output at the\nend of the commit body:\n\n| Harness | Script | Where the values come from |\n|---|---|---|\n| Claude Code | `claude-code.py` | `CLAUDE_EFFORT`, `CLAUDE_CODE_SESSION_ID`, and the session log under `~/.claude/projects/`: model per reply, `custom-title.json`, `subagents/`, and tool calls for team and research. A shell command such as `curl` is not counted as a page read |\n| Codex | `codex.py` | `CODEX_THREAD_ID` (or `CODEX_SESSION_ID`) selects the rollout in `$CODEX_HOME` or `~/.codex`. Turn contexts give model and effort, a naming message gives the name, and spawn records identify direct helpers. Native sends and agmsg receipts give teammates. Completed web actions, Context7 docs commands and documentation tool calls give research counts for the parent and helpers. |\n| Antigravity | `antigravity.py` | `ANTIGRAVITY_CONVERSATION_ID`, `ANTIGRAVITY_APP_DATA_DIR` (`transcript_full.jsonl` model/effort/team/research, `history.jsonl` `/rename`, and `conversation_summaries.db` sub-agents) |\n\n## If your harness is not listed\n\nWork it out once, then add a script here so the next agent does not have to.\n\n1. Find where your harness records, for this session: the model of each reply, the\n   effort level, the session id, the name a person gave you, and each sub-agent you\n   started with its model and effort. Look at environment variables, the session or\n   transcript log, and per-turn context records.\n2. Write a script that prints the block above from those records. It must not take the\n   values as arguments or guess a default. When a value cannot be found, it prints\n   `unknown` in its place.\n3. Cover the time since your session's last commit in this repository. Count the\n   sub-agents that ran, grouped by model and effort. List the other agents you sent\n   messages to. Count web searches, web pages read and documentation lookups, yours and\n   your sub-agents'. Leave out a line when there is nothing to report.\n4. Add a row to the table, and say in one line where each value comes from.\n",
    )
    .unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-qm", "base"]);
    git(&repo, &["switch", "-qc", "other"]);
    fs::write(repo.join(&conflict), "other branch\n").unwrap();
    git(&repo, &["commit", "-qam", "other"]);
    git(&repo, &["switch", "-q", "main"]);
    fs::write(repo.join(&conflict), "main branch\n").unwrap();
    git(&repo, &["commit", "-qam", "main"]);
    let merge = Command::new("git")
        .current_dir(&repo)
        .args([
            "-c",
            "user.name=T",
            "-c",
            "user.email=t@example.invalid",
            "merge",
            "other",
        ])
        .output()
        .unwrap();
    assert_eq!(merge.status.code(), Some(1));
    let conflict_before = fs::read(repo.join(&conflict)).unwrap();
    assert!(!git(&repo, &["ls-files", "-u"]).is_empty());

    fs::write(repo.join(&edit), "staged instructions\n").unwrap();
    git(&repo, &["--literal-pathspecs", "add", "--", &edit]);
    fs::write(repo.join(&edit), "worktree instructions\n").unwrap();
    git(&repo, &["--literal-pathspecs", "rm", "--", &deletion]);
    let index_before = git(&repo, &["ls-files", "--stage"]);
    let run = Command::new(env!("CARGO_BIN_EXE_gitmoji"))
        .current_dir(&repo)
        .arg("init")
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(git(&repo, &["ls-files", "--stage"]), index_before);
    assert_eq!(
        fs::read_to_string(repo.join(&edit)).unwrap(),
        "worktree instructions\n"
    );
    assert_eq!(fs::read(repo.join(&conflict)).unwrap(), conflict_before);
    assert!(!repo.join(&deletion).exists());
    assert!(!repo.join(&retired).exists());
    assert_eq!(
        fs::read_to_string(repo.join(&customized_retired)).unwrap(),
        "old version\n"
    );
    assert_eq!(
        fs::read_to_string(repo.join(&user_script)).unwrap(),
        "old version\n"
    );
    assert_ne!(
        fs::read_to_string(repo.join(&clean)).unwrap(),
        "old version\n"
    );
    assert!(!skill.join("scripts/codex.py.before-update").exists());
    assert!(!skill.join("SKILL.md.before-update").exists());

    // Run the exact command that a contributor would copy from the instructions.
    let output = String::from_utf8(run.stdout).unwrap();
    let commands = output
        .split_once("  cd ")
        .unwrap()
        .1
        .split_once("  git commit")
        .unwrap()
        .0;
    let stage = Command::new("sh")
        .args(["-c", &format!("cd {commands}")])
        .output()
        .unwrap();
    assert!(
        stage.status.success(),
        "{}",
        String::from_utf8_lossy(&stage.stderr)
    );
    assert_eq!(
        git(&repo, &["show", &format!(":{edit}")]),
        "staged instructions\n"
    );
    assert_eq!(
        git(&repo, &["--literal-pathspecs", "ls-files", "--", &deletion]),
        ""
    );
    assert_eq!(
        git(&repo, &["--literal-pathspecs", "ls-files", "--", &retired]),
        ""
    );
    assert_eq!(
        git(&repo, &["show", &format!(":{user_script}")]),
        "old version\n"
    );
    assert!(
        !git(
            &repo,
            &["--literal-pathspecs", "ls-files", "-u", "--", &conflict]
        )
        .is_empty()
    );
    assert_eq!(
        git(&repo, &["show", &format!(":{clean}")]),
        fs::read_to_string(repo.join(clean)).unwrap()
    );
    fs::remove_dir_all(repo).unwrap();
}

#[test]
fn init_preserves_staged_removal_of_the_directory_link() {
    let repo = std::env::temp_dir().join(format!("gitmoji-removed-link-{}", std::process::id()));
    let _ = fs::remove_dir_all(&repo);
    fs::create_dir_all(repo.join(".claude/skills")).unwrap();
    fs::create_dir_all(repo.join("shared")).unwrap();
    let link = ".claude/skills/gitmoji";
    symlink("../../shared", repo.join(link)).unwrap();
    git(&repo, &["init", "-q"]);
    git(&repo, &["add", "--", link]);
    git(&repo, &["commit", "-qm", "share the skill"]);
    git(&repo, &["rm", "--", link]);
    let index_before = git(&repo, &["diff", "--cached"]);
    let run = Command::new(env!("CARGO_BIN_EXE_gitmoji"))
        .current_dir(&repo)
        .arg("init")
        .output()
        .unwrap();
    assert!(!run.status.success());
    assert!(
        String::from_utf8_lossy(&run.stderr).contains(".claude/skills/gitmoji has staged changes")
    );
    assert!(!repo.join(link).exists());
    assert_eq!(git(&repo, &["diff", "--cached"]), index_before);
    fs::remove_dir_all(repo).unwrap();
}
