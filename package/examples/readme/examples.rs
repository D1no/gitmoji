//! Render the special forms with the same subject checker and PR caption used by the CLI.

use anyhow::{Context, Result, ensure};
use gitmoji_triangle::{caption, check, config::Config, vocabulary::Vocabulary};

pub(super) fn render(vocab: &Vocabulary) -> Result<String> {
    let review = &vocab
        .format
        .review_response
        .as_ref()
        .context("the README needs a review-response example")?
        .example;
    let job = &vocab.format.job_example;
    let provisional = "🎨🎭🚧 #425 (portal): show sample appeal decisions for the demo";
    let finished = "🎨🏁 #425 (portal): replace sample decisions with the live appeal record";
    let upgrade = format!(
        "🔺 gitmoji `{}`: check commits with the same vocabulary in CI and locally",
        vocab.version
    );

    let settings = Config::standard()?.for_a_subject_alone();
    for subject in [review.as_str(), job, provisional, finished, &upgrade] {
        let report = check::check_message(vocab, &settings, subject, None);
        ensure!(
            report.passed(),
            "invalid README example {subject:?}: {}",
            report.errors.join(", ")
        );
    }

    let preview = caption::summarize(
        426,
        "Preview appeal decisions",
        &[provisional.to_owned()],
        vocab,
    )
    .map_err(anyhow::Error::msg)?;
    ensure!(
        preview.recognized == 1 && preview.modifiers.iter().any(|symbol| symbol == "🚧"),
        "the sample appeal commit must produce a provisional PR"
    );

    Ok(format!(
        "## Special commit and pull request forms\n\n\
         **In response to review.** After 👌, the enclosed emojis identify the decision prompted by feedback. For written feedback, \
         `Review` links to the reviewer’s request and an optional `Addresses` list names related comments.\n\n\
         - {review}\n\n\
         **Provisional work.** A 🚧 commit makes the open PR title provisional. The same issue tracks the demo through its later replacement.\n\n\
         - {title}\n  - {provisional}\n\n\
         A later 🏁 commit records the outcome on the same ticket.\n\n\
         - {finished}\n\n\
         **Vocabulary upgrade.** When a repository changes its Gitmoji pin, record the adopted version in a standalone commit.\n\n\
         - {upgrade}\n\n\
         **Automated job.** A job without a ticket puts an em dash in the ticket position.\n\n\
         - {job}\n",
        title = preview.title
    ))
}
