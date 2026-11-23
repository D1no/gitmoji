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

/// A review permalink identifies where the feedback came from. Accept HTTPS hosts
/// independent of the forge; validating the link must also work offline.
pub fn valid_review_link(value: &str) -> bool {
    if value
        .chars()
        .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '\\' | '<' | '>' | '"'))
    {
        return false;
    }
    let Ok(uri) = value.parse::<ureq::http::Uri>() else {
        return false;
    };
    let Some(authority) = uri.authority() else {
        return false;
    };
    let host = authority.host();
    uri.scheme_str() == Some("https")
        && !host.is_empty()
        && !authority.as_str().contains('@')
        && (authority.as_str() == host || authority.port_u16().is_some())
}

/// The argument and the corresponding Git trailer both take exactly one permalink.
pub const REVIEW_LINK_ERROR: &str = "Review: needs one absolute HTTPS URL for the primary conversation. Omit the review trailers for verbal feedback.";
/// The stored list uses a comma and a space between finding links.
pub const ADDRESSES_LINK_ERROR: &str = "Addresses: needs one bracketed list of absolute HTTPS URLs. Separate links with a comma and a space. Encode a literal comma immediately before https:// in a URL as %2C.";

/// A single finding must round-trip through the bracketed list. A bare
/// `,https://` would also read as two links written without their separator.
pub fn valid_review_address(value: &str) -> bool {
    let ambiguous = b",https://";
    valid_review_link(value)
        && !value
            .as_bytes()
            .windows(ambiguous.len())
            .any(|part| part.eq_ignore_ascii_case(ambiguous))
}

/// Read the bracketed list of findings in the author's order. Ordinary commas in
/// paths and queries remain part of their URL.
pub fn review_addresses(value: &str) -> Option<Vec<&str>> {
    let inner = value.strip_prefix('[')?.strip_suffix(']')?;
    if inner.trim().is_empty() {
        return Some(Vec::new());
    }
    inner
        .split(", ")
        .map(|part| {
            let url = part.trim();
            valid_review_address(url).then_some(url)
        })
        .collect()
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

/// Validate the optional review provenance on an authored review response.
/// Ordinary commits can use these trailer names for their repository's own purposes.
pub fn check_review_links(records: &[Record]) -> Result<(), &'static str> {
    let mut primary = false;
    let mut addresses = false;
    for record in records {
        match record.key.as_str() {
            "review" => {
                if primary {
                    return Err(
                        "A review response has at most one Review: trailer. Use Addresses: for additional findings.",
                    );
                }
                if record.lines.len() != 1 || !valid_review_link(&record.value) {
                    return Err(REVIEW_LINK_ERROR);
                }
                primary = true;
            }
            "addresses" => {
                if addresses {
                    return Err(
                        "A review response has at most one Addresses: trailer. Combine the findings in one bracketed list.",
                    );
                }
                if record.lines.len() != 1 || review_addresses(&record.value).is_none() {
                    return Err(ADDRESSES_LINK_ERROR);
                }
                addresses = true;
            }
            _ => {}
        }
    }
    if addresses && !primary {
        return Err("Addresses: requires a primary Review: URL for the conversation.");
    }
    Ok(())
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
    fn addresses_preserve_order_ipv6_and_commas_that_belong_to_a_url() {
        let first = "https://[2001:db8::7]:8443/pull/13/files#diff-1";
        let second = "https://review.example/a,b?refs=one,two";
        let embedded = "https://review.example/go?refs=a%2Chttps://example.test/path";
        let list = format!("[{first}, {second}, {embedded}]");
        assert_eq!(review_addresses(&list), Some(vec![first, second, embedded]));
        assert_eq!(review_addresses("[]"), Some(Vec::new()));
        for bad in [
            "https://host/single",
            "[https://host/one, nope]",
            "[https://first.test/a,https://second.test/pull/1]",
            "[https://first.test/a,HTTPS://second.test/pull/1]",
            "[, https://host]",
            "[https://first, ]",
            "[http://host]",
        ] {
            assert_eq!(review_addresses(bad), None, "{bad}");
        }
    }

    #[test]
    fn review_links_accept_https_forges_without_fetching_the_source() {
        for url in [
            "https://github.com/sample/project/pull/17#discussion_r42",
            "https://review.ghe.com:8443/sample/project/pull/17/files#diff-abc",
            "https://github.com/sample/project/discussions/3#discussioncomment-12",
            "https://code.example/changes/13?view=review#comments",
        ] {
            assert!(valid_review_link(url), "{url}");
        }
        for invalid in [
            "https://",
            "https:///a/path",
            "http://code.example/42",
            "#42",
            "https://a:bad/p",
            "https://name:password@code.example/review",
            "https://host/x y",
            "https://host/review#one\nAddresses: https://host/other",
        ] {
            assert!(!valid_review_link(invalid), "{invalid}");
        }
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
