//! Exercise the installed binary's offline history rewrite and candidate guard.

#![allow(clippy::unwrap_used)]

use serde_json::Value;
use std::path::Path;
use std::process::{Command, Output};

fn checked(output: Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .trim_end()
        .to_owned()
}

fn command(program: &str, dir: &Path) -> Command {
    let mut command = Command::new(program);
    command
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", dir.join("empty.gitconfig"));
    command
}

fn git(dir: &Path, args: &[&str]) -> String {
    checked(
        command("git", dir)
            .args([
                "-c",
                "user.name=Tester",
                "-c",
                "user.email=test@example.invalid",
                "-c",
                "commit.gpgSign=false",
                "-c",
                "core.hooksPath=disabled-hooks",
            ])
            .args(args)
            .output()
            .unwrap(),
    )
}
fn upgrade(dir: &Path, args: &[&str]) -> Output {
    command(env!("CARGO_BIN_EXE_gitmoji"), dir)
        .args(["history", "--upgrade", "--local-merges-only"])
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn offline_preview_and_candidate_keep_the_source_graph_intact() {
    let fixture = tempfile::tempdir().unwrap();
    let dir = fixture.path();
    git(dir, &["init", "-q", "-b", "integration"]);
    git(dir, &["commit", "--allow-empty", "-qm", "seed"]);
    git(dir, &["switch", "-qc", "reports"]);
    git(
        dir,
        &[
            "commit",
            "--allow-empty",
            "-qm",
            "🧩✨ TASK-4 (app): export reports",
        ],
    );
    git(dir, &["switch", "-q", "integration"]);
    git(dir, &["merge", "--no-ff", "--no-edit", "reports"]);
    let source = git(dir, &["rev-parse", "HEAD"]);
    let plan: Value = serde_json::from_str(&checked(upgrade(dir, &[]))).unwrap();
    let candidate = plan["candidate_tip"].as_str().unwrap();
    assert_ne!(candidate, source);
    assert_eq!(plan["source_tip"], source);
    assert!(plan["repository"].is_null());
    // Neither the reviewed object nor a branch should exist before applying.
    assert!(
        !command("git", dir)
            .args(["cat-file", "-e", candidate])
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        !upgrade(dir, &["--apply", "review", "--expect-candidate", &source])
            .status
            .success()
    );
    assert!(git(dir, &["branch", "--list", "review"]).is_empty());
    checked(upgrade(
        dir,
        &["--apply", "review", "--expect-candidate", candidate],
    ));
    assert_eq!(git(dir, &["rev-parse", "HEAD"]), source);
    assert_eq!(
        git(dir, &["show", "-s", "--format=%s", "review"]),
        "[`🧩✨`] reports into integration: export reports"
    );
    assert_eq!(
        git(dir, &["rev-parse", "review^{tree}"]),
        git(dir, &["rev-parse", "HEAD^{tree}"])
    );
    let second: Value = serde_json::from_str(&checked(upgrade(dir, &["--ref", "review"]))).unwrap();
    assert_eq!(second["candidate_tip"], candidate);
    assert_eq!(second["review_or_changed_commits"], serde_json::json!([]));
}
