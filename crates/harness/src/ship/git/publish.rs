use crate::ship::{Result, evidence::Evidence, git, prepush};
use std::{path::Path, process::Command};

/// Git opens the SSH connection before the pre-push hook runs `make
/// preflight` for several minutes; without keepalives GitHub drops the idle
/// connection and the push dies silently (#209). A caller's own SSH
/// command (`GIT_SSH_COMMAND`, `GIT_SSH` or `core.sshCommand`) wins.
pub(in crate::ship) fn ssh_keepalive(caller_chose_ssh: bool) -> Option<&'static str> {
    (!caller_chose_ssh).then_some("ssh -o ServerAliveInterval=30 -o ServerAliveCountMax=40")
}

pub(in crate::ship) fn caller_chose_ssh(root: &Path) -> bool {
    std::env::var_os("GIT_SSH_COMMAND").is_some()
        || std::env::var_os("GIT_SSH").is_some()
        || git::git(root, &["config", "--get", "core.sshCommand"]).is_ok()
}

/// `git push` naming the evidenced commit, so the pre-push hook can reuse
/// this ship's preflight evidence instead of running it again.
pub(in crate::ship) fn push_command(root: &Path, head: &str) -> Command {
    let mut command = Command::new("git");
    command.arg("-C").arg(root).arg("push");
    command.env(prepush::ENV, head);
    command
}

pub(in crate::ship) fn push(root: &Path, evidence: &Evidence, lease: Option<&str>) -> Result<()> {
    // The remote is fixed and the refspec names the evidenced branch only.
    let refspec = format!("{}:refs/heads/{}", evidence.head, evidence.branch);
    let mut command = push_command(root, &evidence.head);
    if let Some(from) = lease {
        // A re-check rebase of a branch already pushed: overwrite only `from`.
        command.arg(format!(
            "--force-with-lease=refs/heads/{}:{from}",
            evidence.branch
        ));
    }
    if let Some(ssh) = ssh_keepalive(caller_chose_ssh(root)) {
        command.env("GIT_SSH_COMMAND", ssh);
    }
    command.args(["origin", &refspec]);
    git::remote::run_visible(&mut command)?;
    // Best effort: a commit-id refspec cannot set the upstream itself.
    let _ = git::git(
        root,
        &[
            "branch",
            "--set-upstream-to",
            &format!("origin/{}", evidence.branch),
        ],
    );
    Ok(())
}
