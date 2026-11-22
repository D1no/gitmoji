//! Render a GitHub merge denial as a short, single-line diagnostic.

use base64::{Engine, engine::general_purpose::STANDARD_NO_PAD};
use regex::Regex;
use serde_json::Value;

// A merge error describes a rule or condition. Larger messages can be proxy
// documents or logs, and replacing secrets must happen before truncation.
const MAX_INPUT_BYTES: usize = 4096;
const MAX_DISPLAY_CHARS: usize = 240;

pub(super) fn display(value: &Value, credential: &str) -> Option<String> {
    let message = value.get("message")?.as_str()?;
    if message.len() > MAX_INPUT_BYTES
        || message.chars().any(|c| {
            c.is_control() && !matches!(c, '\n' | '\r' | '\t')
                || matches!(c, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{206f}')
        })
    {
        return None;
    }
    let markup = Regex::new(r"(?i)<[!/?a-z][^>]*>").ok()?;
    if markup.is_match(message) {
        return None;
    }

    let mut reason = if credential.is_empty() {
        message.to_owned()
    } else {
        message.replace(credential, "[redacted]")
    };
    reason = reason.split_whitespace().collect::<Vec<_>>().join(" ");
    if reason.is_empty() {
        return None;
    }
    // Keep the rule preceding a reflected authentication header. Omit the
    // rest rather than attempting to parse arbitrary auth schemes and values.
    let authorization = Regex::new(r"(?i)\b(?:(?:proxy-)?authorization\s*[:=]|bearer\s+\S|https?://[^/\s@]+@|(?:access[_-]?token|password|token|client_secret)\s*[:=])").ok()?;
    // A custom check can itself be named "Basic coverage". A Basic
    // credential encodes a user and password separated by a colon.
    let basic = Regex::new(r"(?i)\bbasic\s+([a-z0-9+/]+)={0,2}").ok()?;
    let basic_at = basic.captures_iter(&reason).find_map(|capture| {
        let decoded = STANDARD_NO_PAD.decode(capture.get(1)?.as_str()).ok()?;
        decoded.contains(&b':').then_some(capture.get(0)?.start())
    });
    if let Some(start) = authorization
        .find(&reason)
        .map(|m| m.start())
        .into_iter()
        .chain(basic_at)
        .min()
    {
        reason.truncate(start);
        reason.push_str("[redacted]");
    }
    let token = Regex::new(r"(?i)(?:github_pat_|gh[pousr]_)[a-z0-9_-]+(?:\.[a-z0-9_-]+)*").ok()?;
    let reason = token.replace_all(&reason, "[redacted]");
    if reason.chars().count() <= MAX_DISPLAY_CHARS {
        Some(reason.into_owned())
    } else {
        let mut short: String = reason.chars().take(MAX_DISPLAY_CHARS - 1).collect();
        short.push('…');
        Some(short)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn displays_the_host_rule_and_only_the_message_field() {
        let reason = "GHES rule «Basic coverage» requires sign-off from the release captain.";
        let body =
            json!({"message": reason, "errors": ["private detail"], "documentation_url": "unused"});
        assert_eq!(display(&body, "").as_deref(), Some(reason));
        for body in [
            json!("<html>Denied</html>"),
            json!({"error":"Denied"}),
            json!({"message":"<html>Denied</html>"}),
            json!({"message":7}),
            json!({"message":" \t "}),
        ] {
            assert!(display(&body, "").is_none());
        }
    }

    #[test]
    fn keeps_output_on_one_line_and_rejects_terminal_or_directional_controls() {
        assert_eq!(
            display(&json!({"message":"Gate one\r\n\tand gate two"}), "").as_deref(),
            Some("Gate one and gate two")
        );
        for control in [
            '\0', '\u{1b}', '\u{009b}', '\u{061c}', '\u{202e}', '\u{2067}',
        ] {
            assert!(display(&json!({"message": format!("Denied {control} forged")}), "").is_none());
        }
    }

    #[test]
    fn redacts_credentials_and_auth_headers_before_truncation() {
        let body = json!({"message":"Denied: arbitrary.fixture and github_pat_example_123 and ghs_example123. Authorization: Custom token details"});
        assert_eq!(
            display(&body, "arbitrary.fixture").as_deref(),
            Some("Denied: [redacted] and [redacted] and [redacted]. [redacted]")
        );
        for auth in [
            "Bearer arbitrary-token",
            "Basic YWxwaGE6YmV0YQ==",
            "Proxy-Authorization: Scheme credential",
            "https://user:fixture@example.invalid/",
            "access_token=fixture",
            "password: fixture",
        ] {
            assert_eq!(
                display(&json!({"message": format!("Denied. {auth}")}), "").as_deref(),
                Some("Denied. [redacted]")
            );
        }
        let stateless = json!({"message": "Denied ghs_123_header.payload-part.sign_ature. Rule X requires approval."});
        assert_eq!(
            display(&stateless, "").as_deref(),
            Some("Denied [redacted]. Rule X requires approval.")
        );
        let long = json!({"message": format!("{}fixture-secret", "é".repeat(236))});
        let short = display(&long, "fixture-secret").unwrap_or_default();
        assert!(short.ends_with('…'));
        assert_eq!(short.chars().count(), MAX_DISPLAY_CHARS);
        assert!(!short.contains("fixt"));
        assert!(display(&json!({"message": "x".repeat(MAX_INPUT_BYTES + 1)}), "").is_none());
    }
}
