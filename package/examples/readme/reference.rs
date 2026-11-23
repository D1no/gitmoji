//! The human-readable vocabulary lanes in the project README.

use std::fmt::Write as _;

use gitmoji_triangle::vocabulary::{Slot, Vocabulary};

/// Render each lane in sheet order, keeping entries from the same group in one cell.
pub fn render(vocab: &Vocabulary) -> String {
    let mut output = String::from("| Lane | Group | Emojis and meanings |\n| --- | --- | --- |\n");
    for (slot, lane) in [
        (Slot::Area, "Area"),
        (Slot::Behaviour, "Behaviour"),
        (Slot::Intent, "Intent"),
    ] {
        for group in vocab.groups_in(slot) {
            let _ = write!(output, "| {lane} | {} | ", cell_text(&group.name));
            for (index, entry) in group.entries.iter().enumerate() {
                if index > 0 {
                    output.push_str("<br>");
                }
                let _ = write!(
                    output,
                    "{} **{}** — {}",
                    entry.emoji,
                    cell_text(&entry.name),
                    cell_text(&entry.meaning)
                );
            }
            output.push_str(" |\n");
        }
    }
    output
}

fn cell_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('|', "&#124;")
        .replace(['\r', '\n'], " ")
}
