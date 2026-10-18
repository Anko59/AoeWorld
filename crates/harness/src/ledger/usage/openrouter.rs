//! OpenRouter credits from its account endpoint, with the key the harness
//! already keeps in the keyring. The key goes to the endpoint on standard
//! input of `curl` and is never printed or stored.
use serde_json::Value;
use std::{
    io::Write,
    process::{Command, Stdio},
};

const URL: &str = "https://openrouter.ai/api/v1/credits";

/// Credits bought and used, in dollars, from the endpoint's JSON.
pub(super) fn parse_credits(text: &str) -> Result<(f64, f64), String> {
    let value: Value =
        serde_json::from_str(text).map_err(|_| "the reply was not JSON".to_owned())?;
    if let Some(message) = value["error"]["message"].as_str() {
        return Err(format!("OpenRouter refused: {message}"));
    }
    let data = &value["data"];
    match (data["total_credits"].as_f64(), data["total_usage"].as_f64()) {
        (Some(total), Some(used)) => Ok((total, used)),
        _ => Err("the reply had no total_credits and total_usage".to_owned()),
    }
}

pub(super) fn render(reading: Result<(f64, f64), String>) -> String {
    match reading {
        Ok((total, used)) => format!(
            "OpenRouter\n  credits left: ${:.2} (${used:.2} used of ${total:.2} bought)\n",
            total - used
        ),
        Err(reason) => format!("OpenRouter\n  unknown: {reason}\n"),
    }
}

/// The key from the environment, else the keyring (docs/credentials.md).
fn key() -> Option<String> {
    if let Some(value) = std::env::var("OPENROUTER_API_KEY")
        .ok()
        .filter(|v| !v.is_empty())
    {
        return Some(value);
    }
    let output = Command::new("secret-tool")
        .args([
            "lookup",
            "service",
            "codex-api",
            "name",
            "OPENROUTER_API_KEY",
        ])
        .output()
        .ok()?;
    let value = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (output.status.success() && !value.is_empty()).then_some(value)
}

fn fetch(key: &str) -> Result<String, String> {
    let mut child = Command::new("curl")
        .args(["-q", "-sS", "--max-time", "20", "-H", "@-", URL])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("curl could not start: {e}"))?;
    let header = format!("Authorization: Bearer {key}\n");
    child
        .stdin
        .take()
        .ok_or("curl has no stdin")?
        .write_all(header.as_bytes())
        .map_err(|e| format!("curl rejected its header: {e}"))?;
    let output = child
        .wait_with_output()
        .map_err(|e| format!("curl failed: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "the request failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub(super) fn section() -> String {
    let Some(key) = key() else {
        return render(Err(
            "no OPENROUTER_API_KEY in the environment or the keyring".to_owned(),
        ));
    };
    render(fetch(&key).and_then(|text| parse_credits(&text)))
}
