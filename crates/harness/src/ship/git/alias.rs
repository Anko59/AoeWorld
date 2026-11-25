//! An `origin` cloned through an SSH host alias (`git@github-perso:owner/name`,
//! the alias defined in the person's SSH configuration) names GitHub only
//! through that configuration; `ssh -G` reads the host name it stands for.
use std::process::{Command, Stdio};

/// `url` with the host of an scp-like address (`[user@]host:path`) replaced by
/// the host name `hostname` returns for it. Other URLs, and hosts `hostname`
/// cannot resolve, are returned unchanged.
pub(crate) fn resolve(url: &str, hostname: impl Fn(&str) -> Option<String>) -> String {
    let Some((user_host, path)) = url.split_once(':') else {
        return url.to_owned();
    };
    if user_host.contains('/') || path.starts_with("//") {
        return url.to_owned();
    }
    let (user, host) = match user_host.rsplit_once('@') {
        Some((user, host)) => (Some(user), host),
        None => (None, user_host),
    };
    match hostname(host) {
        Some(name) if !name.is_empty() && name != host => match user {
            Some(user) => format!("{user}@{name}:{path}"),
            None => format!("{name}:{path}"),
        },
        _ => url.to_owned(),
    }
}

/// The host name SSH connects to for `alias`, from `ssh -G`; `None` when SSH
/// cannot run or prints none.
pub(crate) fn ssh_hostname(alias: &str) -> Option<String> {
    if alias == "github.com" {
        return None;
    }
    let output = Command::new("ssh")
        .args(["-G", "--", alias])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix("hostname "))
        .map(str::to_owned)
}
