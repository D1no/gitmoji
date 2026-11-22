//! Synchronize titles in a trusted pull_request_target workflow.
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail, ensure};
use base64::Engine;
use clap::Args;
use gitmoji_triangle::{caption, vocabulary::Vocabulary};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};

use super::{Client, valid_pr};
use crate::history::incoming_commits_with;

const TITLE_HELP: &str = "pr-title updates the title on GitHub. The managed Actions workflow supplies \
    GITHUB_SERVER_URL, GITHUB_API_URL, GITHUB_REPOSITORY and GITMOJI_TITLE_TOKEN.\n\n\
    For a read-only preview from a full local clone, run:\n  \
    gitmoji github-merge --repo OWNER/REPO --pr NUMBER [--hostname HOST]";

#[derive(Args)]
#[command(after_long_help = TITLE_HELP)]
pub(crate) struct TitleArgs {
    /// Number of the open PR in GITHUB_REPOSITORY.
    #[arg(long)]
    pr: u64,
}

#[derive(Debug, PartialEq, Eq)]
struct Pull {
    title: String,
    branch: String,
    base: String,
    head: String,
}

fn oid(id: &str) -> bool {
    matches!(id.len(), 40 | 64)
        && id
            .bytes()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

impl Client {
    fn open_pull(&self, number: u64) -> Result<Option<Pull>> {
        let pr = self.get_pull(number)?;
        ensure!(
            pr["number"].as_u64() == Some(number),
            "GitHub returned a different PR"
        );
        match pr["state"].as_str() {
            Some("closed") => return Ok(None),
            Some("open") => (),
            _ => bail!("unknown PR state"),
        }
        let branch = pr["base"]["ref"]
            .as_str()
            .context("missing destination branch")?;
        ensure!(
            branch
                .split('/')
                .all(|p| !p.is_empty() && p != "." && p != ".."),
            "invalid destination branch"
        );
        let path = branch
            .split('/')
            .map(|part| utf8_percent_encode(part, NON_ALPHANUMERIC).to_string())
            .collect::<Vec<_>>()
            .join("/");
        let reference =
            self.request("GET", &format!("{}/git/ref/heads/{path}", self.rest), None)?;
        ensure!(
            reference["ref"] == format!("refs/heads/{branch}")
                && reference["object"]["type"] == "commit",
            "GitHub returned a different destination ref"
        );
        let base = reference["object"]["sha"]
            .as_str()
            .filter(|id| oid(id))
            .context("missing base commit ID")?;
        let head = pr["head"]["sha"]
            .as_str()
            .filter(|id| oid(id))
            .context("missing head commit ID")?;
        Ok(Some(Pull {
            title: pr["title"].as_str().context("missing PR title")?.to_owned(),
            branch: branch.into(),
            base: base.into(),
            head: head.into(),
        }))
    }
}

// Use only network/TLS and platform settings from the runner. Git never sees
// the Actions token except as a header on the base repository's fetch.
fn git(directory: &Path, args: &[&str], credential: Option<(&str, &str)>) -> Result<String> {
    let mut cmd = Command::new("git");
    cmd.current_dir(directory).env_clear();
    for key in [
        "PATH",
        "SYSTEMROOT",
        "WINDIR",
        "TMP",
        "TEMP",
        "TMPDIR",
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "NO_PROXY",
        "ALL_PROXY",
        "http_proxy",
        "https_proxy",
        "no_proxy",
        "all_proxy",
        "SSL_CERT_FILE",
        "SSL_CERT_DIR",
    ] {
        if let Some(value) = std::env::var_os(key) {
            cmd.env(key, value);
        }
    }
    if let Some(path) = std::env::var_os("SSL_CERT_FILE") {
        cmd.env("GIT_SSL_CAINFO", path);
    }
    cmd.env("HOME", directory)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_TERMINAL_PROMPT", "0")
        .arg("--no-replace-objects");
    if let Some((remote, token)) = credential {
        let basic =
            base64::engine::general_purpose::STANDARD.encode(format!("x-access-token:{token}"));
        cmd.env("GIT_CONFIG_COUNT", "1")
            .env("GIT_CONFIG_KEY_0", format!("http.{remote}.extraHeader"))
            .env(
                "GIT_CONFIG_VALUE_0",
                format!("Authorization: Basic {basic}"),
            );
    }
    let output = cmd.args(args).output().context("cannot execute git")?;
    // Git diagnostics can quote contributor-controlled strings and credentials.
    ensure!(
        output.status.success(),
        "isolated Git command failed ({})",
        output.status
    );
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn proposal(remote: &str, token: &str, number: u64, pr: &Pull) -> Result<String> {
    let checkout = tempfile::Builder::new()
        .prefix("gitmoji-pr-title-")
        .tempdir()?;
    let repo = checkout.path();
    let local = |args: &[&str]| git(repo, args, None);
    local(&["init", "--bare", "--quiet", "--template="])?;
    let branch = format!("refs/heads/{}", pr.branch);
    local(&["check-ref-format", &branch])?;
    git(
        repo,
        &[
            "-c",
            "http.followRedirects=false",
            "fetch",
            "--no-tags",
            "--no-recurse-submodules",
            "--filter=tree:0",
            "--",
            remote,
            &format!("+{branch}:refs/title/base"),
            &format!("+refs/pull/{number}/head:refs/title/head"),
        ],
        Some((remote, token)),
    )
    .context(
        "could not fetch the destination branch and PR head; verify contents: read and the PR refs",
    )?;
    let fetched = local(&["rev-parse", "refs/title/base", "refs/title/head"])?;
    ensure!(
        fetched.lines().eq([pr.base.as_str(), pr.head.as_str()]),
        "fetched base or PR head differs from the GitHub API"
    );
    let incoming = incoming_commits_with(&pr.base, &[&pr.head], local)?;
    let caption = caption::summarize_pr(
        number,
        &pr.title,
        &incoming.subjects,
        incoming.contains_merge,
        &Vocabulary::builtin()?,
    )
    .map_err(anyhow::Error::msg)?;
    Ok(caption.title)
}

fn synchronize(
    mut read: impl FnMut() -> Result<Option<Pull>>,
    mut preview: impl FnMut(&Pull) -> Result<String>,
    mut update: impl FnMut(&str) -> Result<()>,
) -> Result<&'static str> {
    for _ in 0..3 {
        let Some(pr) = read()? else {
            return Ok("PR is closed.");
        };
        let proposed = preview(&pr);
        // Whether the preview succeeded or failed, recheck both refs and the
        // human wording before acting on it. GitHub has no conditional title PATCH.
        if read()?.as_ref() != Some(&pr) {
            continue;
        }
        let title = proposed?;
        if title == pr.title {
            return Ok("PR title already matches.");
        }
        update(&title)?;
        return Ok("Updated PR title.");
    }
    bail!("PR changed repeatedly during the preview; rerun against its current state")
}

/// Update one open PR's title, using only trusted Actions context.
pub(crate) fn sync_title(args: TitleArgs) -> Result<()> {
    valid_pr(args.pr)?;
    let setting = |key| {
        std::env::var(key)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .with_context(|| format!("{key} is missing or empty. {TITLE_HELP}"))
    };
    let client = Client::with_token(
        &setting("GITHUB_SERVER_URL")?,
        &setting("GITHUB_API_URL")?,
        &setting("GITHUB_REPOSITORY")?,
        setting("GITMOJI_TITLE_TOKEN")?,
    )?;
    let remote = format!("{}/{}.git", client.server, client.repo);
    let status = synchronize(
        || client.open_pull(args.pr),
        |pr| proposal(&remote, &client.token, args.pr, pr),
        |title| {
            let returned = client.update_title(args.pr, title)?;
            ensure!(
                returned["title"] == title,
                "GitHub returned a different title; inspect the current PR and rerun"
            );
            Ok(())
        },
    )?;
    println!("{status}");
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use std::collections::VecDeque;

    fn pull(title: &str) -> Pull {
        Pull {
            title: title.to_owned(),
            branch: "trunk".into(),
            base: "a".repeat(40),
            head: "b".repeat(40),
        }
    }

    #[test]
    fn title_sync_rechecks_on_a_preview_race_and_uses_latest_human_wording() {
        let mut states = VecDeque::from([
            Some(pull("first")),
            Some(pull("second")),
            Some(pull("second")),
            Some(pull("second")),
        ]);
        let mut updated = String::new();
        let result = synchronize(
            || Ok(states.pop_front().unwrap()),
            |pr| {
                if pr.title == "first" {
                    bail!("old refs disappeared");
                }
                Ok(format!("new {}", pr.title))
            },
            |v| {
                v.clone_into(&mut updated);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(result, "Updated PR title.");
        assert_eq!(updated, "new second");
    }

    #[test]
    fn title_sync_does_not_write_unchanged_or_unstable_snapshots() {
        assert_eq!(
            synchronize(|| Ok(None), |_| panic!(), |_| panic!()).unwrap(),
            "PR is closed."
        );
        assert_eq!(
            synchronize(
                || Ok(Some(pull("ready"))),
                |p| Ok(p.title.clone()),
                |_| panic!()
            )
            .unwrap(),
            "PR title already matches."
        );
        let mut i = 0;
        assert!(
            synchronize(
                || {
                    i += 1;
                    Ok(Some(pull(&i.to_string())))
                },
                |_| Ok("proposed".into()),
                |_| panic!()
            )
            .is_err()
        );
        assert_eq!(i, 6);
    }

    #[test]
    fn isolated_bare_fetch_uses_only_matching_base_and_pull_refs() {
        let origin = tempfile::tempdir().unwrap();
        let dir = origin.path();
        let run = |args: &[&str]| git(dir, args, None).unwrap().trim().to_owned();
        run(&["init", "-q", "-b", "trunk"]);
        run(&[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "--allow-empty",
            "-qm",
            "base",
        ]);
        let base = run(&["rev-parse", "HEAD"]);
        run(&["checkout", "-qb", "incoming"]);
        std::fs::write(dir.join("gitmoji.toml"), "unusable project config\n").unwrap();
        run(&["add", "gitmoji.toml"]);
        run(&[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "🎨✨ #4 (ui): show a preview",
        ]);
        let head = run(&["rev-parse", "HEAD"]);
        run(&["update-ref", "refs/pull/12/head", &head]);
        let pr = Pull {
            title: "show the preview".into(),
            branch: "trunk".into(),
            base,
            head,
        };
        let remote = dir.to_str().unwrap();
        assert_eq!(
            proposal(remote, "fixture", 12, &pr).unwrap(),
            "[`🎨✨`] show the preview"
        );
        let changed = Pull {
            base: "0".repeat(40),
            ..pr
        };
        assert!(proposal(remote, "fixture", 12, &changed).is_err());
        run(&["update-ref", "-d", "refs/pull/12/head"]);
        assert!(proposal(remote, "fixture", 12, &changed).is_err());
    }
}
