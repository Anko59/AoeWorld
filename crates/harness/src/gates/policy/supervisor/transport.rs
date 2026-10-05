//! Fixed read-only daemon observations. No service admission or mutating actions.
mod paths;
#[cfg(test)]
mod tests;
use crate::process::{Cancellation, CaptureExit, Captured, capture_command, safe_observation};
use serde_json::{Value, json};
use std::{path::Path, process::Command, time::Duration};
const PROGRAM: &str = "/usr/bin/docker";
const HOST: &str = "unix:///run/aoeworld-supervisor/docker.sock";
const CONFIG: &str = "/etc/aoeworld/supervisor/docker-client";
const SOCKET: &str = "/run/aoeworld-supervisor/docker.sock";
const ARGS: [&str; 7] = [
    "--host",
    HOST,
    "--config",
    CONFIG,
    "info",
    "--format",
    "{{json .ID}}",
];
const LIMIT: usize = 4096;
/// No caller can choose a program, endpoint, configuration, CID or command.
pub(super) fn contract() -> Value {
    json!({"schema":1,"authoritative":false,"status":"READ_ONLY_CONTRACT","program":PROGRAM,"argv":ARGS,"cwd":"/","environment":{"HOME":"/var/empty","PATH":"/usr/bin:/bin","LANG":"C","LC_ALL":"C"},"capture_seconds":5,"response_bytes_per_stream":LIMIT,"mutating_actions":[],"limits":["no worker execution or lease mutation","metadata/daemon-ID observation is not service authentication","coding-host daemon control invalidates local authority qualification"]})
}
// Private primitive seam; the production caller supplies only the literals above.
fn isolated(program: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(program);
    command
        .env_clear()
        .current_dir("/")
        .args(args)
        .env("HOME", "/var/empty")
        .env("PATH", "/usr/bin:/bin")
        .env("LANG", "C")
        .env("LC_ALL", "C");
    command
}
fn unavailable(reason: &str) -> Value {
    json!({"schema":1,"authoritative":false,"status":"UNAVAILABLE","reason":reason,"admission_granted":false,"limits":["read-only fixed info observation, no workers or daemon-container cleanup","root-owned paths and daemon ID cannot prove peer/operator/deployment authentication","same-user/reverted races, ACLs, container-root and coding-host daemon control remain limits","filesystem observations are not wall supervised"]})
}
fn observed(captured: Captured) -> Value {
    let observation = safe_observation(&captured);
    let mut report =
        unavailable("daemon probe did not produce a complete bounded successful observation");
    report["exit"] = json!(observation.outcome.exit_label());
    report["truncated"] = json!(observation.truncated);
    report["duration_ms"] = json!(observation.duration_ms);
    report["stdout_blake3"] = json!(observation.stdout.raw_blake3);
    report["stderr_blake3"] = json!(observation.stderr.raw_blake3);
    report["capture_observation"] = json!(observation);
    if !matches!(captured.exit, CaptureExit::Success)
        || captured.truncated
        || captured.stdout.len() > LIMIT
        || captured.stderr.len() > LIMIT
    {
        return report;
    }
    if !std::str::from_utf8(&captured.stderr).is_ok_and(|text| {
        !text
            .chars()
            .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\r' | '\t'))
    }) {
        return report;
    }
    let Ok(id) = serde_json::from_slice::<String>(&captured.stdout) else {
        return report;
    };
    if id.is_empty()
        || id.len() > 256
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'-' | b'_' | b'.'))
    {
        report["reason"] = json!("daemon identity is not a bounded printable identifier");
        return report;
    }
    report["status"] = json!("PROBED_NON_AUTHORITATIVE");
    report["reason"] =
        json!("fixed endpoint reported an ID; operator/deployment authority not established");
    report["observed_daemon_id"] = json!(id);
    report
}
fn capture(program: &Path, args: &[&str], deadline: Duration, cancel: &Cancellation) -> Value {
    observed(capture_command(isolated(program, args), deadline, cancel))
}
pub(super) fn probe(cancel: &Cancellation) -> Value {
    if cancel.cancelled() {
        return unavailable("daemon probe cancelled before filesystem observation");
    }
    let initial = match paths::fixed() {
        Ok(value) => value,
        Err(reason) => return unavailable(reason),
    };
    if cancel.cancelled() {
        return unavailable("daemon probe cancelled before spawn");
    }
    let report = capture(Path::new(PROGRAM), &ARGS, Duration::from_secs(5), cancel);
    match paths::fixed() {
        Ok(final_state) if initial == final_state => {
            let mut result = report;
            result["limits"] = json!([
                "read-only info only; no Docker create/start/stop/kill/remove",
                "root-owned paths and reported ID are not peer/server/operator authentication",
                "same-user and reverted races, ACLs and container-root namespaces remain limits",
                "filesystem observations are not wall supervised; command capture is bounded",
                "killing the Docker client group does not prove daemon-container cleanup"
            ]);
            result
        }
        _ => unavailable("fixed daemon transport paths changed or became unavailable"),
    }
}
