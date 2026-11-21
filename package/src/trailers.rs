//! The trailers of a commit message: the `Key: value` lines that end it.
//!
//! Two rules look at them: whether an agent is credited as co-author, and whether a
//! description says more than who was involved. They must agree with each other and
//! with git, so both read the same records from here.

/// Reads a line as the start of a trailer, the way git does: a key of letters, digits
/// and hyphens at the very start of the line, then a colon, with spaces allowed before
/// it. Gives the key in lower case and the value.
pub fn trailer(line: &str) -> Option<(String, &str)> {
    let (key, value) = line.split_once(':')?;
    let name = key.trim_end();
    let well_formed = name.starts_with(|c: char| c.is_ascii_alphabetic())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    well_formed.then(|| (name.to_lowercase(), value.trim()))
}

/// The sign that opens the closing block of a stored message, before the version of
/// the vocabulary the message was checked against: `△ `2.0.0``. The hook writes that
/// line. It is a plain triangle, not the 🔺 emoji, so that a search finds only it.
pub const VERSION_MARK: char = '△';

/// One trailer, with the lines it was written on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// The key in lower case, such as `co-authored-by`.
    pub key: String,
    /// The value, with a value folded over several lines joined by single spaces.
    pub value: String,
    /// Which of the lines given hold it: the first, and any indented ones that continue it.
    pub lines: Vec<usize>,
}

/// The trailers of a message body, given as its lines after the subject and the empty
/// line under it.
///
/// Only the last paragraph can hold trailers, as in git, and only when every line of it
/// is one or continues one. An agent's signature may come after that paragraph; `signed`
/// says whether a paragraph is one. Anything that merely looks like a trailer further
/// up, such as an example in the description, is not one.
pub fn records(lines: &[&str], signed: impl Fn(&[&str]) -> bool) -> Vec<Record> {
    let mut paragraphs: Vec<Vec<usize>> = vec![Vec::new()];
    for (index, line) in lines.iter().enumerate() {
        if line.trim().is_empty() {
            paragraphs.push(Vec::new());
        } else if let Some(last) = paragraphs.last_mut() {
            last.push(index);
        }
    }
    paragraphs.retain(|paragraph| !paragraph.is_empty());
    let text = |paragraph: &[usize]| -> Vec<&str> {
        paragraph.iter().map(|&index| lines[index]).collect()
    };
    if paragraphs
        .last()
        .is_some_and(|paragraph| signed(&text(paragraph)))
    {
        paragraphs.pop();
    }
    let Some(block) = paragraphs.last() else {
        return Vec::new();
    };
    let continues = |line: &str| line.starts_with([' ', '\t']);
    let is_block = trailer(lines[block[0]]).is_some()
        && block
            .iter()
            .all(|&index| trailer(lines[index]).is_some() || continues(lines[index]));
    if !is_block {
        return Vec::new();
    }
    let mut found = Vec::new();
    let mut at = 0;
    while at < block.len() {
        let mut end = at + 1;
        while end < block.len() && continues(lines[block[end]]) {
            end += 1;
        }
        let whole = text(&block[at..end])
            .iter()
            .map(|line| line.trim())
            .collect::<Vec<_>>()
            .join(" ");
        if let Some((key, value)) = trailer(&whole) {
            found.push(Record {
                key,
                value: value.to_string(),
                lines: block[at..end].to_vec(),
            });
        }
        at = end;
    }
    found
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn read(body: &str) -> Vec<Record> {
        let lines: Vec<&str> = body.lines().collect();
        records(&lines, |paragraph| {
            paragraph.first().is_some_and(|line| line.trim() == "_")
        })
    }

    #[test]
    fn trailer_should_read_a_key_as_git_does() {
        assert_eq!(
            trailer("Co-Authored-By : Ada <ada@example.com>"),
            Some(("co-authored-by".to_string(), "Ada <ada@example.com>"))
        );
        assert_eq!(
            trailer("Limitation: none"),
            Some(("limitation".to_string(), "none"))
        );
        assert_eq!(trailer(" Co-authored-by: indented, so not a trailer"), None);
        assert_eq!(trailer("We chose this: it is simpler"), None);
        assert_eq!(trailer("no colon here"), None);
    }

    #[test]
    fn records_should_read_only_the_last_paragraph() {
        let found = read(
            "The manual shows this example verbatim:\n\nCo-authored-by: Cursor <cursoragent@cursor.com>\n\nSigned-off-by: Ada <ada@example.com>\n",
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].key, "signed-off-by");
        assert_eq!(found[0].lines, vec![4]);
    }

    #[test]
    fn records_should_join_a_folded_trailer_and_look_past_a_signature() {
        let found = read(
            "Why.\n\nCO-AUTHORED-BY : Cursor\n <cursoragent@cursor.com>\nLimitation: none\n\n_\nmodel (high)\nHarness\n",
        );
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].key, "co-authored-by");
        assert_eq!(found[0].value, "Cursor <cursoragent@cursor.com>");
        assert_eq!(found[0].lines, vec![2, 3]);
        assert_eq!(found[1].key, "limitation");
    }

    #[test]
    fn records_should_find_none_where_prose_is_mixed_in() {
        assert!(
            read(
                "Why.\n\nCo-authored-by: Cursor <cursoragent@cursor.com>\nis what the tool adds.\n"
            )
            .is_empty()
        );
        assert!(read("Only a reason, no trailers.\n").is_empty());
        assert!(read("").is_empty());
    }
}
