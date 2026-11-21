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
