//! Mark agent-written GitHub posts and compare their raw Markdown with the recorded fingerprint.


use std::io::{self, Read as _};

use anyhow::{Context, Result, ensure};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD_NO_PAD;
use clap::{Args, Subcommand, ValueEnum};
use gitmoji_triangle::attribution::{Fence, has_harness_footer};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::signature::{self, Harness};

const CHIP: &str = "`🤖`";
const RESERVED: &str = "<!-- gitmoji-post:";
const HEADER: &str = "<!-- gitmoji-post:v1 ";
const FOOTER_WARNING: &str =
    "A final attribution line names a harness. Remove it if the harness inserted it.";

/// Work with raw GitHub post bodies (including issue, PR and review text).
#[derive(Args)]
pub(crate) struct CommentArgs {
    #[command(subcommand)]
    action: Action,
}

#[derive(Subcommand)]
enum Action {
    /// Read Markdown from stdin and write a marked body with a hidden v1 fingerprint.
    Sign {
        /// Use this harness (Claude Code, Codex and Antigravity support automatic discovery).
        #[arg(long)]
        harness: Option<String>,
        /// Explicitly keep only harness, model and effort in the block (the default).
        #[arg(long, conflicts_with = "full_lineage")]
        public: bool,
        /// Include recorded session, role, helpers, team and research. Raw readers can see them.
        #[arg(long)]
        full_lineage: bool,
    },
    /// Check raw Markdown on stdin. An agent post needs both markers and a matching fingerprint.
    Check {
        /// Also accept unmarked text when checking a mixture of human and agent posts.
        #[arg(long)]
        allow_human: bool,
        /// Emit a structured report with status, recorded lineage and attribution warnings.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Lineage {
    pub(crate) harness: String,
    pub(crate) model: String,
    pub(crate) effort: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) session: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) role: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) sub_agents: Vec<SubAgent>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) team: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) research: Option<Research>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SubAgent {
    pub(crate) model: String,
    pub(crate) effort: String,
    pub(crate) count: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Research {
    pub(crate) searches: usize,
    pub(crate) pages: usize,
    pub(crate) docs: usize,
}

impl Lineage {
    fn public(&mut self) {
        self.session = None;
        self.role = None;
        self.sub_agents.clear();
        self.team.clear();
        self.research = None;
    }

    fn valid(&self) -> bool {
        let simple = |s: &str| {
            !s.trim().is_empty() && !s.chars().any(|c| {
            c.is_control() || matches!(c, '\u{2028}' | '\u{2029}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
        })
        };
        [&self.harness, &self.model, &self.effort]
            .into_iter()
            .all(|s| simple(s))
            && self
                .session
                .iter()
                .chain(self.role.iter())
                .chain(self.team.iter())
                .all(|s| simple(s))
            && self
                .sub_agents
                .iter()
                .all(|agent| agent.count > 0 && simple(&agent.model) && simple(&agent.effort))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Metadata {
    sha256: String,
    lineage: Lineage,
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Status {
    Unsigned,
    Verified,
    Edited,
    Invalid,
}

#[derive(Debug, Serialize)]
pub(super) struct Inspection {
    pub(super) status: Status,
    pub(super) lineage: Option<Lineage>,
    pub(super) detail: Option<String>,
    pub(super) warnings: Vec<String>,
}

fn fingerprint(body: &str) -> String {
    format!("{:x}", Sha256::digest(body.as_bytes()))
}

fn marked_prose(visible: &str) -> Option<&str> {
    let after_chip = visible.strip_prefix(CHIP)?;
    after_chip
        .strip_prefix("\n\n")
        .or_else(|| after_chip.strip_prefix(' '))
}

// Keep block Markdown at the beginning of its own paragraph. A terminal newline
// from a body file is part of the fingerprint, but does not make prose multiline.
fn chip_separator(body: &str) -> &'static str {
    let line = body.trim_end_matches('\n');
    let digits = |c: char| c.is_ascii_digit();
    let plain_number =
        line.starts_with(digits) && !line.trim_start_matches(digits).starts_with(['.', ')']);
    if (line.starts_with(char::is_alphabetic) || plain_number) && !line.contains(['|', '\n', '\r'])
    {
        " "
    } else {
        "\n\n"
    }
}

fn split_record(body: &str) -> Result<Option<(&str, Metadata)>> {
    let mut fence = Fence::default();
    let mut markers = body
        .lines()
        .filter(|line| !fence.code_line(line) && line.starts_with(RESERVED));
    let Some(_) = markers.next() else {
        ensure!(
            !body.starts_with(CHIP),
            "agent prefix is present without a trailing lineage block"
        );
        return Ok(None);
    };
    ensure!(markers.next().is_none(), "more than one lineage block");
    let (visible, tail) = body
        .trim_end_matches('\n')
        .rsplit_once("\n\n")
        .context("expected a separate final lineage block")?;
    let prose = marked_prose(visible)
        .context("lineage block is present without the opening agent prefix")?;
    ensure!(!prose.trim().is_empty(), "empty marked body");
    let encoded = tail
        .strip_prefix(HEADER)
        .and_then(|s| s.strip_suffix(" -->"))
        .context("expected the supported v1 lineage block at the end of the body")?;
    // Standard base64 has no hyphen, so names, roles and model strings cannot close the HTML comment.
    let bytes = STANDARD_NO_PAD
        .decode(encoded)
        .context("invalid encoded lineage block")?;
    let metadata: Metadata = serde_json::from_slice(&bytes).context("invalid v1 lineage fields")?;
    ensure!(
        metadata.sha256.len() == 64
            && metadata
                .sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "invalid SHA-256 fingerprint"
    );
    ensure!(metadata.lineage.valid(), "invalid lineage text");
    Ok(Some((visible, metadata)))
}

/// Return the original visible Markdown and untouched separator/footer for a managed edit.
/// A valid envelope can still carry evidence that the visible content was edited.
pub(crate) fn marked_parts(body: &str) -> Option<(&str, &str)> {
    split_record(&body.replace("\r\n", "\n")).ok()??;
    let (before_marker, _) = body.rsplit_once(HEADER)?;
    for separator in ["\r\n\r\n", "\r\n\n", "\n\r\n", "\n\n"] {
        if let Some(visible) = before_marker.strip_suffix(separator) {
            return Some(body.split_at(visible.len()));
        }
    }
    None
}

pub(super) fn inspect(body: &str) -> Inspection {
    let normalized = body.replace("\r\n", "\n");
    let (status, lineage, detail, prose) = match split_record(&normalized) {
        Ok(Some((visible, metadata))) => {
            let (status, detail) = if metadata.sha256 == fingerprint(visible) {
                (Status::Verified, None)
            } else {
                (
                    Status::Edited,
                    Some("visible text differs from its recorded SHA-256".into()),
                )
            };
            (
                status,
                Some(metadata.lineage),
                detail,
                marked_prose(visible).unwrap_or(visible),
            )
        }
        Ok(None) => (Status::Unsigned, None, None, normalized.as_str()),
        Err(error) => (
            Status::Invalid,
            None,
            Some(error.to_string()),
            normalized.as_str(),
        ),
    };
    let warnings = if has_harness_footer(prose) {
        vec![FOOTER_WARNING.to_owned()]
    } else {
        Vec::new()
    };
    Inspection {
        status,
        lineage,
        detail,
        warnings,
    }
}

fn discover(name: Option<&str>, public: bool) -> Result<Lineage> {
    let known = match name {
        Some(name) => match <Harness as ValueEnum>::from_str(name, true) {
            Ok(harness) => Some(harness),
            Err(_) => {
                ensure!(
                    !name.is_empty()
                        && name.len() <= 64
                        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'),
                    "use a harness name of up to 64 letters, numbers and hyphens"
                );
                return Ok(Lineage {
                    harness: name.to_owned(),
                    model: "unknown".into(),
                    effort: "unknown".into(),
                    session: None,
                    role: None,
                    sub_agents: Vec::new(),
                    team: Vec::new(),
                    research: None,
                });
            }
        },
        None => None,
    };
    signature::comment_lineage(known, public)
}

fn encode(visible: &str, lineage: Lineage) -> Result<String> {
    ensure!(lineage.valid(), "cannot encode invalid lineage fields");
    let metadata = Metadata {
        sha256: fingerprint(visible),
        lineage,
    };
    let encoded = STANDARD_NO_PAD.encode(serde_json::to_vec(&metadata)?);
    Ok(format!("{visible}\n\n{HEADER}{encoded} -->"))
}

fn sign(body: &str, harness: Option<&str>, public: bool) -> Result<String> {
    let body = body.replace("\r\n", "\n");
    ensure!(
        !body.trim().is_empty(),
        "supply a nonempty Markdown body on stdin"
    );
    if let Some((visible, mut existing)) = split_record(&body)? {
        ensure!(
            existing.sha256 == fingerprint(visible),
            "visible text was edited; review it and remove the old marks before signing a new body"
        );
        if public {
            existing.lineage.public();
        }
        return encode(visible, existing.lineage);
    }
    let mut fence = Fence::default();
    for line in body.lines() {
        fence.code_line(line);
    }
    ensure!(
        !fence.is_open(),
        "close the Markdown code fence before signing; it would expose the lineage block"
    );
    encode(
        &format!("{CHIP}{}{body}", chip_separator(&body)),
        discover(harness, public)?,
    )
}

fn stdin() -> Result<String> {
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .context("read Markdown from stdin")?;
    Ok(input)
}

pub(crate) fn run(args: CommentArgs) -> Result<u8> {
    match args.action {
        Action::Sign {
            harness,
            public: _,
            full_lineage,
        } => {
            let signed = sign(&stdin()?, harness.as_deref(), !full_lineage)?;
            for warning in inspect(&signed).warnings {
                eprintln!("gitmoji: note: {warning}");
            }
            print!("{signed}");
            Ok(0)
        }
        Action::Check { allow_human, json } => {
            let report = inspect(&stdin()?);
            let accepted = report.status == Status::Verified
                || (allow_human && report.status == Status::Unsigned);
            if json {
                println!("{}", serde_json::to_string(&report)?);
            } else {
                let reason = match report.status {
                    Status::Verified => "agent prefix and content fingerprint match",
                    Status::Unsigned if allow_human => "unmarked post",
                    Status::Unsigned => {
                        "agent posts require a marked body; use gitmoji comment sign"
                    }
                    Status::Invalid | Status::Edited => {
                        report.detail.as_deref().unwrap_or("invalid post")
                    }
                };
                println!("{reason}");
                for warning in report.warnings {
                    eprintln!("gitmoji: note: {warning}");
                }
            }
            Ok(if accepted { 0 } else { 1 })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Lineage {
        Lineage {
            harness: "Codex".into(),
            model: "m-123".into(),
            effort: "high".into(),
            role: Some("maintainer --> <test>".into()),
            session: Some("session-with--hyphens".into()),
            sub_agents: vec![SubAgent {
                model: "assistant".into(),
                effort: "medium".into(),
                count: 1,
            }],
            team: vec!["peer".into()],
            research: Some(Research {
                searches: 3,
                pages: 2,
                docs: 1,
            }),
        }
    }

    #[test]
    fn offline_signature_round_trip_preserves_markdown_and_line_endings() -> Result<()> {
        let input = "# Changes\r\n\r\nA deliberate line break.  \r\nAnother.\r\n\r\n";
        let signed = sign(input, Some("custom-agent"), true)?;
        let (visible, _) = split_record(&signed)?.context("marked record")?;
        assert_eq!(
            visible,
            format!("{CHIP}\n\n{}", input.replace("\r\n", "\n"))
        );
        assert_eq!(inspect(&signed).status, Status::Verified);
        assert_eq!(
            inspect(&signed.replace('\n', "\r\n")).status,
            Status::Verified
        );
        assert_eq!(
            inspect(&signed.replace("break.  ", "break. ")).status,
            Status::Edited
        );
        assert_eq!(sign(&signed, None, true)?, signed);
        assert!(sign(&signed.replace("Changes", "New"), None, true).is_err());
        Ok(())
    }

    #[test]
    fn simple_replies_use_an_inline_chip_and_blocks_keep_their_markdown_context() -> Result<()> {
        for body in [
            "Fixed in abc123.",
            "2 checks passed.\n",
            "Changes landed.\n\n",
        ] {
            let signed = sign(body, Some("example"), true)?;
            assert_eq!(
                marked_parts(&signed).context("inline parts")?.0,
                format!("{CHIP} {body}")
            );
            assert_eq!(inspect(&signed).status, Status::Verified);
        }
        for body in [
            "# Heading",
            "- first",
            "1. first",
            "3) first",
            "> quote",
            "    code",
            "A | B",
            "`literal`",
            "First.\nSecond.",
            "[link]: https://example.test",
        ] {
            let signed = sign(body, Some("example"), true)?;
            assert_eq!(
                marked_parts(&signed).context("paragraph parts")?.0,
                format!("{CHIP}\n\n{body}")
            );
            assert_eq!(inspect(&signed).status, Status::Verified);
        }
        let paragraph_v1 = encode(&format!("{CHIP}\n\nFixed in abc123."), sample())?;
        assert_eq!(inspect(&paragraph_v1).status, Status::Verified);
        assert_eq!(sign(&paragraph_v1, None, false)?, paragraph_v1);
        let changed_separator =
            paragraph_v1.replacen(&format!("{CHIP}\n\n"), &format!("{CHIP} "), 1);
        assert_eq!(inspect(&changed_separator).status, Status::Edited);
        Ok(())
    }

    #[test]
    fn managed_edits_keep_original_record_bytes_and_expose_edit_evidence() -> Result<()> {
        let signed =
            sign("Body with trailing space.  \n", Some("example"), true)?.replace('\n', "\r\n");
        let (visible, footer) = marked_parts(&signed).context("parts")?;
        assert_eq!(visible, format!("{CHIP} Body with trailing space.  \r\n"));
        assert!(footer.starts_with("\r\n\r\n<!-- gitmoji-post:"));
        let changed = format!("{visible}\r\nMachine-added text{footer}");
        assert_eq!(inspect(&changed).status, Status::Edited);
        assert_eq!(marked_parts(&changed).context("edited parts")?.1, footer);
        assert!(marked_parts("a human post").is_none());
        Ok(())
    }

    #[test]
    fn private_fields_stay_inside_one_block_and_public_resigning_removes_them() -> Result<()> {
        let signed = encode(&format!("{CHIP}\n\nA reviewed change."), sample())?;
        assert_eq!(signed.matches("-->").count(), 1);
        let full = inspect(&signed).lineage.context("full lineage")?;
        assert_eq!(full.session.as_deref(), Some("session-with--hyphens"));
        let safe = sign(&signed, None, true)?;
        let no_extras = serde_json::to_value(inspect(&safe).lineage)?;
        assert_eq!(
            no_extras,
            serde_json::json!({"harness":"Codex","model":"m-123","effort":"high"})
        );
        Ok(())
    }

    #[test]
    fn partial_duplicate_malformed_or_unknown_records_are_invalid() -> Result<()> {
        let signed = sign("result", Some("example"), true)?;
        let (_, footer) = signed.rsplit_once("\n\n").context("trailing block")?;
        for body in [
            format!("{CHIP} result"),
            format!("result\n\n{footer}"),
            format!("{signed}\n\n{footer}"),
            format!("{signed}\n\nmore prose"),
            signed.replace(HEADER, "<!-- gitmoji-post:v2 "),
            format!("{CHIP}\n\nresult\n\n{HEADER}oops ! -->"),
        ] {
            assert_eq!(inspect(&body).status, Status::Invalid, "{body}");
        }
        assert_eq!(inspect("an ordinary comment").status, Status::Unsigned);
        assert!(sign("  ", Some("example"), true).is_err());
        Ok(())
    }

    #[test]
    fn documentation_can_quote_the_protocol() -> Result<()> {
        let example =
            "```markdown\n<!-- gitmoji-post:v99 ... -->\n```\n\n> <!-- gitmoji-post:v1 … -->";
        let signed = sign(example, Some("example"), true)?;
        assert_eq!(inspect(&signed).status, Status::Verified);
        assert!(sign("```md\nliteral", Some("example"), true).is_err());
        Ok(())
    }

    #[test]
    fn injected_attribution_warns_without_rejecting_the_body() -> Result<()> {
        let signed = sign(
            "Work complete.\n\n🤖 Generated with [Claude Code](https://claude.com/claude-code)",
            Some("example"),
            true,
        )?;
        let inspection = inspect(&signed);
        assert_eq!(inspection.status, Status::Verified);
        assert_eq!(inspection.warnings.len(), 1);
        let inline = sign("Made with Cursor", Some("example"), true)?;
        assert_eq!(inspect(&inline).warnings.len(), 1);
        Ok(())
    }
}
