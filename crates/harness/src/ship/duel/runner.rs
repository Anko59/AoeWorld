//! Runtime reports and red/green commands use bounded process-group capture.
use super::Options;
use crate::{
    agents::{Runtime, launch},
    process::{Cancellation, CaptureExit, Captured, capture_command},
    ship::git,
};
use std::{path::Path, process::Command};

pub(crate) fn tail(c: &Captured) -> String {
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&c.stdout),
        String::from_utf8_lossy(&c.stderr)
    );
    bounded(
        &text
            .lines()
            .rev()
            .take(40)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

pub(crate) fn bounded(text: &str) -> String {
    let mut start = text.len().saturating_sub(8000);
    while !text.is_char_boundary(start) {
        start += 1;
    }
    text[start..].to_owned()
}

pub(crate) fn report(runtime: Runtime, output: &str) -> String {
    let values: Vec<serde_json::Value> = output
        .lines()
        .filter_map(|s| serde_json::from_str(s).ok())
        .collect();
    for value in values.iter().rev() {
        let text = match runtime {
            Runtime::Claude => value
                .get("result")
                .and_then(|v| v.as_str())
                .map(str::to_owned),
            Runtime::Codex => value
                .get("item")
                .filter(|v| v["type"] == "agent_message")
                .and_then(|v| v["text"].as_str())
                .map(str::to_owned),
            Runtime::Pi => value
                .get("message")
                .filter(|v| v["role"] == "assistant")
                .and_then(|v| v["content"].as_array())
                .map(|parts| {
                    parts
                        .iter()
                        .filter_map(|v| v["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("\n")
                }),
            Runtime::Dsh => None,
        };
        if let Some(text) = text {
            return bounded(&text);
        }
    }
    bounded(output)
}

pub(crate) fn agent(
    root: &Path,
    options: &Options,
    role: &str,
    prompt: &str,
) -> git::Result<(CaptureExit, String)> {
    eprintln!(
        "duel: launching {} {role} (budget {}s)",
        options.runtime.label(),
        options.budget.as_secs()
    );
    let command = launch::work(
        options.runtime,
        root,
        role,
        prompt,
        options.model.as_deref(),
        options.effort.as_deref(),
    )?;
    let captured = capture_command(command, options.budget, &Cancellation::default());
    let output = String::from_utf8_lossy(&captured.stdout);
    Ok((captured.exit, report(options.runtime, &output)))
}

pub(crate) fn check(root: &Path, options: &Options, command: &str) -> git::Result<(bool, String)> {
    eprintln!("duel: {command}");
    // DUEL_CHECK is the person's trusted command, never a runtime's shell text.
    let mut c = Command::new("sh");
    c.args(["-c", command]).current_dir(root);
    for key in [
        "MAKEFLAGS",
        "MFLAGS",
        "MAKEOVERRIDES",
        "MAKEFILES",
        "GNUMAKEFLAGS",
    ] {
        c.env_remove(key);
    }
    let captured = capture_command(c, options.budget, &Cancellation::default());
    let tail = tail(&captured);
    match captured.exit {
        CaptureExit::Success => Ok((true, tail)),
        CaptureExit::Failed(Some(_)) => Ok((false, tail)),
        other => Err(format!("check could not complete: {other:?}\n{tail}")),
    }
}

pub(crate) fn default_check(paths: &[String]) -> &'static str {
    if paths.iter().any(|p| p.ends_with(".rs")) {
        "make test-unit"
    } else if paths
        .iter()
        .any(|p| p.ends_with(".spec.ts") || p.ends_with(".test.ts"))
    {
        "make test-e2e"
    } else {
        "make test-unit"
    }
}
