//! Compare the installed skill and enrolled workflows with the release's own templates.

use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;

use anyhow::Result;
use gitmoji_triangle::{guide, vocabulary::Vocabulary};

use super::{Row, Status};
use crate::init::{self, SkillPaths, Workflow, WorkflowFile};

pub(super) fn inspect(top: &Path, pin: Option<&str>, vocab: &Vocabulary) -> Vec<Row> {
    let same_release = pin == Some(vocab.version.as_str());
    let mut rows = vec![skill(top, vocab, same_release)];
    rows.extend(
        init::WORKFLOWS
            .iter()
            .map(|workflow| workflow_row(top, workflow, pin, same_release)),
    );
    let provisional = workflow_row(top, &init::PROVISIONAL_WORKFLOW, pin, same_release);
    if provisional.status != Status::NotEnrolled {
        rows.push(provisional);
    }
    rows
}

fn skill(top: &Path, vocab: &Vocabulary, same_release: bool) -> Row {
    let expected = if same_release {
        format!("skill and references from {}", vocab.version)
    } else {
        format!("{}/SKILL.md", init::SKILL_DIR)
    };
    let row = |status, found: String| {
        Row::new(
            "skill",
            status,
            found,
            &expected,
            if status == Status::Ok {
                ""
            } else {
                "gitmoji init"
            },
        )
    };
    let paths = match SkillPaths::read_only(top) {
        Ok(paths) => paths,
        Err(e) => return row(Status::Stale, e.to_string()),
    };
    let reference = if same_release {
        guide::all(vocab)
    } else {
        String::new()
    };
    for (name, content) in init::skill_files(&reference) {
        if !same_release && name != "SKILL.md" {
            continue;
        }
        let name = format!("{}/{name}", init::SKILL_DIR);
        let path = match paths.resolve(&name) {
            Ok(path) => top.join(path),
            Err(e) => return row(Status::Stale, e.to_string()),
        };
        match fs::metadata(&path) {
            Ok(meta) if meta.is_file() => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => return row(Status::Missing, name),
            _ => {
                return row(
                    Status::Stale,
                    format!("{name} is not a readable regular file"),
                );
            }
        }
        if same_release {
            match fs::read_to_string(&path) {
                Ok(installed) if installed.replace("\r\n", "\n") == content => {}
                _ => {
                    return row(
                        Status::Stale,
                        format!("{name} differs from the release or is unreadable"),
                    );
                }
            }
        }
    }
    row(
        Status::Ok,
        if same_release {
            expected.clone()
        } else {
            "skill present; contents require the pinned binary".into()
        },
    )
}

// Git can still establish enrollment after a worktree deletion or a staged removal.
// Only local objects are consulted: this must never fetch from a partial clone.
fn previously_managed(top: &Path, name: &str) -> bool {
    ["", "HEAD"].iter().any(|revision| {
        Command::new("git")
            .current_dir(top)
            .env("GIT_NO_LAZY_FETCH", "1")
            .env("GIT_ALLOW_PROTOCOL", "")
            .args(["cat-file", "blob", &format!("{revision}:{name}")])
            .output()
            .is_ok_and(|output| {
                output.status.success()
                    && output
                        .stdout
                        .split(|c| *c == b'\n')
                        .next()
                        .is_some_and(init::managed_workflow_header)
            })
    })
}

fn installed_workflows(top: &Path, wf: &Workflow) -> Result<(WorkflowFile, WorkflowFile)> {
    // Init treats linked workflow directories as project-owned, including links into
    // the worktree. Do not open files through a linked ancestor.
    for dir in [".github", ".github/workflows"] {
        match top.join(dir).symlink_metadata() {
            Ok(meta) if !meta.is_dir() => {
                return Ok((WorkflowFile::ProjectOwned, WorkflowFile::ProjectOwned));
            }
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Ok((WorkflowFile::Missing, WorkflowFile::Missing));
            }
            Err(e) => return Err(e.into()),
        }
    }
    Ok((
        init::workflow_file(&top.join(wf.path))?,
        if wf.legacy_path.is_empty() {
            WorkflowFile::Missing
        } else {
            init::workflow_file(&top.join(wf.legacy_path))?
        },
    ))
}

fn workflow_row(top: &Path, wf: &Workflow, pin: Option<&str>, same_release: bool) -> Row {
    let component = format!("workflow:{}", wf.job);
    let expected = pin.map_or_else(
        || wf.path.to_owned(),
        |v| format!("{} at release {v}", wf.path),
    );
    let row = |status, found: String| {
        Row::new(
            &component,
            status,
            found,
            &expected,
            match status {
                Status::Ok | Status::NotEnrolled => "",
                _ if wf.job == init::PROVISIONAL_WORKFLOW.job => {
                    "gitmoji init --provisional-workflow"
                }
                _ => "gitmoji init --github-workflows",
            },
        )
    };
    let (current, legacy) = match installed_workflows(top, wf) {
        Ok(pair) => pair,
        Err(e) => return row(Status::Stale, format!("cannot read the workflow: {e}")),
    };
    // A mismatched binary may legitimately have a different filename or template.
    let (text, path) = match (current, legacy) {
        (_, WorkflowFile::Managed(_)) if same_release => {
            return row(
                Status::Stale,
                format!("earlier workflow at {}", wf.legacy_path),
            );
        }
        (WorkflowFile::Managed(text), _) => (text, wf.path),
        (_, WorkflowFile::Managed(text)) => (text, wf.legacy_path),
        (current, legacy) => {
            for (file, name) in [(current, wf.path), (legacy, wf.legacy_path)] {
                if !name.is_empty() && previously_managed(top, name) {
                    return match file {
                        WorkflowFile::Missing => row(
                            Status::Missing,
                            format!("enrolled workflow {name} is missing"),
                        ),
                        _ => row(
                            Status::Stale,
                            format!("{name} is no longer a managed regular file"),
                        ),
                    };
                }
            }
            return row(Status::NotEnrolled, "no managed workflow".into());
        }
    };
    let Some(pin) = pin else {
        return row(
            Status::Ok,
            format!("{path} present; no valid pin for comparison"),
        );
    };
    let workflow_pin = text
        .lines()
        .find_map(|l| l.strip_prefix(init::WORKFLOW_PIN))
        .map(|v| v.trim().trim_matches(['\'', '"']));
    if workflow_pin != Some(pin) {
        return row(
            Status::Mismatch,
            format!("{path} has release {}", workflow_pin.unwrap_or("(missing)")),
        );
    }
    if same_release && text.replace("\r\n", "\n") != init::pin_workflow(wf.template, pin) {
        return row(Status::Stale, format!("{path} differs from release {pin}"));
    }
    row(
        Status::Ok,
        if same_release {
            format!("{path} matches release {pin}")
        } else {
            format!("{path} pins {pin}; contents require the pinned binary")
        },
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use tempfile::tempdir;

    fn write(top: &Path, name: &str, data: &str) {
        let path = top.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, data).unwrap();
    }

    fn git(top: &Path, args: &[&str]) {
        let output = Command::new("git")
            .current_dir(top)
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "-c",
                "commit.gpgsign=false",
            ])
            .arg("-c")
            .arg(format!(
                "core.hooksPath={}",
                top.join("empty-hooks").display()
            ))
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn workflow_checks_distinguish_enrollment_pins_drift_and_tracked_deletions() {
        let repo = tempdir().unwrap();
        let top = repo.path();
        let vocab = Vocabulary::builtin().unwrap();
        let version = vocab.version.as_str();
        let wf = &init::WORKFLOWS[0];
        git(top, &["init", "-q"]);
        let inspect = || workflow_row(top, wf, Some(version), true).status;
        assert_eq!(inspect(), Status::NotEnrolled);
        write(top, wf.path, "name: project-owned\n");
        git(top, &["add", wf.path]);
        assert_eq!(inspect(), Status::NotEnrolled);

        let template = init::pin_workflow(wf.template, version);
        write(top, wf.path, &template.replace('\n', "\r\n"));
        assert_eq!(inspect(), Status::Ok);
        write(top, wf.path, &init::pin_workflow(wf.template, "0.0.1"));
        assert_eq!(inspect(), Status::Mismatch);
        let changed = format!("{template}\n# local edit");
        write(top, wf.path, &changed);
        assert_eq!(inspect(), Status::Stale);
        let old_release = init::pin_workflow(&changed, "0.0.1");
        write(top, wf.path, &old_release);
        assert_eq!(
            workflow_row(top, wf, Some("0.0.1"), false).status,
            Status::Ok
        );
        git(top, &["add", wf.path]);
        git(top, &["commit", "-qm", "enroll"]);
        fs::remove_file(top.join(wf.path)).unwrap();
        assert_eq!(inspect(), Status::Missing);
        git(top, &["add", "-u"]);
        assert_eq!(inspect(), Status::Missing);
    }

    #[test]
    fn skill_checks_every_managed_reference_and_respects_the_pin() {
        let repo = tempdir().unwrap();
        let top = repo.path();
        let vocab = Vocabulary::builtin().unwrap();
        assert_eq!(skill(top, &vocab, true).status, Status::Missing);
        for (name, content) in init::skill_files(&guide::all(&vocab)) {
            write(
                top,
                &format!("{}/{name}", init::SKILL_DIR),
                &content.replace('\n', "\r\n"),
            );
        }
        assert_eq!(skill(top, &vocab, true).status, Status::Ok);
        let reference = format!("{}/references/vocabulary.txt", init::SKILL_DIR);
        write(top, &reference, "different");
        assert_eq!(skill(top, &vocab, true).status, Status::Stale);
        assert_eq!(skill(top, &vocab, false).status, Status::Ok);
        fs::remove_file(top.join(reference)).unwrap();
        assert_eq!(skill(top, &vocab, true).status, Status::Missing);
        assert_eq!(skill(top, &vocab, false).status, Status::Ok);
    }

    #[cfg(unix)]
    #[test]
    fn shared_skill_links_stay_inside_the_worktree() {
        use std::os::unix::fs::symlink;

        let repo = tempdir().unwrap();
        let top = repo.path();
        let vocab = Vocabulary::builtin().unwrap();
        for (name, content) in init::skill_files(&guide::all(&vocab)) {
            write(top, &format!("skills/gitmoji/{name}"), content);
        }
        fs::create_dir_all(top.join(".claude/skills")).unwrap();
        let link = top.join(init::SKILL_DIR);
        symlink("../../skills/gitmoji", &link).unwrap();
        assert_eq!(skill(top, &vocab, true).status, Status::Ok);
        fs::remove_file(&link).unwrap();
        let external = tempdir().unwrap();
        write(external.path(), "SKILL.md", "private fixture data");
        symlink(external.path(), &link).unwrap();
        let outside = skill(top, &vocab, false);
        assert_eq!(outside.status, Status::Stale);
        assert!(!outside.found.contains("private fixture data"));
    }
}
