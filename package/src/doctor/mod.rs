//! Read-only diagnostics for a repository's installed Gitmoji components.

mod files;
mod hooks;

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use anyhow::Result;
use gitmoji_triangle::config::locate;
use gitmoji_triangle::vocabulary::Vocabulary;
use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum Status {
    Ok,
    Missing,
    Mismatch,
    Stale,
    NotEnrolled,
}

impl Status {
    fn healthy(self) -> bool {
        matches!(self, Self::Ok | Self::NotEnrolled)
    }
}

impl fmt::Display for Status {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status = match self {
            Self::Ok => "ok",
            Self::Missing => "missing",
            Self::Mismatch => "mismatch",
            Self::Stale => "stale",
            Self::NotEnrolled => "not-enrolled",
        };
        formatter.write_str(status)
    }
}

#[derive(Debug, Serialize)]
struct Row {
    component: String,
    status: Status,
    found: String,
    expected: String,
    remedy: String,
}

impl Row {
    fn new(
        component: impl Into<String>,
        status: Status,
        found: impl Into<String>,
        expected: impl Into<String>,
        remedy: impl Into<String>,
    ) -> Self {
        Self {
            component: component.into(),
            status,
            found: found.into(),
            expected: expected.into(),
            remedy: remedy.into(),
        }
    }
}

fn pin_row(file: Option<&Path>, binary: &str) -> (Row, Option<String>) {
    let Some(file) = file else {
        return (
            Row::new(
                "pin",
                Status::Missing,
                "no settings file",
                binary,
                "gitmoji init",
            ),
            None,
        );
    };
    let (status, found) = match super::read_release_pin(file) {
        Ok(Some(pin)) => {
            return (
                Row::new("pin", Status::Ok, &pin, "a numbered release", ""),
                Some(pin),
            );
        }
        Ok(None) => (Status::Missing, "no [vocabulary].version setting".into()),
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
        {
            (Status::Missing, error.to_string())
        }
        Err(error) => (Status::Mismatch, error.to_string()),
    };
    let display = file.to_string_lossy();
    let remedy = shlex::try_quote(&display)
        .map(|path| format!("vi {path}"))
        .unwrap_or_else(|_| "edit the Gitmoji settings file".into());
    (
        Row::new(
            "pin",
            status,
            found,
            format!("[vocabulary].version = \"{binary}\""),
            remedy,
        ),
        None,
    )
}

fn binary_row(binary: &str, pin: Option<&str>) -> Row {
    match pin {
        Some(pin) if pin != binary => {
            // The parsed pin contains only release-number digits and dots.
            let remedy = format!(
                "tool=$(mktemp -d) && git clone --quiet --depth 1 --branch v{pin} https://github.com/D1no/gitmoji.git \"$tool/source\" && \"$tool/source/install.sh\""
            );
            Row::new("binary", Status::Mismatch, binary, pin, remedy)
        }
        _ => Row::new(
            "binary",
            Status::Ok,
            binary,
            pin.unwrap_or("running executable"),
            "",
        ),
    }
}

fn root() -> Option<PathBuf> {
    let out = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let mut bytes = out.stdout;
    if bytes.last() == Some(&b'\n') {
        bytes.pop();
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        Some(std::ffi::OsString::from_vec(bytes).into())
    }
    #[cfg(not(unix))]
    {
        Some(String::from_utf8(bytes).ok()?.trim_end_matches('\r').into())
    }
}

pub(super) fn run(config: Option<&Path>, json: bool) -> Result<ExitCode> {
    let rows = if let Some(top) = root() {
        let file = config.map(ToOwned::to_owned).or_else(|| locate(&top));
        let version = env!("CARGO_PKG_VERSION");
        let (pin_row, pin) = pin_row(file.as_deref(), version);
        let mut rows = vec![binary_row(version, pin.as_deref()), pin_row];
        rows.extend(hooks::inspect(&top));
        rows.extend(files::inspect(
            &top,
            pin.as_deref(),
            &Vocabulary::builtin()?,
        ));
        rows
    } else {
        vec![Row::new(
            "repository",
            Status::Missing,
            "no accessible Git working tree",
            "a Git checkout",
            "run gitmoji doctor inside the project checkout",
        )]
    };
    if json {
        println!("{}", serde_json::to_string(&rows)?);
    } else {
        for row in &rows {
            println!("{}  {}  {}", row.component, row.status, row.found);
            if !row.status.healthy() {
                println!("  Expected {}. Remedy: {}", row.expected, row.remedy);
            }
        }
    }
    Ok(if rows.iter().all(|row| row.status.healthy()) {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}
