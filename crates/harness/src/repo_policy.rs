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
    parse_remote(remote.trim())
}

fn parse_remote(remote: &str) -> Result<String> {
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
    api_url(
        &format!("https://api.github.com/repos/{path}"),
        token.as_deref(),
    )
}

fn api_url(url: &str, token: Option<&str>) -> Result<Value> {
    if token.is_some_and(|value| {
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
    command.arg(url);
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
            "GitHub policy API {url}: {}",
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
        || (name == "main" && !contexts.iter().any(|value| value == "release-candidate"))
    {
        return Err(format!("{name}: required checks or strict mode missing").into());
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
    check_with(&repository, api)
}

fn check_with<F>(repository: &str, mut fetch: F) -> Result<()>
where
    F: FnMut(&str) -> Result<Value>,
{
    let repo = fetch(repository)?;
    for name in ["dev", "main"] {
        let branch = fetch(&format!("{repository}/branches/{name}/protection"))?;
        let signatures = fetch(&format!(
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
        branch["required_status_checks"]["strict"] = true.into();
        branch["allow_force_pushes"]["enabled"] = true.into();
        assert!(validate(&repo, &branch, &signatures, "dev").is_err());
        branch["allow_force_pushes"]["enabled"] = false.into();
        assert!(validate(&repo, &branch, &serde_json::json!({"enabled":false}), "dev").is_err());
        let wrong_repo = serde_json::json!({"default_branch":"dev","allow_squash_merge":true,"allow_merge_commit":false,"allow_rebase_merge":false});
        assert!(validate(&wrong_repo, &branch, &signatures, "main").is_err());
        assert!(validate(&repo, &branch, &signatures, "main").is_err());
        branch["required_status_checks"]["contexts"] =
            serde_json::json!(["required", "release-candidate"]);
        assert!(validate(&repo, &branch, &signatures, "main").is_ok());
    }

    #[test]
    fn only_unambiguous_github_origins_are_accepted() {
        assert_eq!(
            parse_remote("git@github.com:Anko59/AoeWorld.git").expect("SSH"),
            "Anko59/AoeWorld"
        );
        assert_eq!(
            parse_remote("https://github.com/Anko59/AoeWorld").expect("HTTPS"),
            "Anko59/AoeWorld"
        );
        for remote in [
            "https://example.com/Anko59/AoeWorld",
            "git@github.com:Anko59",
            "git@github.com:Anko59/AoeWorld/other",
            "git@github.com:/AoeWorld",
            "git@github.com:Anko59/AoeWorld?x=1",
        ] {
            assert!(parse_remote(remote).is_err(), "{remote}");
        }
    }

    #[test]
    fn audit_fetches_both_protected_branches_and_detects_drift() {
        let repo = serde_json::json!({"default_branch":"main","allow_squash_merge":true,"allow_merge_commit":false,"allow_rebase_merge":false});
        let branch = serde_json::json!({"required_status_checks":{"strict":true,"contexts":["required","release-candidate"]},"enforce_admins":{"enabled":true},"required_pull_request_reviews":{},"required_linear_history":{"enabled":true},"allow_force_pushes":{"enabled":false},"allow_deletions":{"enabled":false}});
        let signature = serde_json::json!({"enabled":true});
        let mut paths = Vec::new();
        check_with("owner/repo", |path| {
            paths.push(path.to_owned());
            if path == "owner/repo" {
                Ok(repo.clone())
            } else if path.ends_with("required_signatures") {
                Ok(signature.clone())
            } else {
                Ok(branch.clone())
            }
        })
        .expect("protected branches");
        assert_eq!(paths.len(), 5);
        assert_eq!(paths[1], "owner/repo/branches/dev/protection");
        assert_eq!(paths[3], "owner/repo/branches/main/protection");
        assert!(check_with("owner/repo", |_| Ok(serde_json::json!({}))).is_err());
    }

    #[test]
    fn github_api_handles_token_validation_and_http_failures() {
        assert!(api_url("http://127.0.0.1:1", Some("bad token")).is_err());
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener");
        let port = listener.local_addr().expect("address").port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("request");
            let mut request = [0u8; 4096];
            let count = std::io::Read::read(&mut stream, &mut request).expect("read");
            let request = String::from_utf8_lossy(&request[..count]);
            assert!(request.contains("Authorization: Bearer test_token"));
            std::io::Write::write_all(
                &mut stream,
                b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}",
            )
            .expect("response");
        });
        let value =
            api_url(&format!("http://127.0.0.1:{port}"), Some("test_token")).expect("API response");
        assert_eq!(value["ok"], true);
        server.join().expect("server");
        assert!(api_url("http://127.0.0.1:1", None).is_err());
    }
}
