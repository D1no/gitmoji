//! Inspect Git's selected hooks and the commands registered with Lefthook.
//! The check reads local files without running hook or configuration code.
use std::cell::OnceCell;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;

use super::{Row, Status};

const HOOKS: [&str; 3] = ["commit-msg", "prepare-commit-msg", "pre-commit"];

fn find_config(top: &Path, stem: &str) -> Option<PathBuf> {
    // Lefthook searches extensions first, then root, dotfile, .config.
    for ext in ["yml", "yaml", "json", "jsonc", "toml"] {
        for prefix in ["", ".", ".config/"] {
            let candidate = top.join(format!("{prefix}{stem}.{ext}"));
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    None
}

fn lefthook_config(top: &Path) -> Result<Option<Value>> {
    let main = std::env::var_os("LEFTHOOK_CONFIG")
        .filter(|v| !v.is_empty())
        .map(|p| top.join(p))
        .or_else(|| find_config(top, "lefthook"));
    let local = find_config(top, "lefthook-local");
    ensure!(
        main.is_none() || local.is_none(),
        "layered Lefthook configuration requires inspection"
    );
    let Some(path) = main.or(local) else {
        return Ok(None);
    };
    ensure!(
        path.is_file(),
        "Lefthook configuration must be a regular file"
    );
    let text = std::fs::read_to_string(&path).context("cannot read Lefthook configuration")?;
    let cfg: Value = match path.extension().and_then(OsStr::to_str) {
        Some("yaml" | "yml") => {
            let mut yaml: serde_yaml_ng::Value =
                serde_yaml_ng::from_str(&text).context("invalid Lefthook YAML")?;
            yaml.apply_merge().context("invalid Lefthook YAML merge")?;
            serde_json::to_value(yaml).context("invalid Lefthook configuration")?
        }
        Some("toml") => {
            let parsed: toml::Table = toml::from_str(&text).context("invalid Lefthook TOML")?;
            serde_json::to_value(parsed)?
        }
        Some("json") => serde_json::from_str(&text).context("invalid Lefthook JSON")?,
        _ => bail!("unsupported Lefthook configuration format"),
    };
    ensure!(cfg.is_object(), "invalid Lefthook configuration");
    ensure!(
        ["extends", "remotes"].iter().all(|key| {
            cfg.get(key)
                .is_none_or(|v| v.is_null() || v.as_array().is_some_and(Vec::is_empty))
        }),
        "layered Lefthook configuration requires inspection"
    );
    Ok(Some(cfg))
}

// Only accept direct commands; comments, echo statements and other hooks'
// commands must not certify a hook. shlex accepts both quote styles for {1}.
fn calls_gitmoji(line: &str, hook: &str, managed: bool) -> bool {
    if !managed && (line.contains('\'') || line.contains(r"\$")) {
        // shlex strips quotes; a single-quoted or escaped $1 is a literal filename.
        return false;
    }
    let Some(words) = shlex::split(line) else {
        return false;
    };
    let words: Vec<&str> = words.iter().map(String::as_str).collect();
    let words = words.strip_prefix(&["exec"]).unwrap_or(&words);
    let Some(args) = words.strip_prefix(&["gitmoji"]) else {
        return false;
    };
    let file = if managed { "{1}" } else { "$1" };
    match hook {
        "commit-msg" => args == ["check", "--fix", file] || args == ["check", file, "--fix"],
        "pre-commit" => args == ["staged"],
        "prepare-commit-msg" => {
            let source = if managed { "{2}" } else { "$2" };
            args == ["merge-message", file, source]
                || (!managed && args == ["merge-message", file, "${2:-}"])
        }
        _ => false,
    }
}

fn direct_hook(script: &str, hook: &str) -> bool {
    let mut lines = script.lines().map(str::trim).filter(|line| {
        !line.is_empty() && !line.starts_with('#') && !matches!(*line, "set -e" | "set -eu")
    });
    lines
        .next()
        .is_some_and(|line| calls_gitmoji(line, hook, false))
        && lines.next().is_none()
}

fn delegates_to_lefthook(script: &str, hook: &str) -> bool {
    let Some(line) = script
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
    else {
        return false;
    };
    let Some(words) = shlex::split(line) else {
        return false;
    };
    let words: Vec<&str> = words.iter().map(String::as_str).collect();
    let words = words.strip_prefix(&["exec"]).unwrap_or(&words);
    matches!(words, ["call_lefthook" | "lefthook" | "lefthook.exe", "run", name, "$@"] if *name == hook)
}

// Generated wrappers contain a conditional exit and a shell function. A
// top-level exit inserted before their final dispatch makes that dispatch dead.
fn exits_before_dispatch(script: &str) -> bool {
    let mut in_dispatcher = false;
    let mut conditions = 0_u32;
    for line in script.lines().map(str::trim) {
        if line.starts_with("call_lefthook()") {
            in_dispatcher = true;
        } else if in_dispatcher {
            if line == "}" {
                in_dispatcher = false;
            }
        } else if line.starts_with("if ") && !line.ends_with("fi") {
            conditions += 1;
        } else if line == "fi" {
            conditions = conditions.saturating_sub(1);
        } else if conditions == 0 && line.split([' ', '\t', ';']).next() == Some("exit") {
            return true;
        }
    }
    false
}

fn on_path(top: &Path, candidate: &Path, path: &OsStr) -> bool {
    let is_executable = |p: &Path| p.metadata().is_ok_and(|m| executable(&m));
    if candidate.is_absolute() || candidate.components().count() > 1 {
        return is_executable(&top.join(candidate));
    }
    std::env::split_paths(path).any(|dir| {
        let file = top.join(dir).join(candidate);
        is_executable(&file)
            || (cfg!(windows)
                && file.extension().is_none()
                && ["exe", "cmd", "bat"]
                    .iter()
                    .any(|ext| is_executable(&file.with_extension(ext))))
    })
}

enum Manager {
    Available,
    Missing,
    Dynamic,
}

// Inspect the actual dispatch alternatives in this wrapper. In particular,
// old Homebrew paths and missing npm packages do not count as installations.
fn manager(top: &Path, script: &str, path: &OsStr, override_bin: Option<&OsStr>) -> Manager {
    let mut dynamic_fallback = false;
    for line in script.lines().map(str::trim) {
        let Some(words) = shlex::split(line) else {
            continue;
        };
        let words: Vec<_> = words.iter().map(String::as_str).collect();
        let words = words.strip_prefix(&["exec"]).unwrap_or(&words);
        match words {
            ["$LEFTHOOK_BIN", "$@"] => {
                if let Some(bin) = override_bin.filter(|s| !s.is_empty()) {
                    return if on_path(top, Path::new(bin), path) {
                        Manager::Available
                    } else {
                        Manager::Missing
                    };
                }
            }
            // An explicit lefthook path embedded by the installer has priority
            // over all later alternatives, even when it is no longer executable.
            ["elif", "test", "-n", selected] if !selected.is_empty() => {
                return if selected.contains('$')
                    || selected.split_whitespace().count() > 1 && !Path::new(selected).is_absolute()
                {
                    Manager::Dynamic
                } else if on_path(top, Path::new(selected), path) {
                    Manager::Available
                } else {
                    Manager::Missing
                };
            }
            [cmd, "$@"] if !cmd.starts_with('$') || cmd.starts_with("$dir/") => {
                let os = match std::env::consts::OS {
                    "macos" => "darwin",
                    other => other,
                };
                let arch = match std::env::consts::ARCH {
                    "aarch64" => "arm64",
                    "x86_64" => "x64",
                    other => other,
                };
                let cmd = cmd
                    .strip_prefix("$dir/")
                    .unwrap_or(cmd)
                    .replace("${osArch}", os)
                    .replace("${cpuArch}", arch);
                if on_path(top, Path::new(&cmd), path) {
                    return Manager::Available;
                }
            }
            [cmd @ ("lefthook" | "lefthook.exe"), "run", _, "$@"] => {
                if on_path(top, Path::new(cmd), path) {
                    return Manager::Available;
                }
            }
            [cmd, .., "$@"]
                if *cmd != "call_lefthook" && *cmd != "lefthook" && *cmd != "lefthook.exe" =>
            {
                dynamic_fallback |= on_path(top, Path::new(cmd), path);
            }
            _ => {}
        }
    }
    if dynamic_fallback {
        Manager::Dynamic
    } else {
        Manager::Missing
    }
}

fn always_enabled(node: &Value) -> bool {
    node.get("skip").is_none_or(|v| v == false)
        && node.get("only").is_none_or(|v| v == true)
        && ["glob", "files", "exclude", "file_types", "root"]
            .iter()
            .all(|key| {
                node.get(*key).is_none_or(|v| {
                    v.is_null() || v.as_str() == Some("") || v.as_array().is_some_and(Vec::is_empty)
                })
            })
}

fn job_calls(job: &Value, name: &str, hook: &str, excluded: &[&str]) -> bool {
    if !always_enabled(job)
        || excluded.contains(&name)
        || job["tags"].as_array().is_some_and(|tags| {
            tags.iter()
                .filter_map(Value::as_str)
                .any(|tag| excluded.contains(&tag))
        })
    {
        return false;
    }
    job["run"]
        .as_str()
        .is_some_and(|cmd| calls_gitmoji(cmd.trim(), hook, true))
        || job["group"]["jobs"].as_array().is_some_and(|jobs| {
            jobs.iter()
                .any(|child| job_calls(child, child["name"].as_str().unwrap_or(""), hook, excluded))
        })
}

fn configured(cfg: &Value, hook: &str) -> bool {
    let section = &cfg[hook];
    if !always_enabled(section) {
        return false;
    }
    let excluded_env = std::env::var("LEFTHOOK_EXCLUDE").unwrap_or_default();
    let excluded: Vec<&str> = section["exclude_tags"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .chain(excluded_env.split(',').filter(|s| !s.is_empty()))
        .collect();
    section["commands"].as_object().is_some_and(|commands| {
        commands
            .iter()
            .any(|(name, job)| job_calls(job, name, hook, &excluded))
    }) || section["jobs"].as_array().is_some_and(|jobs| {
        jobs.iter()
            .any(|job| job_calls(job, job["name"].as_str().unwrap_or(""), hook, &excluded))
    })
}

fn expected(hook: &str) -> &'static str {
    match hook {
        "commit-msg" => "gitmoji check --fix with the commit message file",
        "prepare-commit-msg" => "gitmoji merge-message with the message file and source",
        _ => "gitmoji staged",
    }
}

fn executable(meta: &std::fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.is_file() && meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        meta.is_file()
    }
}

pub(super) fn inspect(top: &Path) -> Vec<Row> {
    let dir = super::git_path(
        top,
        &["rev-parse", "--path-format=absolute", "--git-path", "hooks"],
    );
    let config = OnceCell::new();
    let config = || config.get_or_init(|| lefthook_config(top));
    HOOKS
        .iter()
        .map(|&hook| {
            let row = |status, found: &str, remedy: &str| {
                Row::new(
                    format!("hook:{hook}"),
                    status,
                    found,
                    expected(hook),
                    remedy,
                )
            };
            let Some(dir) = &dir else {
                return row(
                    Status::Missing,
                    "Git could not resolve its hooks directory",
                    "git config --show-origin --get core.hooksPath",
                );
            };
            let file = dir.join(hook);
            let meta = match std::fs::metadata(&file) {
                Ok(meta) => meta,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    let cfg = config();
                    if hook == "pre-commit"
                        && cfg
                            .as_ref()
                            .is_ok_and(|cfg| cfg.as_ref().is_none_or(|v| !configured(v, hook)))
                    {
                        return row(Status::NotEnrolled, "optional hook absent", "");
                    }
                    let remedy = if matches!(cfg, Ok(Some(c)) if configured(c,hook)) {
                        "lefthook install"
                    } else {
                        "gitmoji init"
                    };
                    return row(Status::Missing, "hook absent", remedy);
                }
                Err(_) => return row(Status::Mismatch, "hook metadata unreadable", "gitmoji init"),
            };
            if !executable(&meta) {
                let remedy = if cfg!(unix) && meta.is_file() {
                    format!(r#"chmod +x "$(git rev-parse --git-path hooks/{hook})""#)
                } else { "gitmoji init".to_string() };
                return row(
                    Status::Mismatch,
                    "hook is not an executable file",
                    &remedy,
                );
            }
            let Ok(script) = std::fs::read_to_string(&file) else {
                return row(
                    Status::Mismatch,
                    "unable to inspect hook as a script",
                    "gitmoji init",
                );
            };
            if direct_hook(&script, hook) {
                return row(Status::Ok, "direct Git hook", "");
            }
            if !delegates_to_lefthook(&script, hook) {
                return if hook == "pre-commit" {
                    row(Status::NotEnrolled, "project pre-commit hook", "")
                } else {
                    row(Status::Mismatch, "unrecognized hook wiring", "gitmoji init")
                };
            }
            if exits_before_dispatch(&script) {
                return row(Status::Mismatch, "hook exits before dispatching Lefthook", "lefthook install");
            }
            if let Ok(value) = std::env::var("LEFTHOOK")
                && matches!(value.as_str(), "0" | "false")
            {
                return row(Status::Bypassed, &format!("Lefthook bypassed by LEFTHOOK={value}"), "unset LEFTHOOK");
            }
            match config() {
                Ok(Some(cfg)) if configured(cfg, hook) => {
                    match manager(top, &script, &std::env::var_os("PATH").unwrap_or_default(), std::env::var_os("LEFTHOOK_BIN").as_deref()) {
                        Manager::Available => row(Status::Ok, "Lefthook wrapper, executable and configured command", ""),
                        Manager::Missing => row(Status::Missing, "Lefthook executable unavailable to this wrapper", "install Lefthook or set LEFTHOOK_BIN to its executable"),
                        Manager::Dynamic => row(Status::Mismatch, "only a package-manager fallback is available; its Lefthook executable is unverified", "set LEFTHOOK_BIN to the installed Lefthook executable"),
                    }
                }
                Ok(_) if hook == "pre-commit" => {
                    row(Status::NotEnrolled, "optional command not configured", "")
                }
                Ok(_) => row(
                    Status::Missing,
                    "Gitmoji command missing from Lefthook configuration",
                    "gitmoji init",
                ),
                Err(err) => row(
                    Status::Mismatch,
                    &err.to_string(),
                    "lefthook dump --format json",
                ),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use serde_json::json;

    #[test]
    fn recognizes_shell_expansion_and_lefthook_substitutions() {
        assert!(direct_hook(
            "#!/bin/sh\nexec gitmoji check --fix \"$1\"",
            "commit-msg"
        ));
        assert!(!direct_hook("gitmoji check --fix '$1'", "commit-msg"));
        assert!(!direct_hook(r#"gitmoji check --fix "\$1""#, "commit-msg"));
        for run in ["gitmoji check --fix {1}", r#"gitmoji check --fix "{1}""#] {
            let cfg = json!({"commit-msg":{"commands":{"checker":{"run":run},"another":{"glob":"*.md","run":"lint"}}}});
            assert!(configured(&cfg, "commit-msg"));
        }
    }

    #[test]
    fn ignores_unrelated_or_disabled_commands() {
        let cfg = json!({"commit-msg":{"jobs":[
            {"run":"echo gitmoji check --fix {1}"},
            {"run":"gitmoji check --fix {1}", "skip":true},
            {"group":{"jobs":[{"run":"gitmoji check --fix {1}", "glob":"*.md"}]}},
        ]},"pre-push":{"jobs":[{"run":"gitmoji check --fix {1}"}]}});
        assert!(!configured(&cfg, "commit-msg"));
        let good =
            json!({"commit-msg":{"jobs":[{"group":{"jobs":[{"run":"gitmoji check --fix {1}"}]}}]}});
        assert!(configured(&good, "commit-msg"));
    }

    #[test]
    fn resolves_generated_wrapper_targets_without_executing_them() {
        let dir = tempfile::tempdir().unwrap();
        let top = dir.path();
        let name = if cfg!(windows) {
            "lefthook.exe"
        } else {
            "lefthook"
        };
        let script = format!(
            r#"if [ "$LEFTHOOK" = "0" ]; then
  exit 0
fi
call_lefthook()
{{
  "$LEFTHOOK_BIN" "$@"
  {name} "$@"
  "$dir/node_modules/local/lefthook" "$@"
  pnpm lefthook "$@"
}}
call_lefthook run "commit-msg" "$@"
"#
        );
        assert!(delegates_to_lefthook(&script, "commit-msg"));
        assert!(!exits_before_dispatch(&script));
        assert!(exits_before_dispatch(&format!("exit 0\n{script}")));
        assert!(matches!(
            manager(top, &script, top.as_os_str(), None),
            Manager::Missing
        ));
        // Stand-in executable only supplies a file and its mode; it is never invoked.
        std::fs::write(top.join(name), "#!/bin/sh\nexit 97\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(top.join(name), std::fs::Permissions::from_mode(0o755))
                .unwrap();
        }
        assert!(matches!(
            manager(top, &script, top.as_os_str(), None),
            Manager::Available
        ));
        assert!(matches!(
            manager(top, &script, top.as_os_str(), Some(OsStr::new("absent"))),
            Manager::Missing
        ));
        std::fs::rename(top.join(name), top.join("pnpm")).unwrap();
        assert!(matches!(
            manager(top, &script, top.as_os_str(), None),
            Manager::Dynamic
        ));
        std::fs::create_dir_all(top.join("node_modules/local")).unwrap();
        std::fs::rename(top.join("pnpm"), top.join("node_modules/local/lefthook")).unwrap();
        assert!(matches!(
            manager(top, &script, top.as_os_str(), None),
            Manager::Available
        ));
    }

    #[cfg(unix)]
    #[test]
    fn missing_execute_bit_reports_a_working_repair() {
        use std::os::unix::fs::PermissionsExt;
        use std::process::Command;
        let temp = tempfile::tempdir().unwrap();
        let top = temp.path();
        assert!(
            Command::new("git")
                .args(["init", "-q"])
                .current_dir(top)
                .status()
                .unwrap()
                .success()
        );
        let dir = top.join("hooks with spaces");
        std::fs::create_dir(&dir).unwrap();
        assert!(
            Command::new("git")
                .args(["config", "core.hooksPath"])
                .arg(&dir)
                .current_dir(top)
                .status()
                .unwrap()
                .success()
        );
        let file = dir.join("commit-msg");
        std::fs::write(&file, "#!/bin/sh\nexec gitmoji check --fix \"$1\"\n").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        let failed = inspect(top)
            .into_iter()
            .find(|r| r.component == "hook:commit-msg")
            .unwrap();
        assert_eq!(failed.status, Status::Mismatch);
        assert!(
            Command::new("sh")
                .args(["-c", &failed.remedy])
                .current_dir(top)
                .status()
                .unwrap()
                .success()
        );
        assert_eq!(inspect(top)[0].status, Status::Ok);
    }
}
