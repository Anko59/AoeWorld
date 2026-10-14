//! A brief SSH-agent hiccup is retried only for the exact publickey error.
use std::{
    io::{self, Write},
    process::{Command, Output},
    thread,
    time::Duration,
};

const BACKOFF: Duration = Duration::from_secs(20);
const PUBLICKEY: &str = "Permission denied (publickey)";

#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) use tests::without_sleep;

pub(crate) fn output(command: &mut Command) -> Result<Output, String> {
    output_with(command, |delay| {
        #[cfg(test)]
        if tests::record_wait(delay) {
            return;
        }
        thread::sleep(delay);
    })
}

/// Preserve the same argv, environment, lease and evidenced head on retry.
pub(crate) fn output_with(
    command: &mut Command,
    mut wait: impl FnMut(Duration),
) -> Result<Output, String> {
    for attempt in 0..2 {
        let output = command.output().map_err(|e| format!("git: {e}"))?;
        if output.status.success() {
            return Ok(output);
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        let publickey = stderr.contains(PUBLICKEY);
        if attempt == 0 && publickey {
            eprintln!("git: {PUBLICKEY}; retrying in {}s", BACKOFF.as_secs());
            wait(BACKOFF);
            continue;
        }
        let mut error = format!(
            "git remote command failed ({}): {}",
            output.status,
            stderr.trim()
        );
        if !output.stdout.is_empty() {
            error.push('\n');
            error.push_str(String::from_utf8_lossy(&output.stdout).trim());
        }
        if publickey {
            let socket = command
                .get_envs()
                .find(|(key, _)| *key == "SSH_AUTH_SOCK")
                .map(|(_, value)| value.map(|v| v.to_owned()))
                .unwrap_or_else(|| std::env::var_os("SSH_AUTH_SOCK"));
            error.push_str(&format!(
                "\nThe SSH agent at $SSH_AUTH_SOCK ({}) served no key; set GIT_SSH_COMMAND='ssh -i <key> -o IdentitiesOnly=yes'.",
                socket.map_or_else(|| "unset".into(), |v| v.to_string_lossy().into_owned())
            ));
        }
        return Err(error);
    }
    unreachable!("the last attempt returns")
}

/// Push output remains visible; fetching callers consume stdout themselves.
pub(crate) fn run_visible(command: &mut Command) -> Result<(), String> {
    command.stdout(std::process::Stdio::inherit());
    let output = output(command).map_err(|error| format!("git push failed: {error}"))?;
    io::stderr()
        .write_all(&output.stderr)
        .map_err(|e| e.to_string())?;
    Ok(())
}
