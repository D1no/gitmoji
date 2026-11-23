//! Build the opening example from the authored decisions on one issue.

use std::fmt::Write;

use anyhow::{Result, ensure};
use gitmoji_triangle::{caption, check, config::Config, vocabulary::Vocabulary};

pub(super) fn render(vocab: &Vocabulary) -> Result<String> {
    let issue = "#412";
    let rules = [
        format!("🧩👔 {issue} (appeals): allow filing within 30 days of refusal"),
        format!("🧩🛡️ {issue} (appeals): require a refusal reference"),
    ];
    let portal = [
        format!("🎨✨ {issue} (portal): open an appeal from the refusal notice"),
        format!("🎨💬 {issue} (portal): explain which documents to attach"),
        format!("👌 <`🎨🐛`> {issue} (portal): keep the form open on the final day"),
    ];

    let mut markdown = format!(
        "## One issue, several decisions\n\n\
         **Traditional commit**\n\n\
         `feat(appeals): let applicants appeal a refused permit ({issue})`\n\n\
         **With Gitmoji**\n\n"
    );
    let config = Config::standard()?.for_a_subject_alone();
    for (number, title, subjects) in [
        (417, "Set the filing rules for appeals", rules.as_slice()),
        (
            423,
            "Let applicants appeal from the portal",
            portal.as_slice(),
        ),
    ] {
        for subject in subjects {
            let report = check::check_message(vocab, &config, subject, None);
            ensure!(
                report.passed(),
                "invalid README example {subject:?}: {}",
                report.errors.join(", ")
            );
        }
        let caption =
            caption::summarize(number, title, subjects, vocab).map_err(anyhow::Error::msg)?;
        ensure!(
            caption.recognized == subjects.len(),
            "the README example PR {number} must count every authored decision"
        );
        writeln!(markdown, "- {}", caption.title)?;
        for subject in subjects {
            writeln!(markdown, "  - {subject}")?;
        }
    }

    markdown.push_str(
        "\nBefore writing the first PR, an agent can ask “How long can someone appeal?”\n\
         and “What must they supply?” Months later, a human scanning the same issue\n\
         can jump to 👔 for the deadline, 🛡️ for the guard, or 👌 for the fix prompted by review.\n",
    );
    Ok(markdown)
}
