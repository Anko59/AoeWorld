//! CPU and memory of the processes and containers the run starts.
use super::BoxError;
use std::{fs, process::Command};

/// Clock ticks per second of `/proc/<pid>/stat` times (Linux fixes it at 100).
const CLOCK_TICKS: f64 = 100.0;

/// Cumulative CPU seconds and resident MiB of one process.
pub(super) fn process(pid: u32) -> Option<(f64, u64)> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // The command name may contain spaces; fields count from the closing parenthesis.
    let fields: Vec<&str> = stat[stat.rfind(')')? + 2..].split_whitespace().collect();
    let user: f64 = fields.get(11)?.parse().ok()?;
    let system: f64 = fields.get(12)?.parse().ok()?;
    let status = fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let rss_kib: u64 = status
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))?
        .trim()
        .trim_end_matches("kB")
        .trim()
        .parse()
        .ok()?;
    Some(((user + system) / CLOCK_TICKS, rss_kib / 1024))
}

/// What `docker stats` says about one container.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Container {
    /// Cores in use (100% is one core).
    pub cores: f64,
    pub rss_mib: f64,
}

/// One sample of every named container; those not running are skipped.
pub(super) fn containers(names: &[String]) -> Result<Vec<Container>, BoxError> {
    if names.is_empty() {
        return Ok(Vec::new());
    }
    // `docker stats` fails as a whole when a named container has exited.
    let running = Command::new("docker")
        .args(["ps", "--format", "{{.Names}}"])
        .output()?;
    let running = String::from_utf8_lossy(&running.stdout).into_owned();
    let alive: Vec<&String> = names
        .iter()
        .filter(|name| running.lines().any(|line| line == name.as_str()))
        .collect();
    if alive.is_empty() {
        return Ok(Vec::new());
    }
    let output = Command::new("docker")
        .args([
            "stats",
            "--no-stream",
            "--format",
            "{{.CPUPerc}} {{.MemUsage}}",
        ])
        .args(alive)
        .output()?;
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(parse_stats_line)
        .collect())
}

pub(super) fn parse_stats_line(line: &str) -> Option<Container> {
    let mut parts = line.split_whitespace();
    let cores = parts.next()?.trim_end_matches('%').parse::<f64>().ok()? / 100.0;
    let used = parts.next()?;
    let (number, unit) = used.split_at(used.find(|c: char| c.is_alphabetic())?);
    let scale = match unit {
        "B" => 1.0 / (1024.0 * 1024.0),
        "KiB" | "kB" => 1.0 / 1024.0,
        "MiB" | "MB" => 1.0,
        "GiB" | "GB" => 1024.0,
        _ => return None,
    };
    Some(Container {
        cores,
        rss_mib: number.parse::<f64>().ok()? * scale,
    })
}
