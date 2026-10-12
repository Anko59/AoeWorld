//! One seed case as a throwaway git repository: the review policy and the
//! case's base files on `origin/dev`, the case's diff as one commit on a
//! branch, so the normal review runs on it unchanged.
use super::seeds::Case;
use crate::{review::config::PERSONAS, review::trusted, ship::git};
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
};
use tempfile::TempDir;

const POLICY: [&str; 5] = [
    "gates/review.json",
    "gates/review/preamble.md",
    "gates/review/grader.md",
    "gates/review/arbiter.md",
    "gates/registry.json",
];

/// The review policy path hashed into the policy fingerprint must exist.
const ENGINE_STUB: &str = "crates/harness/src/review/stub.txt";

fn run(root: &Path, args: &[&str]) -> Result<String, String> {
    let identity = [
        "-c",
        "user.name=calibrate",
        "-c",
        "user.email=calibrate@localhost",
    ];
    let signing = [
        "-c",
        "commit.gpgsign=false",
        "-c",
        "core.hooksPath=/dev/null",
    ];
    let all: Vec<&str> = identity
        .iter()
        .chain(&signing)
        .chain(args)
        .copied()
        .collect();
    git::git(root, &all)
}

/// The registry of the review policy with every path in one `static` suite:
/// seed cases are judged by their content, not by where the real repository
/// would run its gates, so the floor never blocks a tier.
fn flatten(registry: &str) -> Result<String, String> {
    let mut value: serde_json::Value =
        serde_json::from_str(registry).map_err(|e| format!("gates/registry.json: {e}"))?;
    for suite in value["suites"].as_array_mut().into_iter().flatten() {
        let paths = match suite["id"].as_str() {
            Some("static") => serde_json::json!(["**"]),
            Some("everything") => serde_json::json!([]),
            _ => continue,
        };
        suite["paths"] = paths;
    }
    serde_json::to_string_pretty(&value).map_err(|e| e.to_string())
}

pub(super) fn write(root: &Path, relative: &str, text: &str) -> Result<(), String> {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap_or(root)).map_err(|e| e.to_string())?;
    fs::write(&path, text).map_err(|e| format!("{relative}: {e}"))
}

/// Build the repository of `case` with the review policy read from `source`
/// (the policy the real review would use). The directory lives until dropped.
pub(super) fn materialise(source: &Path, case: &Case) -> Result<TempDir, String> {
    let dir = tempfile::Builder::new()
        .prefix("aoe-calibrate-")
        .tempdir()
        .map_err(|e| e.to_string())?;
    let root = dir.path();
    run(root, &["init", "-q", "-b", "dev"])?;
    let mut policy: Vec<String> = POLICY.iter().map(|p| (*p).to_owned()).collect();
    policy.extend(
        PERSONAS
            .iter()
            .map(|p| format!("gates/review/personas/{p}.md")),
    );
    for relative in policy {
        let text = trusted(source, &relative)?;
        let text = if relative == "gates/registry.json" {
            flatten(&text)?
        } else {
            text
        };
        write(root, &relative, &text)?;
    }
    write(root, ENGINE_STUB, "calibration stub\n")?;
    super::hooks::install(source, root)?;
    for (path, text) in case.files() {
        write(root, path, &text)?;
    }
    run(root, &["add", "-A"])?;
    run(root, &["commit", "-q", "-m", "base"])?;
    run(root, &["update-ref", "refs/remotes/origin/dev", "HEAD"])?;
    run(root, &["checkout", "-q", "-b", "calibration"])?;
    apply(root, &case.patch())?;
    run(root, &["add", "-A"])?;
    run(root, &["commit", "-q", "-m", &format!("seed {}", case.id)])?;
    Ok(dir)
}

fn apply(root: &Path, patch: &str) -> Result<(), String> {
    let mut child = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["apply", "--whitespace=nowarn", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("git apply: {e}"))?;
    child
        .stdin
        .take()
        .ok_or("git apply: no stdin")?
        .write_all(patch.as_bytes())
        .map_err(|e| e.to_string())?;
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "git apply: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}
