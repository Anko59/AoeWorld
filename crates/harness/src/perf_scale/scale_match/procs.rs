//! CPU and memory of the processes and containers the run starts.
use super::{BoxError, Options};
use std::{fs, process::Command};

/// `cargo build` arguments for the release server; `--profile` adds the
/// sampling profiler and line tables (symbols of inlined frames).
pub(super) fn server_build_args(options: &Options) -> Vec<&'static str> {
    let mut args = vec!["build", "--release", "--locked", "-p", "aoe-server"];
    if options.profile.is_some() {
        args.extend([
            "--features",
            "sampling-profiler",
            "--config",
            "profile.release.debug=\"line-tables-only\"",
        ]);
    }
    args
}

/// The server's sampling-profiler environment for `--profile`.
pub(super) fn profile_env(options: &Options, command: &mut Command) -> Result<(), BoxError> {
    if let Some(directory) = &options.profile {
        // The window counts from server start, before the measured minutes; a
        // window ending after the run would exit "successfully" with no profile.
        let window = options.profile_after + options.profile_seconds;
        if window as f64 > options.minutes * 60.0 {
            return Err(format!(
                "--profile-after {} + --profile-seconds {} = {window} s ends after the {} s run; \
                 shorten the window or raise --minutes",
                options.profile_after,
                options.profile_seconds,
                options.minutes * 60.0
            )
            .into());
        }
        fs::create_dir_all(directory)?;
        command
            .env("AOE_SAMPLING_PROFILE", fs::canonicalize(directory)?)
            .env(
                "AOE_SAMPLING_PROFILE_AFTER",
                options.profile_after.to_string(),
            )
            .env(
                "AOE_SAMPLING_PROFILE_SECONDS",
                options.profile_seconds.to_string(),
            );
    }
    Ok(())
}

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
