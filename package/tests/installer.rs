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
    let clean = format!("{shared}/scripts/codex.py");
    for path in [&edit, &deletion, &conflict, &clean] {
        fs::write(repo.join(path), "old version\n").unwrap();
    }
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
