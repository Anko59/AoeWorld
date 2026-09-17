//! Read-only GitHub repository policy audit.
use serde_json::Value;
use std::{
    error::Error,
    io::Write,
    process::{Command, Stdio},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn repository() -> Result<String> {
    let output = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .output()?;
    if !output.status.success() {
        return Err("cannot read origin remote".into());
    }
    let remote = String::from_utf8(output.stdout)?;
    let remote = remote.trim();
    let path = remote
        .strip_prefix("git@github.com:")
        .or_else(|| remote.strip_prefix("https://github.com/"))
        .ok_or("origin is not a GitHub repository")?;
    let path = path.strip_suffix(".git").unwrap_or(path);
    let (owner, name) = path.split_once('/').ok_or("invalid GitHub origin")?;
    if name.contains('/')
        || owner.is_empty()
        || name.is_empty()
        || !path
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_' | b'.'))
    {
        return Err("invalid GitHub repository path".into());
    }
    Ok(path.to_owned())
}

fn api(path: &str) -> Result<Value> {
    let token = std::env::var("GITHUB_TOKEN").ok();
    if token.as_ref().is_some_and(|value| {
        !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    }) {
        return Err("GITHUB_TOKEN contains unexpected characters".into());
    }
    let mut command = Command::new("curl");
    command.args([
        "--fail-with-body",
        "--silent",
        "--show-error",
        "--config",
        "-",
        "--header",
        "Accept: application/vnd.github+json",
        "--header",
        "X-GitHub-Api-Version: 2026-03-10",
    ]);
    command.arg(format!("https://api.github.com/repos/{path}"));
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn()?;
    if let Some(mut input) = child.stdin.take()
        && let Some(token) = token
    {
        input.write_all(format!("header = \"Authorization: Bearer {token}\"\n").as_bytes())?;
    }
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(format!(
            "GitHub policy API {path}: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn validate(repo: &Value, branch: &Value, signatures: &Value, name: &str) -> Result<()> {
    if repo["default_branch"] != "main"
        || repo["allow_squash_merge"] != true
        || repo["allow_merge_commit"] != false
        || repo["allow_rebase_merge"] != false
    {
        return Err("repository merge strategy or default branch drifted".into());
    }
    let contexts = branch["required_status_checks"]["contexts"]
        .as_array()
        .ok_or("required status checks missing")?;
    if branch["required_status_checks"]["strict"] != true
        || !contexts.iter().any(|value| value == "required")
    {
        return Err(format!("{name}: strict aggregate check missing").into());
    }
    if branch["enforce_admins"]["enabled"] != true
        || branch["required_pull_request_reviews"].is_null()
        || branch["required_linear_history"]["enabled"] != true
        || branch["allow_force_pushes"]["enabled"] != false
        || branch["allow_deletions"]["enabled"] != false
        || signatures["enabled"] != true
    {
        return Err(format!("{name}: protected branch settings drifted").into());
    }
    Ok(())
}

pub fn check() -> Result<()> {
    let repository = repository()?;
    let repo = api(&repository)?;
    for name in ["dev", "main"] {
        let branch = api(&format!("{repository}/branches/{name}/protection"))?;
        let signatures = api(&format!(
            "{repository}/branches/{name}/protection/required_signatures"
        ))?;
        validate(&repo, &branch, &signatures, name)?;
    }
    println!("GitHub policy verified for {repository}: dev and main");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_drift() {
        let repo = serde_json::json!({"default_branch":"main","allow_squash_merge":true,"allow_merge_commit":false,"allow_rebase_merge":false});
        let mut branch = serde_json::json!({"required_status_checks":{"strict":true,"contexts":["required"]},"enforce_admins":{"enabled":true},"required_pull_request_reviews":{},"required_linear_history":{"enabled":true},"allow_force_pushes":{"enabled":false},"allow_deletions":{"enabled":false}});
        let signatures = serde_json::json!({"enabled":true});
        assert!(validate(&repo, &branch, &signatures, "dev").is_ok());
        branch["required_status_checks"]["strict"] = false.into();
        assert!(validate(&repo, &branch, &signatures, "dev").is_err());
    }
}
