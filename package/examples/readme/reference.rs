//! The human-readable vocabulary lanes in the project README.

use std::fmt::Write as _;

use gitmoji_triangle::vocabulary::{Slot, Vocabulary};

/// Keep each lane in one cell so its subsections can be read independently.
pub fn render(vocab: &Vocabulary) -> String {
    let lanes = [
        (Slot::Area, "Area"),
        (Slot::Behaviour, "Behavior"),
        (Slot::Intent, "Intent"),
    ];
    let mut output = String::from("<table>\n<thead><tr>");
    for (_, title) in lanes {
        let _ = write!(output, "<th>{title}</th>");
    }
    output.push_str("</tr></thead>\n<tbody><tr>\n");
    for (slot, _) in lanes {
        output.push_str("<td valign=\"top\" width=\"33%\">\n");
        for group in vocab.groups_in(slot) {
            let _ = writeln!(output, "<h3>{}</h3>", html_text(&group.name));
            for entry in &group.entries {
                let _ = writeln!(
                    output,
                    "<p>{} <strong>{}</strong> — {}</p>",
                    html_text(&entry.emoji),
                    html_text(&entry.name),
                    html_text(&entry.meaning)
                );
            }
        }
        output.push_str("</td>\n");
    }
    output.push_str("</tr></tbody>\n</table>\n");
    output
}

fn html_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
