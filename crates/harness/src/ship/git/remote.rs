//! A brief SSH-agent hiccup is retried only for the exact publickey error.
use std::{
    io::{self, Read, Write},
    process::{Command, Output, Stdio},
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
    wait: impl FnMut(Duration),
) -> Result<Output, String> {
    output_using(command, Command::output, wait)
}

fn output_using(
    command: &mut Command,
    mut run: impl FnMut(&mut Command) -> io::Result<Output>,
    mut wait: impl FnMut(Duration),
) -> Result<Output, String> {
    for attempt in 0..2 {
        let output = run(command).map_err(|e| format!("git: {e}"))?;
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
        if stderr.contains("(non-fast-forward)") {
            error.push_str(
                "\nThe branch on origin is not an ancestor of this head (it was rebased): ship again \
                 with SHIP_FORCE=1 to push with a lease.",
            );
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

/// Push output remains visible while it runs (a pre-push hook prints its gates
/// for minutes); stderr is also kept so a publickey refusal can be retried.
pub(crate) fn run_visible(command: &mut Command) -> Result<(), String> {
    output_using(
        command,
        |command| tee_stderr(command, &mut io::stderr()),
        |delay| {
            #[cfg(test)]
            if tests::record_wait(delay) {
                return;
            }
            thread::sleep(delay);
        },
    )
    .map_err(|error| format!("git push failed: {error}"))?;
    Ok(())
}

/// Runs `command` with stdout inherited and stderr forwarded to `sink` as it
/// arrives, returning the stderr it forwarded.
pub(crate) fn tee_stderr(command: &mut Command, sink: &mut impl Write) -> io::Result<Output> {
    let mut child = command
        .stdout(Stdio::inherit())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stderr = Vec::new();
    if let Some(mut pipe) = child.stderr.take() {
        let mut chunk = [0u8; 4096];
        loop {
            let read = pipe.read(&mut chunk)?;
            if read == 0 {
                break;
            }
            sink.write_all(&chunk[..read])?;
            sink.flush()?;
            stderr.extend_from_slice(&chunk[..read]);
        }
    }
    let status = child.wait()?;
    Ok(Output {
        status,
        stdout: Vec::new(),
        stderr,
    })
}
