//! `make showcase-session SESSION=<jsonl>`: turns the log of a headless agent
//! session (`claude -p --output-format stream-json`, `codex exec --json`) into
//! the `steps` of an `agent-session` scene (docs/showcase.md). Read-only: it
//! reads one regular file and prints JSON. The result is bounded like the
//! scene itself, and strings that look like credentials are redacted.
use super::scenes::{MAX_SESSION_CHARS, MAX_STEP_CHARS, MAX_STEPS, Role, SessionStep, Verdict};
use serde_json::Value;
use std::{io::Read, path::Path};

/// A log larger than this is refused, not truncated.
const LOG_LIMIT: u64 = 8 * 1024 * 1024;
/// Characters that end a word; everything else (`@`, `/`, `.`, …) is part of one.
const SEPARATORS: &str = "\"'`=:,;()[]{}<>&|";
const REDACTED: &str = "[redacted]";
const SECRET_NAMES: &[&str] = &[
    "key",
    "token",
    "secret",
    "password",
    "passwd",
    "credential",
    "authorization",
    "auth",
];
const SECRET_PREFIXES: &[&str] = &[
    "sk-",
    "sk_",
    "ghp_",
    "gho_",
    "ghs_",
    "ghu_",
    "github_pat_",
    "xox",
    "akia",
    "aiza",
    "eyj",
    "glpat-",
    "npm_",
    "hf_",
];

pub(crate) fn run(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() {
        return Err(format!("{} must be a regular file", path.display()).into());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(LOG_LIMIT + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > LOG_LIMIT {
        return Err(format!("session log exceeds the {LOG_LIMIT}-byte limit").into());
    }
    let text = String::from_utf8(bytes).map_err(|e| format!("session log is not UTF-8: {e}"))?;
    let steps = convert(&text)?;
    println!("{}", serde_json::to_string_pretty(&steps)?);
    Ok(())
}

/// The bounded steps of a log; lines that are not JSON events are skipped.
pub(crate) fn convert(log: &str) -> Result<Vec<SessionStep>, String> {
    let mut steps = Vec::new();
    for event in log
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
    {
        claude_event(&event, &mut steps);
        codex_event(&event, &mut steps);
    }
    if steps.is_empty() {
        return Err(
            "no user, agent, tool or hook events found: is this a `claude -p \
                    --output-format stream-json` or `codex exec --json` log?"
                .into(),
        );
    }
    let mut total = 0;
    let mut kept = Vec::new();
    for step in steps.into_iter().take(MAX_STEPS) {
        total += step.text.chars().count();
        if total > MAX_SESSION_CHARS {
            break;
        }
        kept.push(step);
    }
    Ok(kept)
}

fn push(steps: &mut Vec<SessionStep>, role: Role, text: &str, verdict: Option<Verdict>) {
    let text = bound(&redact(text.trim()));
    if !text.is_empty() {
        steps.push(SessionStep {
            role,
            text,
            verdict,
        });
    }
}

fn bound(text: &str) -> String {
    if text.chars().count() <= MAX_STEP_CHARS {
        return text.to_owned();
    }
    let cut: String = text.chars().take(MAX_STEP_CHARS - 1).collect();
    format!("{cut}…")
}

/// Claude's stream-json: assistant and user messages made of content blocks.
fn claude_event(event: &Value, steps: &mut Vec<SessionStep>) {
    let role = event["type"].as_str();
    if !matches!(role, Some("assistant" | "user")) {
        return;
    }
    let content = &event["message"]["content"];
    if let Some(prompt) = content.as_str() {
        push(steps, Role::User, prompt, None);
    }
    for block in content.as_array().into_iter().flatten() {
        match block["type"].as_str() {
            Some("text") if role == Some("assistant") => {
                push(
                    steps,
                    Role::Agent,
                    block["text"].as_str().unwrap_or(""),
                    None,
                );
            }
            Some("text") => push(
                steps,
                Role::User,
                block["text"].as_str().unwrap_or(""),
                None,
            ),
            Some("tool_use") => {
                let call = tool_call(block["name"].as_str().unwrap_or("tool"), &block["input"]);
                push(steps, Role::Tool, &call, None);
            }
            // Only refusals and failures are shown; successful output would
            // crowd the transcript.
            Some("tool_result") if block["is_error"].as_bool() == Some(true) => {
                let text = result_text(&block["content"]);
                refusal(steps, &text);
            }
            _ => {}
        }
    }
}

/// A failed tool call: a hook or permission refusal is a denial, any other
/// failure is the tool's own output.
fn refusal(steps: &mut Vec<SessionStep>, text: &str) {
    let lower = text.to_lowercase();
    if ["hook", "denied", "not allowed", "refused", "permission"]
        .iter()
        .any(|word| lower.contains(word))
    {
        push(steps, Role::Hook, text, Some(Verdict::Denied));
    } else {
        push(steps, Role::Tool, text, None);
    }
}

fn result_text(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|block| block["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// `Bash: git push`, `Edit: src/lib.rs`: the one argument that says what ran.
fn tool_call(name: &str, input: &Value) -> String {
    let detail = [
        "command",
        "file_path",
        "path",
        "pattern",
        "url",
        "description",
    ]
    .iter()
    .find_map(|key| input[*key].as_str())
    .map_or_else(|| input.to_string(), str::to_owned);
    format!("{name}: {detail}")
}

/// Codex's `exec --json`: completed items (messages and commands).
fn codex_event(event: &Value, steps: &mut Vec<SessionStep>) {
    if event["type"].as_str() != Some("item.completed") {
        return;
    }
    let item = &event["item"];
    match item["type"].as_str() {
        Some("agent_message") => push(
            steps,
            Role::Agent,
            item["text"].as_str().unwrap_or(""),
            None,
        ),
        Some("command_execution") => {
            let command = item["command"].as_str().unwrap_or("");
            push(steps, Role::Tool, &format!("$ {command}"), None);
            if item["status"].as_str() == Some("declined") {
                push(steps, Role::Hook, "command declined", Some(Verdict::Denied));
            }
        }
        _ => {}
    }
}

/// Replace what looks like a credential with `[redacted]`: known token
/// prefixes, long mixed letter-and-digit runs, `Bearer` values and the value
/// after a secret-looking name (`API_KEY=…`, `"token": "…"`).
pub(crate) fn redact(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut secret_next = false;
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String, secret_next: &mut bool| {
        if word.is_empty() {
            return;
        }
        let lower = word.to_lowercase();
        // `Bearer` introduces the secret that follows it and stays readable.
        let scheme = lower == "bearer";
        let hidden = !scheme && (*secret_next || looks_secret(word));
        *secret_next = scheme;
        out.push_str(if hidden { REDACTED } else { word });
        word.clear();
    };
    let mut pending_name = false;
    for ch in text.chars() {
        if !ch.is_whitespace() && !SEPARATORS.contains(ch) {
            word.push(ch);
            continue;
        }
        let named = SECRET_NAMES
            .iter()
            .any(|name| word.to_lowercase().contains(name));
        if !word.is_empty() {
            pending_name = named;
        }
        flush(&mut word, &mut out, &mut secret_next);
        match ch {
            // A secret-looking name followed by `=` or `:` hides its value.
            '=' | ':' if pending_name => {
                secret_next = true;
                pending_name = false;
            }
            ' ' | '"' | '\'' | '\t' => {}
            _ => {
                pending_name = false;
                secret_next = false;
            }
        }
        out.push(ch);
    }
    flush(&mut word, &mut out, &mut secret_next);
    out
}

fn looks_secret(word: &str) -> bool {
    let lower = word.to_lowercase();
    if SECRET_PREFIXES.iter().any(|p| lower.starts_with(p)) && word.len() >= 12 {
        return true;
    }
    word.len() >= 32
        && word.chars().any(|c| c.is_ascii_digit())
        && word.chars().any(|c| c.is_ascii_alphabetic())
        && !word.contains('/')
        && !word.contains('.')
}
