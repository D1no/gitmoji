//! Check the doctor contract through the installed command and real Git worktrees.

#![allow(clippy::unwrap_used)]

use serde::Deserialize;
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::SystemTime;
use tempfile::TempDir;

#[derive(Debug, Deserialize)]
struct Component {
    component: String,
    status: String,
    found: String,
    expected: String,
    remedy: String,
    install: Option<String>,
    warnings: Option<Vec<String>>,
}

struct Project {
    scratch: TempDir,
    root: PathBuf,
    git: PathBuf,
}

impl Project {
    fn new() -> Self {
        let scratch = tempfile::tempdir().unwrap();
        let root = scratch.path().join("project with spaces");
        fs::create_dir(&root).unwrap();
        let git = std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .map(|dir| dir.join(format!("git{}", std::env::consts::EXE_SUFFIX)))
            .find(|path| path.is_file())
            .unwrap();
        let project = Self { scratch, root, git };
        project.git(&["init", "-q"]);
        project
    }

    fn run(&self, cwd: &Path, program: &OsStr, args: &[&str]) -> Output {
        let mut command = Command::new(program);
        command
            .current_dir(cwd)
            .args(args)
            .env("PATH", self.git.parent().unwrap())
            .env("HOME", self.scratch.path())
            .env("USERPROFILE", self.scratch.path())
            .env(
                "GIT_CONFIG_GLOBAL",
                self.scratch.path().join("empty-config"),
            )
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_TERMINAL_PROMPT", "0");
        for key in [
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_COMMON_DIR",
            "GIT_INDEX_FILE",
            "GIT_CONFIG_COUNT",
            "GIT_CONFIG_PARAMETERS",
        ] {
            command.env_remove(key);
        }
        command.output().unwrap()
    }

    fn git(&self, args: &[&str]) {
        let output = self.run(&self.root, self.git.as_os_str(), args);
        assert!(output.status.success(), "{args:?}: {output:?}");
    }

    fn cli(&self, cwd: &Path, args: &[&str]) -> Output {
        self.run(cwd, OsStr::new(env!("CARGO_BIN_EXE_gitmoji")), args)
    }

    fn init(&self, workflows: bool) {
        let mut args = vec!["init"];
        if workflows {
            args.push("--github-workflows");
        }
        let output = self.cli(&self.root, &args);
        assert!(output.status.success(), "{output:?}");
    }

    fn report(&self, cwd: &Path, code: i32) -> BTreeMap<String, Component> {
        let output = self.cli(cwd, &["doctor", "--json"]);
        assert_eq!(output.status.code(), Some(code), "{output:?}");
        let components: Vec<Component> = serde_json::from_slice(&output.stdout).unwrap();
        let mut report = BTreeMap::new();
        for row in components {
            assert!(!row.component.is_empty());
            assert!(matches!(
                row.status.as_str(),
                "ok" | "not-enrolled" | "missing" | "mismatch" | "stale" | "bypassed"
            ));
            if matches!(
                row.status.as_str(),
                "missing" | "mismatch" | "stale" | "bypassed"
            ) {
                assert!(!row.expected.is_empty(), "{row:?}");
                assert!(!row.remedy.is_empty(), "{row:?}");
            }
            assert!(report.insert(row.component.clone(), row).is_none());
        }
        report
    }
}

// Snapshot both the contents and modification times of the tiny fixture, including
// its index and refs. Running git status here could refresh the index itself.
fn snapshot(root: &Path) -> BTreeMap<PathBuf, (SystemTime, Vec<u8>)> {
    let mut result = BTreeMap::new();
    let mut pending = vec![root.to_owned()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if entry.file_type().unwrap().is_dir() {
                pending.push(path);
            } else {
                let modified = entry.metadata().unwrap().modified().unwrap();
                result.insert(path.clone(), (modified, fs::read(&path).unwrap()));
            }
        }
    }
    result
}

#[test]
fn healthy_linked_worktree_reports_local_installation_without_writing_files() {
    let project = Project::new();
    project.init(false);
    project.git(&["add", "."]);
    project.git(&[
        "-c",
        "core.hooksPath=disabled-hooks",
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "-c",
        "commit.gpgsign=false",
        "commit",
        "-qm",
        "Adopt the convention",
    ]);
    let linked = project.scratch.path().join("linked worktree");
    project.git(&[
        "worktree",
        "add",
        "-q",
        "--detach",
        linked.to_str().unwrap(),
        "HEAD",
    ]);
    let nested = linked.join(".claude/skills");
    let before = snapshot(project.scratch.path());
    let report = project.report(&nested, 0);
    for name in [
        "binary",
        "pin",
        "skill",
        "hook:commit-msg",
        "hook:prepare-commit-msg",
    ] {
        assert_eq!(report[name].status, "ok", "{name}: {report:?}");
    }
    assert!(report["binary"].found.contains(env!("CARGO_PKG_VERSION")));
    for name in ["workflow:gitmoji-commits", "workflow:gitmoji-pr-title"] {
        assert_eq!(report[name].status, "not-enrolled", "{report:?}");
    }
    assert_eq!(snapshot(project.scratch.path()), before);

    let hook = project.root.join(".git/hooks/prepare-commit-msg");
    fs::remove_file(&hook).unwrap();
    let after_removal = snapshot(project.scratch.path());
    let report = project.report(&nested, 1);
    assert_eq!(report["hook:prepare-commit-msg"].status, "missing");
    assert_eq!(report["hook:commit-msg"].status, "ok");
    assert_eq!(snapshot(project.scratch.path()), after_removal);
}

#[test]
fn mismatched_pin_is_actionable_without_misreporting_old_release_templates() {
    let project = Project::new();
    project.init(true);
    let pin = project.root.join("gitmoji.toml");
    fs::write(&pin, "[vocabulary]\nversion = '0.0.1'\n").unwrap();
    for name in ["gitmoji-commits", "gitmoji-pr-title"] {
        let path = project.root.join(format!(".github/workflows/{name}.yml"));
        let template = fs::read_to_string(&path).unwrap();
        let matching_pin = template.replace(env!("CARGO_PKG_VERSION"), "0.0.1");
        fs::write(
            path,
            format!("{matching_pin}\n# Template from the pinned release\n"),
        )
        .unwrap();
    }
    let report = project.report(&project.root, 1);
    assert_eq!(report["binary"].expected, "0.0.1");
    assert!(report["binary"].remedy.contains(&format!(
        "/releases/download/v{}/install.sh",
        env!("CARGO_PKG_VERSION")
    )));
    assert!(report["binary"].remedy.ends_with("--pinned"));
    assert_eq!(
        report["binary"].install.as_deref(),
        Some("install.sh --pinned")
    );
    assert!(
        ["binary", "pin"]
            .iter()
            .any(|name| report[*name].status == "mismatch"),
        "{report:?}"
    );
    for name in [
        "workflow:gitmoji-commits",
        "workflow:gitmoji-pr-title",
        "skill",
    ] {
        assert_ne!(report[name].status, "stale", "{report:?}");
    }
}

#[test]
fn a_missing_or_invalid_pin_still_yields_a_component_report() {
    let project = Project::new();
    let report = project.report(&project.root, 1);
    assert_eq!(report["pin"].status, "missing");
    let config = project.root.join(".config");
    fs::create_dir(&config).unwrap();
    let file = config.join("gitmoji.toml");
    fs::write(&file, "# project has no vocabulary pin yet\n").unwrap();
    let report = project.report(&config, 1);
    assert_eq!(report["pin"].status, "missing");
    fs::write(&file, "[vocabulary]\nversion = 'latest'\n").unwrap();
    let report = project.report(&config, 1);
    assert_eq!(report["pin"].status, "mismatch");
    let output = project.cli(&config, &["doctor"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8(output.stdout).unwrap().contains("pin"));
}

#[test]
fn dependabot_prefixes_distinguish_upstream_substitutions_from_invalid_headings() {
    let project = Project::new();
    project.init(false);
    assert!(
        !project
            .report(&project.root, 0)
            .contains_key("dependabot:prefixes")
    );
    let github = project.root.join(".github");
    fs::create_dir_all(&github).unwrap();
    let file = github.join("dependabot.yaml");
    fs::write(&file, "version: 2\nupdates: [{package-ecosystem: cargo}]\n").unwrap();
    let report = project.report(&project.root, 0);
    assert_eq!(report["dependabot:prefixes"].status, "not-enrolled");
    let yaml = r#"version: 2
updates:
  - package-ecosystem: npm
    directory: /
    schedule: { interval: weekly }
    commit-message:
      prefix: "🧑‍💻⬆ — (deps)" # bare Unicode arrow
      prefix-development: "🧑‍💻⬆️ — (deps-dev)"
  - package-ecosystem: cargo
    directory: /
    schedule: { interval: weekly }
    commit-message: {prefix: '🧩⬆ — (deps)'}
"#;
    fs::write(&file, yaml).unwrap();
    let before = snapshot(&github);
    let report = project.report(&github, 0);
    let row = &report["dependabot:prefixes"];
    assert_eq!(row.status, "ok");
    let warnings = row.warnings.as_ref().unwrap();
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("updates[0].commit-message.prefix-development"));
    assert!(warnings[0].contains(":arrow_up:"));
    let plain = project.cli(&github, &["doctor"]);
    assert!(
        String::from_utf8(plain.stdout)
            .unwrap()
            .contains("Warning:")
    );
    assert_eq!(snapshot(&github), before);

    let invalid = yaml
        .replace("🧩⬆ — (deps)", "🧩🔒 — (deps)")
        .replace("🧑‍💻⬆ — (deps)", "🎉 — (deps)");
    fs::write(&file, invalid).unwrap();
    let report = project.report(&github, 1);
    let row = &report["dependabot:prefixes"];
    assert_eq!(row.status, "mismatch");
    assert!(row.found.contains("updates[0].commit-message.prefix:"));
    assert!(
        row.found
            .contains("updates[1].commit-message.prefix: the stored shortcode")
    );
    assert!(
        row.warnings
            .as_ref()
            .unwrap()
            .iter()
            .any(|w| w.contains(":lock:"))
    );
}

#[test]
fn dependabot_include_uses_the_project_scope_and_ticket_rules() {
    let project = Project::new();
    project.init(false);
    let github = project.root.join(".github");
    fs::create_dir_all(&github).unwrap();
    let file = github.join("dependabot.yml");
    let settings = project.root.join("gitmoji.toml");
    let rules = format!(
        "[vocabulary]\nversion = '{}'\n[ticket]\npatterns = ['TASK-[0-9]+']\n[scope]\nallowed = ['deps', 'deps-dev']\n",
        env!("CARGO_PKG_VERSION")
    );
    fs::write(&settings, &rules).unwrap();
    let yaml = r#"version: 2
updates:
  - package-ecosystem: npm
    directory: /
    schedule: { interval: weekly }
    commit-message: {prefix: '🧩⬆ TASK-42 ', include: scope}
"#;
    fs::write(&file, yaml).unwrap();
    let report = project.report(&github, 0);
    assert_eq!(report["dependabot:prefixes"].status, "ok");
    assert!(report["dependabot:prefixes"].warnings.is_none());
    fs::write(&settings, rules.replace(", 'deps-dev'", "")).unwrap();
    let report = project.report(&github, 1);
    assert!(report["dependabot:prefixes"].found.contains("deps-dev"));
    fs::write(
        &file,
        yaml.replace("ecosystem: npm", "ecosystem: github-actions"),
    )
    .unwrap();
    assert_eq!(
        project.report(&github, 0)["dependabot:prefixes"].status,
        "ok"
    );
    fs::write(&file, yaml.replace("TASK-42 ", "#42 ")).unwrap();
    let report = project.report(&github, 1);
    assert!(report["dependabot:prefixes"].found.contains("ticket"));
    for prefix in ["— (deps)", "— (deps):"] {
        fs::write(&file, yaml.replace("TASK-42 ", prefix)).unwrap();
        let report = project.report(&github, 1);
        assert!(
            report["dependabot:prefixes"]
                .found
                .contains("already supplies a scope")
        );
    }
}

#[test]
fn dependabot_reports_malformed_configuration_without_echoing_unrelated_fields() {
    let project = Project::new();
    project.init(false);
    let github = project.root.join(".github");
    fs::create_dir_all(&github).unwrap();
    let file = github.join("dependabot.yml");
    for (yaml, diagnostic) in [
        ("updates: [invalid-example-value", "line"),
        (
            "updates: [{commit-message: {prefix-development: '🧩⬆ — (deps)'}}]",
            ".prefix is required",
        ),
        (
            "updates: [{commit-message: {prefix: false}}]",
            ".prefix must be a string",
        ),
        (
            "updates: [{commit-message: {prefix: '🧩⬆ — (deps)', include: 42}}]",
            ".include must be scope",
        ),
        (
            "updates: [{commit-message: {prefix: '🔖 release'}}]",
            "authored change",
        ),
    ] {
        fs::write(&file, yaml).unwrap();
        let report = project.report(&github, 1);
        let row = &report["dependabot:prefixes"];
        assert_eq!(row.status, "mismatch", "{yaml}");
        assert!(row.found.contains(diagnostic), "{row:?}");
        assert!(!row.found.contains("invalid-example-value"));
    }
}

#[test]
fn dependabot_checks_group_prefixes_and_their_actual_update_entries() {
    let project = Project::new();
    project.init(false);
    let github = project.root.join(".github");
    fs::create_dir_all(&github).unwrap();
    let file = github.join("dependabot.yml");
    let yaml = r#"version: 2
multi-ecosystem-groups:
  sdk:
    schedule: { interval: weekly }
    commit-message: {prefix: "🧩⬆ — (deps)"}
updates:
  - package-ecosystem: npm
    directory: /
    patterns: ['*']
    multi-ecosystem-group: sdk
  - package-ecosystem: pip
    directory: /
    patterns: ['*']
    multi-ecosystem-group: sdk
"#;
    fs::write(&file, yaml).unwrap();
    assert_eq!(
        project.report(&github, 0)["dependabot:prefixes"].status,
        "ok"
    );
    fs::write(&file, yaml.replace("🧩⬆ — (deps)", "unknown")).unwrap();
    let report = project.report(&github, 1);
    assert!(
        report["dependabot:prefixes"]
            .found
            .contains("multi-ecosystem-groups[\"sdk\"].commit-message.prefix")
    );
    fs::write(
        &file,
        format!("{yaml}    commit-message: {{prefix: '🧩⬆ — (deps)'}}\n"),
    )
    .unwrap();
    let report = project.report(&github, 1);
    assert!(
        report["dependabot:prefixes"]
            .found
            .contains("updates[1].commit-message belongs in multi-ecosystem-groups")
    );
}
