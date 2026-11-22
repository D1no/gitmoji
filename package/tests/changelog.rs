//! Exercise release-note drafting against actual Git trees, parent order and tags.

#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::{Command, Output};
use tempfile::TempDir;

fn run(dir: &Path, program: &str, args: &[&str]) -> Output {
    Command::new(program)
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", dir.join("no-global-config"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .output()
        .unwrap()
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = run(dir, "git", args);
    assert!(out.status.success(), "{args:?}: {out:?}");
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

struct Project {
    dir: TempDir,
    tree: String,
}

impl Project {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let cwd = dir.path();
        git(cwd, &["init", "-q"]);
        git(cwd, &["config", "user.name", "Release fixture"]);
        git(cwd, &["config", "user.email", "test@example.invalid"]);
        let tree = git(cwd, &["write-tree"]);
        Self { dir, tree }
    }

    fn commit(&self, parents: &[&str], message: &str) -> String {
        let mut args = vec!["commit-tree", &self.tree];
        for parent in parents {
            args.extend(["-p", *parent]);
        }
        args.extend(["-m", message]);
        git(self.dir.path(), &args)
    }

    fn changelog(&self, revision: &str) -> Output {
        run(
            self.dir.path(),
            env!("CARGO_BIN_EXE_gitmoji"),
            &["changelog", revision],
        )
    }

    fn markdown(&self, revision: &str) -> String {
        let out = self.changelog(revision);
        assert!(out.status.success(), "{out:?}");
        String::from_utf8(out.stdout).unwrap()
    }
}

#[test]
fn release_draft_follows_pr_parents_through_nested_integrations() {
    let project = Project::new();
    let root = project.commit(&[], "Root");
    let one = project.commit(&[&root], "🧩✨ #17 (service): expose a new contract");
    let first = project.commit(
        &[&root, &one],
        "Merge pull request #20 from example/service\n\nExpose the new contract",
    );

    // Sync trunk back into the UI branch. Its local caption quotes the title of
    // PR #20, which is already reachable from the next PR's destination.
    let ui = project.commit(&[&root], "🎨✨ #31 (ui): show connection details");
    let sync = project.commit(
        &[&ui, &first],
        "[`🧩✨`] trunk into ui: Expose the new contract (#20)\n\nArrived from trunk (1):\n- Expose the new contract (#20)",
    );
    let infra = project.commit(&[&sync], "🧱🏷️💥 #32 (infra): rename the network setting");
    let ci = project.commit(&[&infra], "👷☑️ #32 (ci): check the new version");
    let mixed = project.commit(
        &[&first, &ci],
        "[`🎨✨`] [Preview] Show connection details (#33)\n\nMerge pull request #33 from example/ui",
    );
    let release = project.commit(&[&mixed], "🔖 release 2.0.0");
    let landing = project.commit(&[&mixed, &release], "[`🔖`] Publish 2.0.0 (#44)");
    git(project.dir.path(), &["tag", "v1", &first]);
    git(project.dir.path(), &["tag", "v2", &landing]);
    let people = "## People-facing\n\n- **💥 Breaking** — \\[Preview\\] Show connection details (#33; #31, #32) — also: 🧱 👷\n";
    assert_eq!(project.markdown("v1..v2"), people);
    assert_eq!(
        project.markdown("v2"),
        format!("{people}\n## System\n\n- Expose the new contract (#20; #17)\n")
    );
    assert_eq!(project.markdown("v2..v2"), "");
    assert_eq!(project.markdown(&format!("{release}..v2")), "");

    // Area order, group names, and the winner of a mixed PR all belong to this project.
    std::fs::write(project.dir.path().join("gitmoji.toml"),
        "[[changelog.groups]]\nname='Operators'\nareas=['🧱','👷']\n\n[[changelog.groups]]\nname='Design'\nareas=['🎨']\n").unwrap();
    assert_eq!(
        project.markdown("v1..v2"),
        "## Operators\n\n- **💥 Breaking** — \\[Preview\\] Show connection details (#33; #31, #32) — also: 🎨\n"
    );
}

#[test]
fn nested_pr_lands_via_local_merge_and_a_guessed_issue_is_never_a_pr() {
    let project = Project::new();
    let root = project.commit(&[], "Root");
    let side = project.commit(&[&root], "🧩✨ TASK-79 (retired-scope): add an export");
    let nested = project.commit(
        &[&root, &side],
        "Add a compact export (#5)\n\nMerge pull request #5 from example/export",
    );
    let operation = project.commit(&[&root], "Prepare operation (#999)");
    let landing = project.commit(&[&operation, &nested], "Merge branch 'delivery' into trunk");
    let more = project.commit(&[&landing], "Assorted notes");
    let unverified = project.commit(&[&landing, &more], "Discuss an issue (#999)");
    std::fs::write(
        project.dir.path().join("gitmoji.toml"),
        "[ticket]\npatterns=['TASK-[0-9]+']\n[scope]\nallowed=['current-scope']\n",
    )
    .unwrap();
    assert_eq!(
        project.markdown(&unverified),
        "## System\n\n- Add a compact export (#5; TASK-79)\n"
    );
}

#[test]
fn enclosing_pr_only_reports_its_own_decisions() {
    let project = Project::new();
    let root = project.commit(&[], "Root");
    let internal = project.commit(&[&root], "👷☑️ #61 (ci): verify the migration");
    let ui = project.commit(&[&root], "🎨✨💥 #62 (ui): remove the older dialog");
    let inner = project.commit(&[&root, &ui], "[`🎨✨💥`] Replace the old dialog (#63)");
    let integration = project.commit(&[&internal, &inner], "Merge branch 'ui' into delivery");
    let marker = project.commit(&[&integration], "🔖 #999 release candidate");
    let outer = project.commit(
        &[&root, &marker],
        "[`🎨` / `✨💥`] Verify <migration> (#64)",
    );
    assert_eq!(
        project.markdown(&outer),
        "## People-facing\n\n- **💥 Breaking** — Replace the old dialog (#63; #62)\n\n## Behind the scenes\n\n- Verify &lt;migration&gt; (#64; #61)\n"
    );
}

#[test]
fn groups_the_authored_dependabot_upgrade_without_a_ticket() {
    let project = Project::new();
    let root = project.commit(&[], "Root");
    git(
        project.dir.path(),
        &["config", "user.name", "dependabot[bot]"],
    );
    git(
        project.dir.path(),
        &[
            "config",
            "user.email",
            "49699333+dependabot[bot]@users.noreply.github.com",
        ],
    );
    let update = project.commit(&[&root], "🧩:arrow_up: — (deps): Bump dependencies");
    let landing = project.commit(&[&root, &update], "[`🧩⬆️`] Update dependencies (#7)");
    assert_eq!(
        project.markdown(&landing),
        "## System\n\n- Update dependencies (#7)\n"
    );
}

#[test]
fn bad_ranges_and_invalid_audience_maps_fail_without_a_partial_draft() {
    let project = Project::new();
    let tip = project.commit(&[], "Root");
    for invalid in ["x...y", "missing..", "..trunk", "missing"] {
        let out = project.changelog(invalid);
        assert!(!out.status.success(), "{invalid}");
        assert!(out.stdout.is_empty());
    }
    std::fs::write(
        project.dir.path().join("gitmoji.toml"),
        "[[changelog.groups]]\nname='People'\nareas=['✨']\n",
    )
    .unwrap();
    let out = project.changelog(&tip);
    assert!(!out.status.success());
    assert!(
        String::from_utf8(out.stderr)
            .unwrap()
            .contains("needs an area")
    );
}
