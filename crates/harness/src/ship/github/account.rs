//! `AOE_GH_ACCOUNT` for a judge that runs on a Mac (docs/local-development.md):
//! which of the Mac's `gh` accounts `gh`, and `git` over https, use. The token
//! comes from `gh auth token` at each call; it is never written down or put on
//! a command line. An OrbStack machine's `gh` picks the account by itself, so
//! Linux is left alone.
use std::process::Command;

const ACCOUNT: &str = "AOE_GH_ACCOUNT";

/// Git asks this helper for github.com only. Its text holds no token: the
/// password goes from `gh` to Git through pipes, and a refusal stops Git
/// instead of trying another login.
const HELPER: &str = r#"!f() { test "$1" = get || return 0; t=$(gh auth token --hostname github.com --user "$AOE_GH_ACCOUNT") && printf 'username=%s\npassword=%s\n' "$AOE_GH_ACCOUNT" "$t" || echo quit=1; }; f"#;
const HELPER_KEY: &str = "credential.https://github.com.helper";

fn selected() -> Option<String> {
    selected_in(cfg!(target_os = "macos"), |name| std::env::var(name).ok())
}

fn selected_in(mac: bool, get: impl Fn(&str) -> Option<String>) -> Option<String> {
    get(ACCOUNT).filter(|account| mac && !account.is_empty())
}

/// `gh`, with the selected account's token in that process's environment.
pub(crate) fn gh() -> Result<Command, String> {
    gh_as(selected().as_deref(), token)
}

fn gh_as(
    account: Option<&str>,
    token: impl FnOnce(&str) -> Result<String, String>,
) -> Result<Command, String> {
    let mut command = Command::new("gh");
    if let Some(account) = account {
        let token = token(account).map_err(|error| format!("{ACCOUNT}={account}: {error}"))?;
        command.env("GH_TOKEN", token);
    }
    Ok(command)
}

fn token(account: &str) -> Result<String, String> {
    let output = Command::new("gh")
        .args([
            "auth",
            "token",
            "--hostname",
            "github.com",
            "--user",
            account,
        ])
        .output()
        .map_err(|error| format!("gh: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// A `git` that talks to `origin`: over https it authenticates as the account.
pub(crate) fn git(command: &mut Command) {
    if let Some(account) = selected() {
        let configured = std::env::var("GIT_CONFIG_COUNT")
            .ok()
            .and_then(|count| count.parse().ok())
            .unwrap_or(0);
        git_as(command, Some(&account), configured);
    }
}

/// Adds the helper after the `configured` settings of the caller's environment;
/// an empty helper first drops the ones configured for other logins.
fn git_as(command: &mut Command, account: Option<&str>, configured: usize) {
    let Some(account) = account else {
        return;
    };
    command.env(ACCOUNT, account);
    command.env("GIT_CONFIG_COUNT", (configured + 2).to_string());
    for (index, helper) in (configured..).zip(["", HELPER]) {
        command.env(format!("GIT_CONFIG_KEY_{index}"), HELPER_KEY);
        command.env(format!("GIT_CONFIG_VALUE_{index}"), helper);
    }
}

#[cfg(test)]
mod tests;
