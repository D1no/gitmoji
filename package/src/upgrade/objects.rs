//! Lossless Git object access. Commit messages and headers may contain non-UTF-8 bytes.

use std::collections::HashMap;
use std::io::Write;
use std::process::{Command, Output, Stdio};

use anyhow::{Context, Result, bail, ensure};

pub(super) fn command() -> Command {
    let mut git = Command::new("git");
    // All writes are to the local object database and one new candidate ref.
    // A promisor remote must never be contacted to complete an offline preview.
    git.arg("--no-replace-objects")
        .env_remove("GIT_GRAFT_FILE")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_ALLOW_PROTOCOL", "");
    git
}

pub(super) fn checked(result: Output) -> Result<Vec<u8>> {
    if !result.status.success() {
        let diagnostics = String::from_utf8_lossy(&result.stderr);
        bail!("Git exited with {}: {}", result.status, diagnostics.trim());
    }
    Ok(result.stdout)
}

pub(super) fn git(args: &[&str]) -> Result<Vec<u8>> {
    checked(command().args(args).output().context("cannot run Git")?)
}

pub(super) fn line(args: &[&str]) -> Result<String> {
    Ok(String::from_utf8(git(args)?)?
        .trim_end_matches(['\n', '\r'])
        .to_owned())
}

pub(super) fn resolve(rev: &str) -> Result<String> {
    line(&[
        "rev-parse",
        "--verify",
        "--end-of-options",
        &format!("{rev}^{{commit}}"),
    ])
}

pub(super) fn partition<'a>(input: &'a [u8], sep: &[u8]) -> (&'a [u8], &'a [u8]) {
    input
        .windows(sep.len())
        .position(|s| s == sep)
        .map_or((input, &[]), |pos| {
            (&input[..pos], &input[pos + sep.len()..])
        })
}

pub(super) struct Commit {
    pub headers: Vec<Vec<u8>>,
    pub parents: Vec<String>,
    pub message: Vec<u8>,
}

impl Commit {
    pub fn read(oid: &str) -> Result<Self> {
        let raw = git(&["cat-file", "commit", oid])?;
        let (head, msg) = partition(&raw, b"\n\n");
        ensure!(head.len() < raw.len(), "invalid commit object {oid}");
        let mut headers: Vec<Vec<u8>> = Vec::new();
        let mut parents = Vec::new();
        for field in head.split(|&c| c == b'\n') {
            if field.starts_with(b" ") {
                let previous = headers
                    .last_mut()
                    .context("invalid continuation in commit header")?;
                previous.push(b'\n');
                previous.extend_from_slice(field);
            } else {
                if let Some(parent) = field.strip_prefix(b"parent ") {
                    parents.push(std::str::from_utf8(parent)?.to_owned());
                }
                headers.push(field.to_vec());
            }
        }
        Ok(Self {
            headers,
            parents,
            message: msg.to_vec(),
        })
    }

    pub fn subject(&self) -> &[u8] {
        partition(&self.message, b"\n").0
    }

    pub fn header(&self, key: &[u8]) -> Option<&[u8]> {
        self.headers.iter().find_map(|h| h.strip_prefix(key))
    }
}

pub(super) fn tree_changes(args: &[&str]) -> Result<Vec<u8>> {
    let mut cmd = command();
    cmd.args([
        "diff-tree",
        "--no-commit-id",
        "--no-abbrev",
        "--raw",
        "-r",
        "-z",
        "--no-renames",
        "--no-relative",
        "--no-ext-diff",
        "--no-textconv",
        "--ignore-submodules=none",
    ]);
    checked(cmd.args(args).arg("--").output()?)
}

pub(super) fn branch_available(reference: &str) -> Result<()> {
    let occupied = format!("branch {reference}");
    ensure!(
        !git(&["worktree", "list", "--porcelain", "-z"])?
            .split(|&c| c == 0)
            .any(|s| s == occupied.as_bytes()),
        "candidate branch is checked out in a worktree; choose a fresh name"
    );
    for (args, error) in [
        (
            vec!["symbolic-ref", "--quiet", "--no-recurse", reference],
            "candidate is an existing symbolic ref; choose a fresh name",
        ),
        (
            vec!["show-ref", "--verify", "--quiet", reference],
            "candidate branch already exists; choose a fresh name",
        ),
    ] {
        let out = command().args(args).output()?;
        match out.status.code() {
            Some(0) => bail!("{error}"),
            Some(1) => (),
            _ => bail!(
                "could not verify the candidate ref: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ),
        }
    }
    Ok(())
}

pub(super) fn hash_commit(raw: &[u8], write: bool) -> Result<String> {
    let mut cmd = command();
    cmd.args(["hash-object", "-t", "commit", "--stdin"]);
    if write {
        cmd.arg("-w");
    }
    let mut proc = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stream = proc.stdin.take().context("missing git stdin")?;
    stream.write_all(raw)?;
    drop(stream);
    Ok(String::from_utf8(checked(proc.wait_with_output()?)?)?
        .trim_end()
        .into())
}

pub(super) fn candidate_inventory(
    inventory: &[u8],
    parents: &[String],
    mapped: &HashMap<String, String>,
    subjects: &HashMap<String, String>,
) -> Result<Vec<u8>> {
    let mut rows: Vec<Vec<u8>> = inventory
        .split(|&c| c == b'\n')
        .map(<[u8]>::to_vec)
        .collect();
    let count = format!("--max-count={}", rows.len() - 1);
    let base = format!("^{}", parents[0]);
    let mut args = vec![
        "log",
        "--first-parent",
        "--topo-order",
        "--no-patch",
        "--no-color",
        "--no-notes",
        "--no-show-signature",
        "--encoding=UTF-8",
        "-z",
        "--format=%H%x00%s",
        &count,
        &base,
    ];
    args.extend(parents[1..].iter().map(String::as_str));
    args.push("--");
    let log = git(&args)?;
    let mut fields = log.split(|&c| c == 0);
    for row in rows.iter_mut().skip(1) {
        let (Some(oid), Some(subject)) = (fields.next(), fields.next()) else {
            break;
        };
        let oid = std::str::from_utf8(oid)?;
        let Some(new) = mapped.get(oid) else { continue };
        let Some(listed) = row.strip_prefix(b"- ") else {
            continue;
        };
        let (short, listed_subject) = partition(listed, b" ");
        if short.len() >= 4 && oid.as_bytes().starts_with(short) && listed_subject == subject {
            *row = [
                b"- ".as_slice(),
                new.as_bytes(),
                b" ",
                subjects.get(oid).map_or(subject, |s| s.as_bytes()),
            ]
            .concat();
        }
    }
    Ok(rows.join(&b'\n'))
}
