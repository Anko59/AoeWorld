//! Containers outlive a host suspend: killing the `docker run` client leaves its
//! container running. Each harness container carries the boot-time gap at its start
//! ([`LABEL`]); one whose gap has since moved was started before a suspend, so
//! its supervisor was interrupted and nothing else will stop it.
use crate::process::{self, Cancellation, CaptureExit, suspend};
use std::{error::Error, process::Command, time::Duration};

const DOCKER_DEADLINE: Duration = Duration::from_secs(60);
/// `ID<TAB>gap` for each container carrying the label; the gap is empty when unknown.
const FORMAT: &str = "{{.ID}}\t{{.Label \"aoe.harness-boot-gap\"}}";

/// Containers in a listing (`ID<TAB>gap`) whose gap moved since they started, with their gap.
/// Lines without a readable gap (a run that had no clock) are never judged.
pub(crate) fn interrupted(listing: &str, now: Duration) -> Vec<(String, u64)> {
    listing
        .lines()
        .filter_map(|line| {
            let (id, started) = line.split_once('\t')?;
            let started: u64 = started.trim().parse().ok()?;
            suspend::moved(Duration::from_secs(started), now).then(|| (id.to_owned(), started))
        })
        .collect()
}

fn docker(arguments: &[&str]) -> Result<String, Box<dyn Error>> {
    let mut command = Command::new("docker");
    command.args(arguments);
    let captured = process::capture_command(command, DOCKER_DEADLINE, &Cancellation::default());
    match captured.exit {
        CaptureExit::Success => Ok(String::from_utf8_lossy(&captured.stdout).into_owned()),
        _ => Err(format!(
            "docker {}: {}",
            arguments.join(" "),
            String::from_utf8_lossy(&captured.stderr).trim()
        )
        .into()),
    }
}

/// Run `stop` on every container, even after one fails; the failures are reported together.
pub(crate) fn stop_all(
    stale: &[(String, u64)],
    mut stop: impl FnMut(&str) -> Result<(), Box<dyn Error>>,
) -> Result<(), String> {
    let failures: Vec<String> = stale
        .iter()
        .filter_map(|(id, _)| stop(id).err().map(|error| format!("{id}: {error}")))
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}

/// Stop every harness container that a host suspend interrupted; returns what was stopped.
pub(crate) fn stop_interrupted() -> Result<Vec<(String, u64)>, Box<dyn Error>> {
    // The real clock: a simulated suspend must never remove a container a real run started.
    let now = suspend::read_real()?.gap();
    let label = format!("label={}", suspend::LABEL);
    let listing = docker(&["ps", "-a", "--filter", &label, "--format", FORMAT])?;
    let stale = interrupted(&listing, now);
    stop_all(&stale, |id| docker(&["rm", "-f", id]).map(drop))?;
    Ok(stale)
}

/// `aoe-harness recover` (`make recover`): stop the containers a host suspend left running.
pub(crate) fn recover() -> Result<(), Box<dyn Error>> {
    let stopped = stop_interrupted()?;
    if stopped.is_empty() {
        println!("recover: no harness container was interrupted by a host suspend");
    }
    for (id, started) in stopped {
        println!("recover: stopped {id} (started at boot gap {started}s; the gap has since moved)");
    }
    Ok(())
}
