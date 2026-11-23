//! Assemble the repository homepage from its vocabulary, checked examples, CLI and install guide.

mod examples;
mod features;
mod reference;
mod story;

use std::env;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use gitmoji_triangle::vocabulary::{Slot, Vocabulary};

const INSTALL: &str = include_str!("../../../INSTALL.md");

fn combinations(vocab: &Vocabulary) -> Result<String> {
    let rule = |title: &str| {
        vocab
            .rules
            .iter()
            .find(|rule| rule.title == title)
            .map(|rule| rule.text.as_str())
            .with_context(|| format!("missing combination rule: {title}"))
    };
    let modifiers: Vec<_> = vocab
        .groups_in(Slot::Modifier)
        .flat_map(|group| &group.entries)
        .map(|entry| format!("{} {}", entry.emoji, entry.name.to_lowercase()))
        .collect();
    let markers: Vec<_> = vocab
        .groups_in(Slot::Marker)
        .flat_map(|group| &group.entries)
        .map(|entry| format!("{} {}", entry.emoji, entry.name.to_lowercase()))
        .collect();
    Ok(format!(
        "## How to combine the emojis\n\n\
         `{subject}`\n\n\
         - **Area and second emoji.** {pair} Projects can require the area with `lone_second = false` under `[rules]`.\n\
         - **Behaviour or intent.** {second}\n\
         - **Modifier.** Append at most one ({modifiers}) and give the change a ticket.\n\
         - **Standalone marker.** {markers} each have their own subject with no ticket or scope.\n\n\
         The ticket ties the commits together. The scope names the part of the project that owns the decision.\n",
        subject = vocab.format.subject,
        pair = rule("Pair first")?,
        second = rule("Behaviour or intent")?,
        modifiers = modifiers.join(", "),
        markers = markers.join(" and "),
    ))
}

fn render() -> Result<String> {
    let vocab = Vocabulary::builtin()?;
    let mut page = String::from(
        "<!-- Generated from Gitmoji's vocabulary, CLI, examples and INSTALL.md. See docs/development.md. -->\n\n\
         # Gitmoji Triangle\n\n\
         Gitmoji helps people and coding agents record one decision per commit. When agents land changes faster than people can read every diff, the emoji pair shows the area and intent of each change. The issue number keeps related decisions together across pull requests.\n\n",
    );
    for section in [
        story::render(&vocab)?,
        combinations(&vocab)?,
        examples::render(&vocab)?,
    ] {
        page.push_str(section.trim_end());
        page.push_str("\n\n");
    }
    page.push_str("## Vocabulary by lane\n\n");
    page.push_str(&reference::render(&vocab));
    page.push('\n');
    // The standalone installation guide lives next to README.md, so relative links work in both.
    for line in INSTALL.lines() {
        if line.starts_with("# ") || line.starts_with("## ") {
            page.push('#');
        }
        page.push_str(line);
        page.push('\n');
    }
    page.push('\n');
    page.push_str(features::render()?.trim_end());
    page.push_str("\n\n## Pull request titles\n\nGitmoji updates the emoji prefix as commits arrive. The words remain editable. A landed merge adds the PR number at the end. [PR and merge setup](skills/gitmoji/references/pull-requests.md) · [Local branch merges](skills/gitmoji/references/pull-requests.md#local-branch-merges)\n\n");
    page.push_str("## Guides\n\n[Authoring commits](skills/gitmoji/SKILL.md) · [History upgrades](skills/gitmoji/references/upgrade-history.md) · [CLI](package/README.md) · [Agent skill](skills/README.md) · [Contributing](AGENTS.md) · [Development](docs/development.md) · [Website](website/README.md) · [CI and releases](.github/ci-and-releases.md)\n\n[Changelog](CHANGELOG.md) · [MIT license](LICENSE)\n");
    Ok(page)
}

fn main() -> Result<()> {
    let args: Vec<_> = env::args().skip(1).collect();
    ensure!(args.len() == 1, "use --write, --check or --stdout");
    let readme = Path::new(env!("CARGO_MANIFEST_DIR")).join("../README.md");
    let output = render()?;
    match args[0].as_str() {
        "--write" => {
            if fs::read(&readme).ok().as_deref() != Some(output.as_bytes()) {
                fs::write(&readme, output).context("write README.md")?;
            }
        }
        "--check" => ensure!(
            fs::read(&readme).context("read README.md")? == output.as_bytes(),
            "README.md is stale. Run cargo run --locked --manifest-path package/Cargo.toml --example readme -- --write"
        ),
        "--stdout" => print!("{output}"),
        _ => bail!("use --write, --check or --stdout"),
    }
    Ok(())
}
