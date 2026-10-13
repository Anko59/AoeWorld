//! The session converter's output is always a valid scene, and its redaction
//! covers the common credential forms without hiding ordinary text.
use super::super::super::showcase::{Storyboard, session::convert, session::redact};

fn claude_line(block: serde_json::Value, kind: &str) -> String {
    serde_json::json!({"type": kind, "message": {"content": [block]}}).to_string()
}

#[test]
fn control_characters_never_reach_the_scene() {
    let log = [
        claude_line(
            serde_json::json!({"type":"tool_use","name":"Bash",
                "input":{"command":"cat <<EOF\n\tindented\r\n\tline\rend\u{7}\u{0}\nEOF"}}),
            "assistant",
        ),
        claude_line(
            serde_json::json!({"type":"tool_result","is_error":true,
                "content":"\u{1b}[31merror\u{1b}[0m: \u{1b}]0;title\u{7}boom\u{1b}]8;;x\u{1b}\\!\u{9b}"}),
            "user",
        ),
    ]
    .join("\n");
    let steps = convert(&log).unwrap();
    assert_eq!(steps[0].text, "Bash: cat <<EOF\n indented\n line\nend\nEOF");
    assert_eq!(steps[1].text, "error: boom!");
    let scene = serde_json::json!({"kind":"agent-session","tag":"t","caption":"c","steps":steps});
    Storyboard::parse(&format!(r#"{{"title":"t","scenes":[{scene}]}}"#)).unwrap();
}

#[test]
fn each_credential_form_is_hidden() {
    for (input, expected) in [
        (
            "Authorization: Basic dXNlcjpwYXNzd29yZA==",
            "Authorization: Basic [redacted]",
        ),
        ("run --token abc123xyz now", "run --token [redacted] now"),
        ("x --api-key abc y", "x --api-key [redacted] y"),
        ("curl -u user:hunter2 url", "curl -u user:[redacted] url"),
        ("curl --user 'me:pw' url", "curl --user 'me:[redacted]' url"),
        ("export TOKEN hunter2", "export TOKEN [redacted]"),
        (
            "git clone https://bob:s3cret@example.com/r.git",
            "git clone https://bob:[redacted]@example.com/r.git",
        ),
        (
            r#"{"u":"postgres://app:pw@db:5432/x"}"#,
            r#"{"u":"postgres://app:[redacted]@db:5432/x"}"#,
        ),
    ] {
        assert_eq!(redact(input), expected);
    }
}

#[test]
fn ordinary_text_is_not_redacted() {
    for plain in [
        "git push -u origin main",
        "cargo test --lib -- --nocapture",
        "see src/main.rs:12:5 and https://example.com/a/b?x=1",
        "https://user@example.com/path",
        "commit 6ea73acb and 6ea73acbbbd9e1f40991a3b1c2d3e4f5a6b7c8d9",
        "Basic usage of the key board: press a key",
        "the token count is 5; --keyboard us",
        "export PATH /usr/bin",
    ] {
        assert_eq!(redact(plain), plain);
    }
}
