//! Read final cgroup usage before Docker removes the workload container.
use std::{error::Error, fs, process::Command};

pub(super) fn container(args: &[String]) -> Result<i32, Box<dyn Error>> {
    let (program, args) = args.split_first().ok_or("missing container command")?;
    let status = Command::new(program).args(args).status()?;
    let peak =
        fs::read_to_string("/sys/fs/cgroup/memory.peak").unwrap_or_else(|_| "unavailable".into());
    let events = fs::read_to_string("/sys/fs/cgroup/memory.events").unwrap_or_default();
    eprintln!("resource: container_peak_bytes={}", peak.trim());
    if events
        .lines()
        .any(|line| line.strip_prefix("oom_kill ").is_some_and(|n| n != "0"))
    {
        eprintln!("resource: container exceeded its memory cap (kernel OOM kill)");
    }
    Ok(status.code().unwrap_or(137))
}
