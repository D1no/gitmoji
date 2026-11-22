//! The `gitmoji` command as a user runs it: arguments in, text and an exit code out.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn gitmoji(folder: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_gitmoji"))
        .args(args)
        .current_dir(folder)
        .output()
        .unwrap()
}

fn text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn scratch(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!("gitmoji-cli-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    folder
}

fn git(folder: &Path, args: &[&str]) {
    let done = Command::new("git")
        .args(["-c", "user.name=T", "-c", "user.email=t@example.invalid"])
        .args(args)
        .current_dir(folder)
        .output()
        .unwrap();
    assert!(done.status.success(), "git {args:?}: {}", text(&done));
}

#[test]
fn explain_should_read_a_subject_and_a_run_of_emojis() {
    let here = scratch("explain");
    let subject = gitmoji(&here, &["explain", "🧩👔 #1 (server): allow an appeal"]);
    assert!(subject.status.success(), "{}", text(&subject));
    assert!(text(&subject).contains("for #1, in server"));
    let emojis = gitmoji(&here, &["explain", "🧩👔"]);
    assert!(emojis.status.success(), "{}", text(&emojis));
    assert!(text(&emojis).contains("👔"));
    let wrong = gitmoji(&here, &["explain", "fix: allow an appeal"]);
    assert!(!wrong.status.success());
    std::fs::remove_dir_all(here).unwrap();
}

#[test]
fn check_fix_should_write_the_file_it_was_given() {
    let here = scratch("fix");
    let file = here.join("COMMIT_EDITMSG");
    std::fs::write(
        &file,
        ":jigsaw::necktie: #1 (server): allow an appeal\n\nWhy.\n",
    )
    .unwrap();
    let done = gitmoji(&here, &["check", "--fix", file.to_str().unwrap()]);
    assert!(done.status.success(), "{}", text(&done));
    let written = std::fs::read_to_string(&file).unwrap();
    assert!(
        written.starts_with("🧩👔 #1 (server): allow an appeal\n"),
        "{written}"
    );
    assert!(
        written.ends_with("`\n") && written.contains("\n△ `"),
        "{written}"
    );
    // A second run changes nothing.
    assert!(
        gitmoji(&here, &["check", "--fix", file.to_str().unwrap()])
            .status
            .success()
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), written);
    std::fs::remove_dir_all(here).unwrap();
}

#[test]
fn agent_only_signatures_apply_to_live_authored_messages() {
    let repo = scratch("agent-signature");
    git(&repo, &["init", "-q"]);
    std::fs::write(repo.join("gitmoji.toml"), "[rules]\nsignature = 'agents'\n").unwrap();
    let markers = [
        ("CLAUDECODE", "1"),
        ("CLAUDE_CODE_SESSION_ID", "test-session"),
        ("CLAUDE_CODE_ENTRYPOINT", "cli"),
        ("CODEX_THREAD_ID", "test-thread"),
        ("CODEX_SESSION_ID", "test-session"),
        ("ANTIGRAVITY_CONVERSATION_ID", "test-session"),
    ];
    let run = |args: &[&str], session: Option<(&str, &str)>| {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_gitmoji"));
        cmd.current_dir(&repo).args(args);
        for (key, _) in markers {
            cmd.env_remove(key);
        }
        if let Some((key, value)) = session {
            cmd.env(key, value);
        }
        cmd.output().unwrap()
    };
    let draft = repo.join("draft");
    let subject = "🧩👔 #3 (server): allow an appeal";
    let message = format!("{subject}\n\nCustomers need time to reply.\n");
    std::fs::write(&draft, &message).unwrap();
    for session in [
        None,
        Some(("CLAUDECODE", "0")),
        Some(("CODEX_THREAD_ID", "")),
    ] {
        let human = run(&["check", "draft"], session);
        assert!(human.status.success(), "{}", text(&human));
    }
    for session in markers {
        let agent = run(&["check", "draft"], Some(session));
        assert!(!agent.status.success() && text(&agent).contains("an agent signs"));
    }

    let session = Some(("CLAUDECODE", "1"));
    let unsigned = run(&["check", "--fix", "draft"], session);
    assert!(!unsigned.status.success() && text(&unsigned).contains("an agent signs"));
    assert!(!text(&unsigned).contains("--no-commit"));
    let stamped = run(&["check", "draft"], session);
    assert!(!stamped.status.success() && text(&stamped).contains("an agent signs"));

    std::fs::write(
        &draft,
        format!("{message}\n_\nexample-model (high)\nClaude Code\n"),
    )
    .unwrap();
    for args in [
        vec!["check", "draft"],
        vec!["check", "--fix", "draft"],
        vec!["check", "draft"],
    ] {
        let signed = run(&args, session);
        assert!(signed.status.success(), "{}", text(&signed));
    }
    assert!(std::fs::read_to_string(&draft).unwrap().contains("\n△ `"));
    for subject in [
        "🔺 gitmoji `1.0.0`: adopt the release",
        "🔖 release 1.0.0",
        "Merge branch 'topic'",
        "Revert \"Allow an appeal\"",
        "fixup! Allow an appeal",
    ] {
        std::fs::write(&draft, subject).unwrap();
        let exempt = run(&["check", "--fix", "draft"], session);
        assert!(exempt.status.success(), "{subject}: {}", text(&exempt));
    }
    assert!(run(&["check", "-m", subject], session).status.success());

    git(&repo, &["commit", "--allow-empty", "-qm", &message]);
    let history = run(&["history", "HEAD"], session);
    assert!(history.status.success(), "{}", text(&history));
    std::fs::remove_dir_all(repo).unwrap();
}

#[test]
fn reject_a_ticket_led_draft_and_preserve_the_stored_history() {
    let repo = scratch("ticket-at-line-start");
    git(&repo, &["init", "-q"]);
    let subject = "🧩👔 #3 (server): allow an appeal";
    let file = repo.join("draft");
    let draft = format!("{subject}\n\nAnother reason.\n#42: keep the decision visible.\n");
    std::fs::write(&file, &draft).unwrap();

    for cleanup in ["strip", "verbatim"] {
        git(&repo, &["config", "commit.cleanup", cleanup]);
        for args in [vec!["check", "-m", &draft], vec!["check", "--fix", "draft"]] {
            let result = gitmoji(&repo, &args);
            assert!(!result.status.success());
            assert!(text(&result).contains("See #42"), "{}", text(&result));
        }
        assert!(
            std::fs::read_to_string(&file)
                .unwrap()
                .contains("#42: keep")
        );
    }

    git(&repo, &["config", "core.commentChar", ";"]);
    let result = gitmoji(&repo, &["check", "-m", &draft]);
    assert!(!result.status.success() && text(&result).contains("See #42"));
    git(&repo, &["config", "--unset", "core.commentChar"]);

    git(&repo, &["commit", "--allow-empty", "-q", "-m", &draft]);
    let history = gitmoji(&repo, &["history"]);
    assert!(history.status.success(), "{}", text(&history));
    git(&repo, &["config", "commit.cleanup", "strip"]);
    let safe = format!(
        "{subject}\n\nSee #42 for the decision.\n #42 is indented.\n# On branch main\n# ------------------------ >8 ------------------------\n#42 below scissors"
    );
    for message in [&safe, "Revert \"Allow an appeal\"\n\n#42 in generated text"] {
        let result = gitmoji(&repo, &["check", "-m", message]);
        assert!(result.status.success(), "{}", text(&result));
    }
    std::fs::remove_dir_all(repo).unwrap();
}

#[test]
fn check_should_preserve_only_the_current_worktrees_pending_merge_message() {
    let folder = scratch("pending-merge");
    let repo = folder.join("repo");
    let worktree = folder.join("review");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["commit", "--allow-empty", "-qm", "base"]);
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "review",
            worktree.to_str().unwrap(),
        ],
    );
    git(&repo, &["commit", "--allow-empty", "-qm", "topic"]);
    git(&worktree, &["merge", "--no-commit", "--no-ff", "main"]);
    let output = Command::new("git")
        .arg("rev-parse")
        .arg("--absolute-git-dir")
        .current_dir(&worktree)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", text(&output));
    let git_dir = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
    // The shortcode and CRLF would expose either form of normalization by --fix.
    let message = ":jigsaw::necktie: Preview environments (#61)\r\n\r\nPR body.\r\n";
    for name in ["MERGE_MSG", "COMMIT_EDITMSG"] {
        let file = git_dir.join(name);
        std::fs::write(&file, message).unwrap();
        let done = gitmoji(&worktree, &["check", "--fix", file.to_str().unwrap()]);
        assert!(done.status.success(), "{}", text(&done));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), message);
        let other_worktree = gitmoji(&repo, &["check", file.to_str().unwrap()]);
        assert!(!other_worktree.status.success());
    }
    std::fs::write(
        worktree.join("gitmoji.toml"),
        "[rules]\nsignature = 'error'\n",
    )
    .unwrap();
    git(&worktree, &["add", "gitmoji.toml"]);
    let caption =
        "[`🧩✨`] main into review: Add previews\n\nArrived from main (1):\n- abc1234 Add previews";
    for name in ["MERGE_MSG", "COMMIT_EDITMSG"] {
        let file = git_dir.join(name);
        std::fs::write(&file, caption).unwrap();
        let unsigned = gitmoji(&worktree, &["check", "--fix", file.to_str().unwrap()]);
        assert!(!unsigned.status.success(), "{}", text(&unsigned));
        let diagnostic = text(&unsigned);
        assert!(
            diagnostic.contains("merge is already pending"),
            "{diagnostic}"
        );
        assert!(diagnostic.contains("git commit --file="), "{diagnostic}");
        assert!(diagnostic.contains("For future merges"), "{diagnostic}");
        assert!(diagnostic.contains("git merge --no-commit"), "{diagnostic}");
        assert!(
            !diagnostic.contains("gitmoji guide message"),
            "{diagnostic}"
        );
    }
    let draft = worktree.join("COMMIT_EDITMSG");
    std::fs::write(&draft, message).unwrap();
    assert!(
        !gitmoji(&worktree, &["check", draft.to_str().unwrap()])
            .status
            .success()
    );
    assert!(
        !gitmoji(
            &worktree,
            &["check", "-m", "[`🎨✨`] Preview environments (#61)"]
        )
        .status
        .success()
    );
    git(&worktree, &["merge", "--abort"]);
    let file = git_dir.join("COMMIT_EDITMSG");
    std::fs::write(&file, message).unwrap();
    assert!(
        !gitmoji(&worktree, &["check", file.to_str().unwrap()])
            .status
            .success()
    );
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn history_should_count_commits_by_the_version_they_name() {
    let here = scratch("history");
    git(&here, &["init", "-q", "-b", "main"]);
    let vocabulary = String::from_utf8(gitmoji(&here, &["vocabulary"]).stdout).unwrap();
    let current = format!("△ `{}`", vocabulary.trim());
    for message in [
        "old style commit".to_string(),
        format!("🧩👔 #1 (server): a\n\nWhy.\n\n{current}"),
        "🧩👔 #2 (server): b\n\nWhy.\n\n△ `1.0.0`".to_string(),
        "Merge branch 'side'".to_string(),
    ] {
        git(
            &here,
            &[
                "commit",
                "-q",
                "--allow-empty",
                "--no-verify",
                "-m",
                &message,
            ],
        );
    }
    let all = gitmoji(&here, &["history"]);
    let said = text(&all);
    assert!(!all.status.success(), "the old style commit fails: {said}");
    assert!(said.contains("1 of the 3 commits"), "{said}");
    assert!(
        said.contains("1 commit matching a recognized Git subject"),
        "{said}"
    );
    let open = gitmoji(&here, &["history", "--unmigrated"]);
    let said = text(&open);
    assert!(said.contains("1 of the 2 commits"), "{said}");
    let range = gitmoji(&here, &["history", "HEAD~2..HEAD~1"]);
    assert!(range.status.success(), "{}", text(&range));
    let none = gitmoji(&here, &["history", "nonsense..HEAD"]);
    assert!(!none.status.success());
    std::fs::remove_dir_all(here).unwrap();
}

#[test]
fn history_recognizes_a_merge_by_its_parents_even_in_a_shallow_clone() {
    let repo = scratch("merge-parents");
    let title = "[`🎨✨`] Preview environments (#61)";
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["commit", "--allow-empty", "-qm", "root"]);
    git(&repo, &["checkout", "-qb", "topic"]);
    git(&repo, &["commit", "--allow-empty", "-qm", title]);
    git(&repo, &["checkout", "-q", "main"]);
    git(&repo, &["merge", "--no-ff", "-qm", title, "topic"]);

    for args in [
        &["history", "HEAD^!"][..],
        &["history", "--unmigrated", "HEAD^!"][..],
    ] {
        let merged = gitmoji(&repo, args);
        assert!(merged.status.success(), "{}", text(&merged));
        assert!(text(&merged).contains("0 checked"));
        assert!(text(&merged).contains("1 commit with multiple parents"));
    }
    // The topic commit has the same title and a single parent, just like a squash.
    std::fs::write(repo.join("gitmoji.toml"), "[rules]\nuntagged = \"warn\"\n").unwrap();
    let ordinary = gitmoji(&repo, &["history", "HEAD^2^!"]);
    assert!(!ordinary.status.success());
    assert!(text(&ordinary).contains("1 of the 1 commit"));

    let shallow = scratch("merge-parents-shallow");
    git(
        &shallow,
        &[
            "clone",
            "-q",
            "--depth=1",
            "--no-local",
            repo.to_str().unwrap(),
            ".",
        ],
    );
    let merged = gitmoji(&shallow, &["history"]);
    assert!(merged.status.success(), "{}", text(&merged));
    assert!(text(&merged).contains("1 commit with multiple parents"));
    std::fs::remove_dir_all(shallow).unwrap();
    std::fs::remove_dir_all(repo).unwrap();
}

#[test]
fn stored_dependabot_upgrades_pass_checks_and_get_one_caption() {
    let repo = tempfile::tempdir().unwrap();
    let here = repo.path();
    git(here, &["init", "-q"]);
    git(here, &["commit", "--allow-empty", "-m", "base"]);
    let bot = "dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>";
    for (stored, title, expected) in [
        // A bare U+2B06 in the bot prefix preserves Unicode in the signed Git object.
        (
            "🧑‍💻⬆ — (deps): Bump two packages",
            "🧑‍💻⬆ — (deps): Bump two packages",
            "[`🧑‍💻⬆️`] Bump two packages",
        ),
        (
            "👷⬆ — (ci): Bump one action",
            "👷⬆️ — (ci): Bump one action in the release workflow",
            "[`👷⬆️`] Bump one action in the release workflow",
        ),
        (
            "🧩:arrow_up: — (deps): Bump ms",
            "🧩⬆ — (deps): Bump ms",
            "[`🧩⬆️`] Bump ms",
        ),
        (
            "🧩:arrow_up: — (deps): Bump ms from 2.1.0 to 2.1.3",
            "🧩⬆️ — (deps): Bump ms from 2.1.0 to 2.1.3",
            "[`🧩⬆️`] Bump ms from 2.1.0 to 2.1.3",
        ),
        (
            "🧩:arrow_up: — (deps): Bump ms",
            "🧩⬆️ — (deps): Bump ms from 2.1.0 to 2.1.3 in /applications/customer-services/reporting",
            "[`🧩⬆️`] Bump ms from 2.1.0 to 2.1.3 in /applications/customer-services/reporting",
        ),
        (
            ":construction_worker::arrow_up: — (ci): Bump the actions group with 2 updates",
            "👷⬆️ — (ci): Bump the actions group with 2 updates",
            "[`👷⬆️`] Bump the actions group with 2 updates",
        ),
    ] {
        git(
            here,
            &["commit", "--allow-empty", "--author", bot, "-m", stored],
        );
        let check = gitmoji(here, &["history", "HEAD^..HEAD"]);
        assert!(check.status.success(), "{}", text(&check));
        for input in [title, expected] {
            let rendered = gitmoji(
                here,
                &[
                    "merge-caption",
                    "--pr",
                    "1",
                    "--title",
                    input,
                    "HEAD^",
                    "HEAD",
                ],
            );
            assert_eq!(text(&rendered).trim(), expected);
        }
        let original = Command::new("git")
            .args(["show", "-s", "--format=format:%s", "HEAD"])
            .current_dir(here)
            .output()
            .unwrap();
        assert!(original.status.success());
        assert_eq!(original.stdout, stored.as_bytes());
    }
    let bare = gitmoji(here, &["check", "-m", "👷⬆ — (ci): Bump one action"]);
    assert!(bare.status.success(), "{}", text(&bare));
    let raw = ":construction_worker::arrow_up: — (ci): Bump the actions group with 2 updates";
    let edited = gitmoji(
        here,
        &[
            "merge-caption",
            "--pr",
            "1",
            "--title",
            "Adopt the runner update",
            "HEAD^",
            "HEAD",
        ],
    );
    assert_eq!(text(&edited).trim(), "[`👷⬆️`] Adopt the runner update");
    let custom = "👷⬆️ — (billing): Review the invoice worker";
    let rendered = gitmoji(
        here,
        &[
            "merge-caption",
            "--pr",
            "1",
            "--title",
            custom,
            "HEAD^",
            "HEAD",
        ],
    );
    assert_eq!(text(&rendered).trim(), format!("[`👷⬆️`] {custom}"));
    assert!(!gitmoji(here, &["check", "-m", raw]).status.success());
    git(here, &["commit", "--allow-empty", "-m", raw]);
    assert!(!gitmoji(here, &["history", "HEAD^..HEAD"]).status.success());
    git(
        here,
        &[
            "commit",
            "--allow-empty",
            "--author",
            "dependabot[bot] <contributor@example.invalid>",
            "-m",
            raw,
        ],
    );
    assert!(!gitmoji(here, &["history", "HEAD^..HEAD"]).status.success());
    let unrelated = gitmoji(
        here,
        &[
            "merge-caption",
            "--pr",
            "1",
            "--title",
            "A reviewer title",
            "HEAD^",
            "HEAD",
        ],
    );
    assert_eq!(text(&unrelated).trim(), "A reviewer title");
}

#[test]
fn log_exits_quietly_when_its_reader_has_closed_the_pipe() {
    let repo = tempfile::tempdir().unwrap();
    git(repo.path(), &["init", "-q"]);
    git(
        repo.path(),
        &["commit", "--allow-empty", "-qm", "🔖 release fixture"],
    );
    let (reader, writer) = std::io::pipe().unwrap();
    drop(reader);
    let result = Command::new(env!("CARGO_BIN_EXE_gitmoji"))
        .args(["log", "🔖"])
        .current_dir(repo.path())
        .stdout(writer)
        .output()
        .unwrap();
    assert!(result.status.success(), "{}", text(&result));
    assert!(result.stderr.is_empty());
}

#[cfg(target_os = "linux")]
#[test]
fn log_reports_a_write_failure_to_the_user() {
    let repo = tempfile::tempdir().unwrap();
    git(repo.path(), &["init", "-q"]);
    git(
        repo.path(),
        &["commit", "--allow-empty", "-qm", "🔖 release fixture"],
    );
    let full = std::fs::File::options()
        .write(true)
        .open("/dev/full")
        .unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_gitmoji"))
        .args(["log", "🔖"])
        .current_dir(repo.path())
        .stdout(full)
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(1), "{}", text(&result));
    assert!(text(&result).contains("cannot write gitmoji log to stdout"));
}

#[test]
fn log_finds_each_authored_slot_and_preserves_the_original_subjects() {
    let repo = tempfile::tempdir().unwrap();
    let here = repo.path();
    git(here, &["init", "-q", "-b", "main"]);
    let empty = gitmoji(here, &["log", "⬆"]);
    assert!(empty.status.success() && empty.stdout.is_empty());
    assert!(!gitmoji(here, &["log", "⬆", "missing"]).status.success());
    for bad in ["nonsense", "🧩⬆️"] {
        assert!(!gitmoji(here, &["log", bad]).status.success());
    }

    let commit = |subject: &str| git(here, &["commit", "--allow-empty", "-qm", subject]);
    commit("🔖 release 1.0");
    git(here, &["tag", "base"]);
    commit("🛰⬆💥  #1 (api): retain ⬆ and :arrow_up: in text");
    commit("👷⬆️ — (actions): update action");
    git(here, &["checkout", "-qb", "bot"]);
    let stored = ":artificial_satellite::arrow_up: — (deps): update client";
    git(
        here,
        &[
            "commit",
            "--allow-empty",
            "--author",
            "dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>",
            "-qm",
            stored,
        ],
    );
    git(here, &["checkout", "-q", "main"]);
    // A merge may even have an authored-looking subject. It is still an integration.
    git(
        here,
        &[
            "merge",
            "--no-ff",
            "bot",
            "-qm",
            "🛰⬆ — (deps): combine work",
        ],
    );
    commit(stored);
    commit("🛰🛡 #1 (api): mention an ⬆ in the summary");

    let subjects = |filter: &str, range: &str| {
        let done = gitmoji(here, &["log", filter, range]);
        assert!(done.status.success(), "{}", text(&done));
        assert!(done.stderr.is_empty());
        String::from_utf8(done.stdout)
            .unwrap()
            .lines()
            .map(|line| {
                let (hash, subject) = line.split_once(' ').unwrap();
                assert_eq!(hash.len(), 9);
                subject.to_owned()
            })
            .collect::<Vec<_>>()
    };
    let expected = [
        "🛰️⬆️ — (deps): update client",
        "👷⬆️ — (actions): update action",
        "🛰️⬆️💥  #1 (api): retain ⬆ and :arrow_up: in text",
    ];
    for filter in ["⬆", "⬆️", ":arrow_up:"] {
        assert_eq!(subjects(filter, "base..HEAD"), expected);
    }
    assert_eq!(subjects("💥", "HEAD"), [expected[2]]);
    assert_eq!(subjects("🛰", "bot^..bot"), [expected[0]]);
    assert_eq!(subjects("🔖", "HEAD"), ["🔖 release 1.0"]);
    assert!(subjects("🔖", "base..HEAD").is_empty());

    commit("🛰⬆ NO-TICKET (api): retain the ⬆ in prose");
    let failing = gitmoji(here, &["history", "HEAD^..HEAD"]);
    assert!(!failing.status.success());
    assert!(text(&failing).contains("🛰️⬆️ NO-TICKET (api): retain the ⬆ in prose"));

    let raw = Command::new("git")
        .args(["show", "-s", "--format=format:%s", "bot"])
        .current_dir(here)
        .output()
        .unwrap();
    assert_eq!(raw.stdout, stored.as_bytes());
}

#[test]
fn staged_groups_presentation_variants_and_counts_each_file_once() {
    let repo = tempfile::tempdir().unwrap();
    let here = repo.path();
    git(here, &["init", "-q"]);
    std::fs::write(
        here.join("gitmoji.toml"),
        "[hints]\n'api/' = '🛰'\n'*.rs' = '🛰️'\n",
    )
    .unwrap();
    std::fs::create_dir(here.join("api")).unwrap();
    for file in ["api/client.rs", "lib.rs"] {
        std::fs::write(here.join(file), "").unwrap();
    }
    git(here, &["add", "api", "lib.rs", "gitmoji.toml"]);
    let done = gitmoji(here, &["staged"]);
    assert!(done.status.success(), "{}", text(&done));
    let result = text(&done);
    assert!(result.contains("🛰️ (2)"), "{result}");
    assert!(!result.contains("More than one area"), "{result}");
}

#[test]
fn merge_caption_reads_the_changes_arriving_on_the_actual_target() {
    let repo = scratch("caption-arriving");
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["commit", "--allow-empty", "-qm", "base"]);
    git(&repo, &["checkout", "-qb", "preview"]);
    git(
        &repo,
        &[
            "commit",
            "--allow-empty",
            "-qm",
            "🧱🛂 #1 (infra): scope previews",
        ],
    );
    git(&repo, &["checkout", "-qb", "ui"]);
    git(
        &repo,
        &[
            "commit",
            "--allow-empty",
            "-qm",
            "🎨✨ #1 (ui): show previews",
        ],
    );
    git(&repo, &["checkout", "-q", "main"]);
    git(
        &repo,
        &[
            "commit",
            "--allow-empty",
            "-qm",
            "👷🔥 #4 (ci): remove old jobs",
        ],
    );
    git(&repo, &["checkout", "-q", "ui"]);
    let inner = gitmoji(
        &repo,
        &[
            "merge-caption",
            "--pr",
            "2",
            "--title",
            "Show previews",
            "preview",
            "ui",
        ],
    );
    assert!(inner.status.success(), "{}", text(&inner));
    assert_eq!(
        String::from_utf8(inner.stdout).unwrap(),
        "[`🎨✨`] Show previews\n"
    );
    // The sync and inner merge have authored-looking headings. Neither is an authored commit.
    git(
        &repo,
        &[
            "merge",
            "--no-ff",
            "-qm",
            "👷⚡️ #1 (ci): sync from trunk",
            "main",
        ],
    );
    git(&repo, &["checkout", "-q", "preview"]);
    git(
        &repo,
        &[
            "merge",
            "--no-ff",
            "-qm",
            "🤖📝 #2 (agent): merge the UI",
            "ui",
        ],
    );
    let outer = gitmoji(
        &repo,
        &[
            "merge-caption",
            "--pr",
            "3",
            "--title",
            "--preview `show` $(literal)",
            "main",
            "preview",
        ],
    );
    assert!(outer.status.success(), "{}", text(&outer));
    assert_eq!(
        String::from_utf8(outer.stdout).unwrap(),
        "[`🧱🎨` / `🛂✨`] --preview `show` $(literal)\n"
    );
    std::fs::remove_dir_all(repo).unwrap();
}

#[test]
fn caption_coverage_follows_the_physical_pr_history() {
    let repo = scratch("merge-coverage");
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["commit", "--allow-empty", "-qm", "base"]);
    git(&repo, &["tag", "base"]);
    git(
        &repo,
        &[
            "commit",
            "--allow-empty",
            "-qm",
            "🎨✨ #1 (ui): show preview",
        ],
    );
    git(
        &repo,
        &[
            "commit",
            "--allow-empty",
            "-qm",
            "Documentation from before adoption",
        ],
    );
    git(&repo, &["tag", "incoming"]);
    git(
        &repo,
        &[
            "commit",
            "--allow-empty",
            "-qm",
            "🧱🛂 #1 (infra): unrelated policy",
        ],
    );
    git(&repo, &["replace", "incoming", "HEAD"]);
    let tip = Command::new("git")
        .args(["rev-parse", "incoming"])
        .current_dir(&repo)
        .output()
        .unwrap();
    std::fs::write(repo.join(".git/info/grafts"), tip.stdout).unwrap();
    let result = gitmoji(
        &repo,
        &[
            "merge-caption",
            "--json",
            "--pr",
            "7",
            "--title",
            "Preview",
            "base",
            "incoming",
        ],
    );
    assert!(result.status.success(), "{}", text(&result));
    let expected = serde_json::json!({
        "subject": "[`🎨✨`] Preview (#7)", "title": "[`🎨✨`] Preview", "recognized": 1, "total": 2
    });
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&result.stdout).unwrap(),
        expected
    );
    std::fs::remove_dir_all(repo).unwrap();
}

#[test]
fn history_checks_a_merge_verb_and_requires_a_revision_argument() {
    let repo = scratch("authored-merge-verb");
    git(&repo, &["init", "-q"]);
    git(
        &repo,
        &[
            "commit",
            "--allow-empty",
            "-qm",
            "Merge duplicate onboarding forms",
        ],
    );
    let result = gitmoji(&repo, &["history"]);
    assert!(!result.status.success());
    assert!(text(&result).contains("Merge duplicate onboarding forms"));
    assert!(
        !gitmoji(&repo, &["history", "--", "--no-walk"])
            .status
            .success()
    );
    std::fs::remove_dir_all(repo).unwrap();
}

#[test]
fn merge_caption_rejects_ambiguous_or_incomplete_inputs() {
    let repo = scratch("caption-inputs");
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["commit", "--allow-empty", "-qm", "initial"]);
    git(
        &repo,
        &[
            "commit",
            "--allow-empty",
            "-qm",
            "🎨✨ #1 (ui): show previews",
        ],
    );
    for args in [
        ["--pr", "3", "--title", "Title", "--", "main", "--all"],
        [
            "--pr",
            "3",
            "--title",
            "Title",
            "--",
            "main",
            "--output=side-effect",
        ],
        ["--pr", "3", "--title", "Title", "--", "missing", "main"],
        ["--pr", "0", "--title", "Title", "--", "main~1", "main"],
        [
            "--pr",
            "3",
            "--title",
            "Line one\nLine two",
            "--",
            "main~1",
            "main",
        ],
    ] {
        let mut cmd = vec!["merge-caption"];
        cmd.extend(args);
        let result = gitmoji(&repo, &cmd);
        assert!(!result.status.success(), "{cmd:?} unexpectedly succeeded");
        assert!(result.stdout.is_empty());
    }
    assert!(!repo.join("side-effect").exists());
    let shallow = scratch("caption-shallow");
    git(
        &shallow,
        &[
            "clone",
            "--no-local",
            "--depth=1",
            "-q",
            repo.to_str().unwrap(),
            ".",
        ],
    );
    let output = gitmoji(
        &shallow,
        &[
            "merge-caption",
            "--pr",
            "3",
            "--title",
            "Title",
            "HEAD",
            "HEAD",
        ],
    );
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(text(&output).contains("unshallow"), "{}", text(&output));
    std::fs::remove_dir_all(shallow).unwrap();
    std::fs::remove_dir_all(repo).unwrap();
}

#[test]
fn wider_inventory_is_used_only_when_the_incoming_graph_contains_a_merge() {
    let repo = scratch("nested-limit");
    git(&repo, &["init", "-q", "-b", "trunk"]);
    git(&repo, &["commit", "--allow-empty", "-qm", "root"]);
    git(&repo, &["checkout", "-qb", "old"]);
    git(&repo, &["commit", "--allow-empty", "-qm", "old feature"]);
    git(&repo, &["checkout", "-q", "trunk"]);
    git(&repo, &["merge", "--no-ff", "-qm", "existing merge", "old"]);
    git(&repo, &["checkout", "-qb", "topic"]);
    let activities = [
        "✨", "📝", "🔧", "♻️", "🐛", "☑️", "🔊", "🔥", "🔌", "🛡️", "👔", "💾", "🔐",
    ];
    for symbol in activities {
        git(
            &repo,
            &[
                "commit",
                "--allow-empty",
                "-qm",
                &format!("🧩{symbol} #1 (read): a decision"),
            ],
        );
    }
    let read = |title: &str| {
        let r = gitmoji(
            &repo,
            &[
                "merge-caption",
                "--json",
                "--pr",
                "17",
                "--title",
                title,
                "trunk",
                "HEAD",
            ],
        );
        assert!(r.status.success(), "{}", text(&r));
        serde_json::from_slice::<serde_json::Value>(&r.stdout).unwrap()
    };
    let simple = read("Choose");
    assert_eq!(simple["title"], "[`🧩` / `✨📝🔧♻️🐛☑️🔊`…] Choose");
    assert!(simple.get("previous_title").is_none());
    git(&repo, &["checkout", "-qb", "contributor"]);
    git(
        &repo,
        &[
            "commit",
            "--allow-empty",
            "-qm",
            "🧩✨ #1 (read): another decision",
        ],
    );
    git(&repo, &["checkout", "-q", "topic"]);
    git(
        &repo,
        &[
            "merge",
            "--no-ff",
            "-qm",
            "🧩⚡️ must never enter the inventory",
            "contributor",
        ],
    );
    let wide = read(simple["title"].as_str().unwrap());
    let whole = "[`🧩` / `✨📝🔧♻️🐛☑️🔊🔥🔌🛡️👔💾🔐`] Choose";
    assert_eq!(wide["title"], whole);
    assert_eq!(wide["previous_title"], simple["title"]);
    assert_eq!(wide["total"], 14);
    git(
        &repo,
        &["commit", "--allow-empty", "-qm", "🧩🥅 #1 (read): one more"],
    );
    let overflow = read(whole);
    assert_eq!(
        overflow["title"],
        "[`🧩` / `✨📝🔧♻️🐛☑️🔊🔥🔌🛡️👔💾`…] Choose"
    );
    assert_eq!(read(overflow["title"].as_str().unwrap()), overflow);
    std::fs::remove_dir_all(repo).unwrap();
}

#[test]
fn branch_caption_uses_parents_and_body_even_when_it_names_an_arriving_pr() {
    let repo = scratch("branch-caption-classification");
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["commit", "--allow-empty", "-qm", "root"]);
    git(&repo, &["checkout", "-qb", "side"]);
    let msg = "[`🧩✨`] side into main: Add previews (#42)\n\nArrived from side (1):\n- abc1234 Add previews (#42)";
    git(&repo, &["commit", "--allow-empty", "-qm", msg]);
    git(&repo, &["checkout", "-q", "main"]);
    git(&repo, &["merge", "--no-ff", "-qm", msg, "side"]);
    for args in [
        &["history", "HEAD^!"][..],
        &["explain", "--commit", "HEAD"][..],
    ] {
        let r = gitmoji(&repo, args);
        assert!(r.status.success(), "{}", text(&r));
        assert!(text(&r).contains("local branch caption"), "{}", text(&r));
    }
    for args in [
        &["history", "side^!"][..],
        &["explain", "--commit", "side"][..],
    ] {
        assert!(!gitmoji(&repo, args).status.success());
    }
    std::fs::write(repo.join("gitmoji.toml"), "[rules]\nsignature = 'error'\n").unwrap();
    assert!(!gitmoji(&repo, &["history", "HEAD^!"]).status.success());

    git(&repo, &["checkout", "-q", "side"]);
    git(&repo, &["commit", "--allow-empty", "-qm", "next"]);
    git(&repo, &["checkout", "-q", "main"]);
    git(&repo, &["merge", "--no-ff", "--no-commit", "side"]);
    git(&repo, &["add", "gitmoji.toml"]);
    let draft = repo.join(".git/COMMIT_EDITMSG");
    std::fs::write(&draft, msg).unwrap();
    let unsigned = gitmoji(&repo, &["check", "--fix", draft.to_str().unwrap()]);
    assert!(!unsigned.status.success(), "{}", text(&unsigned));
    assert!(text(&unsigned).contains("signing-a-local-merge-from-an-agent-terminal"));
    assert!(
        text(&unsigned).contains("gitmoji signature"),
        "{}",
        text(&unsigned)
    );

    std::fs::write(
        &draft,
        format!("{msg}\n\n_\nexample-model (high)\nExample Harness\n"),
    )
    .unwrap();
    let signed = gitmoji(&repo, &["check", "--fix", draft.to_str().unwrap()]);
    assert!(signed.status.success(), "{}", text(&signed));
    assert!(
        std::fs::read_to_string(&draft)
            .unwrap()
            .contains("example-model (high)")
    );
    std::fs::remove_dir_all(repo).unwrap();
}

#[test]
fn checking_a_merge_caption_as_text_asks_for_the_stored_commit() {
    let repo = scratch("merge-caption-text");
    // A permissive author policy must not turn a guessed integration into a success.
    std::fs::write(repo.join("gitmoji.toml"), "[rules]\nuntagged = 'warn'\n").unwrap();
    for subject in [
        "[`🧩🐛`] trunk into topic: #42 Fix the shared value",
        "[`🧩` / `✨🐛`] team/a, team/b into staging: 2 commits",
        "fetched trunk into trunk: 2 commits",
        "'#12-source' into topic: Legacy change",
    ] {
        let output = gitmoji(&repo, &["check", "-m", subject]);
        let said = text(&output);
        assert!(!output.status.success(), "{subject}");
        assert!(said.contains("explain --commit <sha>"), "{said}");
        assert!(said.contains("history <range>"), "{said}");
        assert!(!said.contains("backticks"), "{said}");
    }
    for subject in [
        "[WIP] a into b: maybe",
        "[`🧩🐛`] Move two drafts into trunk: the plan",
        "integrate into two branches: preview",
        "🧩✨ #42 (app): move receipts into storage: retain a copy",
    ] {
        let output = gitmoji(&repo, &["check", "-m", subject]);
        assert!(!text(&output).contains("explain --commit"), "{subject}");
    }
    std::fs::remove_dir_all(repo).unwrap();
}

#[test]
fn pr_title_explains_the_actions_context_and_local_preview() {
    let binary = env!("CARGO_BIN_EXE_gitmoji");
    let help = Command::new(binary)
        .args(["pr-title", "--help"])
        .output()
        .unwrap();
    assert!(help.status.success());
    let help = String::from_utf8(help.stdout).unwrap();
    for name in [
        "GITHUB_SERVER_URL",
        "GITHUB_API_URL",
        "GITHUB_REPOSITORY",
        "GITMOJI_TITLE_TOKEN",
    ] {
        assert!(help.contains(name), "{help}");
    }
    for value in [None, Some(""), Some("   ")] {
        let mut cmd = Command::new(binary);
        cmd.args(["pr-title", "--pr", "42"])
            .env_remove("GITHUB_SERVER_URL");
        if let Some(value) = value {
            cmd.env("GITHUB_SERVER_URL", value);
        }
        let result = cmd.output().unwrap();
        assert!(!result.status.success());
        let diagnostic = String::from_utf8(result.stderr).unwrap();
        assert!(
            diagnostic.contains("GITHUB_SERVER_URL is missing or empty"),
            "{diagnostic}"
        );
        for message in [&help, &diagnostic] {
            assert!(message.contains("updates the title on GitHub"));
            assert!(message.contains("read-only preview"));
            assert!(
                message.contains(
                    "gitmoji github-merge --repo OWNER/REPO --pr NUMBER [--hostname HOST]"
                )
            );
        }
    }
}

#[test]
fn staged_policy_follows_only_the_proposed_tree_from_any_directory() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    git(repo, &["init", "-q", "-b", "trunk"]);
    let root = repo.join("gitmoji.toml");
    let nested = repo.join("web");
    for folder in ["web", "api", ".config"] {
        std::fs::create_dir(repo.join(folder)).unwrap();
    }
    for path in ["web/view", "api/route"] {
        std::fs::write(repo.join(path), "initial").unwrap();
    }
    let strict = "[rules]\none_scope = 'error'\n[paths]\nweb = ['web/']\napi = ['api/']\n";
    let warn = strict.replace("'error'", "'warn'");
    std::fs::write(&root, strict).unwrap();
    std::fs::write(repo.join(".config/gitmoji.toml"), &warn).unwrap();
    git(repo, &["add", "api", "web"]);
    git(repo, &["add", "-N", "gitmoji.toml"]);
    let staged = || gitmoji(&nested, &["staged"]);
    let defaults = staged();
    assert!(defaults.status.success(), "{}", text(&defaults));
    assert!(!text(&defaults).contains("span 2 scopes"));
    git(repo, &["add", ".config/gitmoji.toml"]);
    let warning = staged();
    assert!(warning.status.success(), "{}", text(&warning));
    assert!(
        text(&warning).contains("span 2 scopes"),
        "{}",
        text(&warning)
    );
    git(repo, &["add", "gitmoji.toml"]);
    std::fs::write(&root, &warn).unwrap();
    let refusal = staged();
    assert!(!refusal.status.success(), "{}", text(&refusal));
    assert!(
        text(&refusal).contains("span 2 scopes"),
        "{}",
        text(&refusal)
    );
    let local = gitmoji(&nested, &["--config", "../gitmoji.toml", "staged"]);
    assert!(local.status.success(), "{}", text(&local));
    assert!(text(&local).contains("span 2 scopes"));
    git(repo, &["commit", "-qm", "adopt strict policy"]);
    for path in ["web/view", "api/route"] {
        std::fs::write(repo.join(path), "changed").unwrap();
    }
    git(repo, &["add", "api", "web"]);
    git(repo, &["rm", "--cached", "gitmoji.toml"]);
    std::fs::write(&root, "invalid toml!").unwrap();
    let fallback = staged();
    assert!(fallback.status.success(), "{}", text(&fallback));
    assert!(text(&fallback).contains("span 2 scopes"));
    git(repo, &["add", "gitmoji.toml"]);
    std::fs::write(&root, warn).unwrap();
    let invalid = staged();
    assert!(!invalid.status.success());
    assert!(
        text(&invalid).contains("cannot read staged gitmoji.toml"),
        "{}",
        text(&invalid)
    );
}

#[test]
fn message_hook_uses_the_index_git_prepared_for_a_partial_commit() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    git(repo, &["init", "-q", "-b", "trunk"]);
    git(repo, &["config", "core.hooksPath", ".git/hooks"]);
    let root = repo.join("gitmoji.toml");
    std::fs::write(&root, "[rules]\nuntagged = 'error'\n").unwrap();
    std::fs::write(repo.join("work"), "first").unwrap();
    git(repo, &["add", "."]);
    git(repo, &["commit", "-qm", "seed"]);
    for (name, command) in [
        ("pre-commit", "gitmoji staged"),
        ("commit-msg", "gitmoji check --fix \"$1\""),
    ] {
        let hook = repo.join(".git/hooks").join(name);
        std::fs::write(&hook, format!("#!/bin/sh\nexec {command}\n")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }
    std::fs::write(&root, "[rules]\nuntagged = 'warn'\n").unwrap();
    git(repo, &["add", "gitmoji.toml"]);
    std::fs::write(repo.join("work"), "second").unwrap();
    let mut paths = vec![
        PathBuf::from(env!("CARGO_BIN_EXE_gitmoji"))
            .parent()
            .unwrap()
            .to_path_buf(),
    ];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let path = std::env::join_paths(paths).unwrap();
    let commit = |target| {
        Command::new("git")
            .args([
                "-c",
                "user.name=T",
                "-c",
                "user.email=t@example.invalid",
                "commit",
                "-qm",
                "plain subject",
                "--only",
                "--",
                target,
            ])
            .current_dir(repo)
            .env("PATH", &path)
            .output()
            .unwrap()
    };
    let work = commit("work");
    assert!(!work.status.success(), "{}", text(&work));
    assert!(text(&work).contains("gitmoji:"), "{}", text(&work));
    let policy = commit("gitmoji.toml");
    assert!(policy.status.success(), "{}", text(&policy));
    let draft = repo.join("draft.txt");
    std::fs::write(&draft, "plain subject").unwrap();
    std::fs::write(&root, "[rules]\nuntagged = 'error'\n").unwrap();
    assert!(
        !gitmoji(repo, &["check", draft.to_str().unwrap()])
            .status
            .success()
    );
    assert!(
        !gitmoji(repo, &["check", "-m", "plain subject"])
            .status
            .success()
    );
}

#[cfg(unix)]
#[test]
fn staged_policy_follows_links_only_inside_the_proposed_tree() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "trunk"]);
    std::fs::create_dir(repo.join("policy")).unwrap();
    let settings = repo.join("policy/gitmoji.toml");
    let root = repo.join("gitmoji.toml");
    std::fs::write(&settings, "invalid TOML").unwrap();
    symlink("policy/gitmoji.toml", &root).unwrap();
    git(&repo, &["add", "."]);
    std::fs::write(&settings, "").unwrap();
    let staged = || gitmoji(&repo, &["staged"]);
    let invalid = staged();
    assert!(!invalid.status.success());
    assert!(text(&invalid).contains("cannot read staged gitmoji.toml"));
    git(&repo, &["add", "policy/gitmoji.toml"]);
    assert!(staged().status.success());

    std::fs::remove_file(&root).unwrap();
    std::fs::write(dir.path().join("outside.toml"), "").unwrap();
    symlink("../outside.toml", &root).unwrap();
    git(&repo, &["add", "gitmoji.toml"]);
    let external = staged();
    assert!(!external.status.success());
    assert!(text(&external).contains("inside the repository"));
}

#[test]
fn unresolved_policy_requires_staging_a_resolution() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    git(repo, &["init", "-q", "-b", "trunk"]);
    let root = repo.join("gitmoji.toml");
    std::fs::write(&root, "[rules]\none_scope = 'off'\n").unwrap();
    git(repo, &["add", "gitmoji.toml"]);
    git(repo, &["commit", "-qm", "base"]);
    git(repo, &["checkout", "-qb", "other"]);
    std::fs::write(&root, "[rules]\none_scope = 'warn'\n").unwrap();
    git(repo, &["commit", "-qam", "warn"]);
    git(repo, &["checkout", "-q", "trunk"]);
    std::fs::write(&root, "[rules]\none_scope = 'error'\n").unwrap();
    git(repo, &["commit", "-qam", "enforce"]);
    let merge = Command::new("git")
        .current_dir(repo)
        .args([
            "-c",
            "user.name=T",
            "-c",
            "user.email=t@example.invalid",
            "merge",
            "--no-commit",
            "other",
        ])
        .output()
        .unwrap();
    assert_eq!(merge.status.code(), Some(1), "{}", text(&merge));
    std::fs::write(&root, "[rules]\none_scope = 'warn'\n").unwrap();
    let unresolved = gitmoji(repo, &["staged"]);
    assert!(!unresolved.status.success());
    assert!(text(&unresolved).contains("cannot read the staged tree"));
    git(repo, &["add", "gitmoji.toml"]);
    assert!(gitmoji(repo, &["staged"]).status.success());
}

#[test]
fn staged_scope_counts_a_path_that_changed_type() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    git(repo, &["init", "-q", "-b", "trunk"]);
    std::fs::write(
        repo.join("gitmoji.toml"),
        "[rules]\none_scope = 'error'\n[paths]\nassets = ['icon']\napp = ['view']\n",
    )
    .unwrap();
    std::fs::write(repo.join("icon"), "view").unwrap();
    std::fs::write(repo.join("view"), "one").unwrap();
    git(repo, &["add", "."]);
    git(repo, &["commit", "-qm", "seed"]);
    std::fs::write(repo.join("view"), "two").unwrap();
    git(repo, &["add", "view"]);
    // Mode 120000 stages a symlink, including on filesystems without symlink support.
    let blob = Command::new("git")
        .current_dir(repo)
        .args(["rev-parse", "HEAD:icon"])
        .output()
        .unwrap();
    assert!(blob.status.success(), "{}", text(&blob));
    let oid = std::str::from_utf8(&blob.stdout).unwrap().trim();
    git(
        repo,
        &["update-index", "--cacheinfo", &format!("120000,{oid},icon")],
    );
    let result = gitmoji(repo, &["staged"]);
    assert!(!result.status.success());
    assert!(
        text(&result).contains("span 2 scopes: app (1), assets (1)"),
        "{}",
        text(&result)
    );
}

#[test]
fn directory_and_submodule_entries_do_not_hide_the_next_policy_file() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    git(repo, &["init", "-q", "-b", "trunk"]);
    std::fs::create_dir(repo.join(".config")).unwrap();
    std::fs::write(repo.join(".config/gitmoji.toml"), "bad TOML!").unwrap();
    std::fs::create_dir(repo.join("gitmoji.toml")).unwrap();
    std::fs::write(repo.join("gitmoji.toml/file"), "hello").unwrap();
    git(repo, &["add", "."]);
    let assert_fallback = || {
        let result = gitmoji(repo, &["staged"]);
        assert!(!result.status.success());
        assert!(
            text(&result).contains("cannot read staged .config/gitmoji.toml"),
            "{}",
            text(&result)
        );
    };
    assert_fallback();
    git(repo, &["commit", "-qm", "seed"]);
    git(repo, &["rm", "-qr", "gitmoji.toml"]);
    let head = Command::new("git")
        .current_dir(repo)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert!(head.status.success());
    let oid = std::str::from_utf8(&head.stdout).unwrap().trim();
    git(
        repo,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("160000,{oid},gitmoji.toml"),
        ],
    );
    assert_fallback();
}
