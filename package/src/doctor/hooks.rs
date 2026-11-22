//! Inspect Git's selected hooks and the commands registered with Lefthook.
//! The check reads local files without running hook or configuration code.
use std::cell::OnceCell;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;

use super::{Row, Status};

const HOOKS: [&str; 3] = ["commit-msg", "prepare-commit-msg", "pre-commit"];

fn hooks_directory(top: &Path) -> Option<PathBuf> {
    // Git applies core.hooksPath and the linked worktree's shared directory here.
    let out = Command::new("git")
        .current_dir(top)
        .args(["rev-parse", "--path-format=absolute", "--git-path", "hooks"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let path = out.stdout.strip_suffix(b"\n").unwrap_or(&out.stdout);
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        Some(PathBuf::from(OsStr::from_bytes(path)))
    }
    #[cfg(not(unix))]
    {
        std::str::from_utf8(path)
            .ok()
            .map(|p| PathBuf::from(p.trim_end_matches('\r')))
    }
}

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

fn always_enabled(node: &Value) -> bool {
    node.get("skip").is_none_or(|v| v == false) && node.get("only").is_none_or(|v| v == true)
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
    let dir = hooks_directory(top);
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
                return row(
                    Status::Mismatch,
                    "hook is not an executable file",
                    "gitmoji init",
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
            if std::env::var_os("LEFTHOOK").is_some_and(|v| v == "0") {
                return row(
                    Status::Mismatch,
                    "Lefthook bypassed by LEFTHOOK=0",
                    "unset LEFTHOOK",
                );
            }
            match config() {
                Ok(Some(cfg)) if configured(cfg, hook) => {
                    row(Status::Ok, "Lefthook wrapper and configured command", "")
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
