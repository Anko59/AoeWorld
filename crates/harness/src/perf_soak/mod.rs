//! Repeated target-scale network lifecycle and retained-memory evidence.
use crate::perf::{self, ClientResult, Verdict};
use aoe_scenario::TARGET_DISTRIBUTED;
use serde::Serialize;
use std::{
    error::Error,
    fs,
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const MAX_RETAINED_RSS_BYTES: u64 = 1_073_741_824;

#[derive(Serialize)]
struct Report {
    version: u16,
    revision: String,
    dirty: bool,
    scenario: &'static str,
    requested_seconds: u64,
    elapsed_ms: u64,
    cycles: u64,
    clients_connected: u64,
    snapshots: u64,
    deltas: u64,
    encoded_bytes: u64,
    retained_rss_start_bytes: u64,
    retained_rss_peak_bytes: u64,
    retained_rss_end_bytes: u64,
    retained_rss_limit_bytes: u64,
    verdict: Verdict,
    failure: Option<String>,
}

fn git(args: &[&str]) -> Result<String> {
    let output = Command::new("git").args(args).output()?;
    if !output.status.success() {
        return Err(format!("git {args:?}: {}", String::from_utf8_lossy(&output.stderr)).into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn parse_rss(status: &str) -> Result<u64> {
    let line = status
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))
        .ok_or("VmRSS is missing from process status")?;
    let kib = line
        .split_whitespace()
        .next()
        .ok_or("VmRSS has no value")?
        .parse::<u64>()?;
    kib.checked_mul(1_024).ok_or("VmRSS overflow".into())
}

fn rss_bytes() -> Result<u64> {
    parse_rss(&fs::read_to_string("/proc/self/status")?)
}

fn run_for<F>(
    duration: Duration,
    name: &str,
    path: &Path,
    rss_limit: u64,
    mut cycle: F,
) -> Result<()>
where
    F: FnMut() -> Result<ClientResult>,
{
    let initial_rss = rss_bytes()?;
    let started = Instant::now();
    let mut report = Report {
        version: 1,
        revision: git(&["rev-parse", "HEAD"])?,
        dirty: !git(&["status", "--porcelain"])?.is_empty(),
        scenario: TARGET_DISTRIBUTED.name,
        requested_seconds: duration.as_secs(),
        elapsed_ms: 0,
        cycles: 0,
        clients_connected: 0,
        snapshots: 0,
        deltas: 0,
        encoded_bytes: 0,
        retained_rss_start_bytes: initial_rss,
        retained_rss_peak_bytes: initial_rss,
        retained_rss_end_bytes: initial_rss,
        retained_rss_limit_bytes: rss_limit,
        verdict: Verdict::Pass,
        failure: None,
    };
    loop {
        match cycle() {
            Ok(sample)
                if sample.snapshots == TARGET_DISTRIBUTED.players
                    && sample.deltas == u64::from(TARGET_DISTRIBUTED.players) * 2 =>
            {
                report.cycles += 1;
                report.clients_connected += u64::from(sample.snapshots);
                report.snapshots += u64::from(sample.snapshots);
                report.deltas += sample.deltas;
                report.encoded_bytes += sample.bytes;
            }
            Ok(sample) => {
                report.verdict = Verdict::Regression;
                report.failure = Some(format!(
                    "incomplete cycle: {} snapshots and {} deltas",
                    sample.snapshots, sample.deltas
                ));
                break;
            }
            Err(error) => {
                report.verdict = Verdict::Inconclusive;
                report.failure = Some(error.to_string());
                break;
            }
        }
        report.retained_rss_end_bytes = rss_bytes()?;
        report.retained_rss_peak_bytes = report
            .retained_rss_peak_bytes
            .max(report.retained_rss_end_bytes);
        if report.retained_rss_end_bytes > rss_limit {
            report.verdict = Verdict::Regression;
            report.failure = Some(format!("retained process RSS exceeded {rss_limit} bytes"));
            break;
        }
        if started.elapsed() >= duration {
            break;
        }
    }
    report.elapsed_ms = started.elapsed().as_millis() as u64;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(&report)?)?;
    fs::write(
        path.with_extension("md"),
        format!(
            "# Synthetic network soak\n\nRevision: `{}`; dirty: `{}`; verdict: `{:?}`.\n\n{} cycles, {} clients, {} snapshots, {} deltas, {} encoded bytes in {} ms. Retained RSS: {} → {} bytes (peak {}, limit {}).\n",
            report.revision,
            report.dirty,
            report.verdict,
            report.cycles,
            report.clients_connected,
            report.snapshots,
            report.deltas,
            report.encoded_bytes,
            report.elapsed_ms,
            report.retained_rss_start_bytes,
            report.retained_rss_end_bytes,
            report.retained_rss_peak_bytes,
            report.retained_rss_limit_bytes
        ),
    )?;
    println!("{name}: {} cycles, {:?}", report.cycles, report.verdict);
    if report.verdict != Verdict::Pass {
        return Err(report
            .failure
            .unwrap_or_else(|| "network soak failed".to_owned())
            .into());
    }
    Ok(())
}

fn run_to(seconds: u64, name: &str, path: &Path) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    run_for(
        Duration::from_secs(seconds),
        name,
        path,
        MAX_RETAINED_RSS_BYTES,
        || runtime.block_on(perf::network(TARGET_DISTRIBUTED)),
    )
}

pub fn run(seconds: u64, name: &str) -> Result<()> {
    run_to(
        seconds,
        name,
        Path::new(&format!("reports/perf/{name}.json")),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rss_parser_rejects_missing_or_overflowing_values() {
        assert_eq!(parse_rss("VmRSS:\t123 kB\n").expect("rss"), 125_952);
        assert!(parse_rss("VmSize:\t123 kB\n").is_err());
        assert!(parse_rss("VmRSS:\t\n").is_err());
        assert!(parse_rss("VmRSS:\t18446744073709551615 kB\n").is_err());
        assert!(git(&["rev-parse", "--verify", "refs/heads/absent-soak-test"]).is_err());
    }

    #[test]
    fn soak_reports_target_connections_and_rejects_incomplete_cycles() {
        let temp = tempfile::tempdir().expect("report directory");
        let path = temp.path().join("soak.json");
        run_to(0, "test", &path).expect("target cycle");
        let value: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).expect("report")).expect("json");
        assert_eq!(value["verdict"], "PASS");
        assert_eq!(value["clients_connected"], 64);
        assert_eq!(value["snapshots"], 64);
        assert_eq!(value["deltas"], 128);
        assert!(path.with_extension("md").is_file());
        assert!(
            run_for(
                Duration::ZERO,
                "test",
                &path,
                MAX_RETAINED_RSS_BYTES,
                || {
                    let mut sample = ClientResult::default();
                    sample.snapshots = 63;
                    Ok(sample)
                }
            )
            .is_err()
        );
        let failed: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).expect("failed report")).expect("json");
        assert_eq!(failed["verdict"], "REGRESSION");
        assert!(
            run_for(
                Duration::ZERO,
                "test",
                &path,
                MAX_RETAINED_RSS_BYTES,
                || Err("connection failed".into())
            )
            .is_err()
        );
        let failed: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).expect("failed report")).expect("json");
        assert_eq!(failed["verdict"], "INCONCLUSIVE");
        assert!(
            run_for(Duration::ZERO, "test", &path, 0, || {
                let mut sample = ClientResult::default();
                sample.snapshots = 64;
                sample.deltas = 128;
                Ok(sample)
            })
            .is_err()
        );
        let failed: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).expect("memory report")).expect("json");
        assert_eq!(failed["verdict"], "REGRESSION");
        assert!(
            failed["failure"]
                .as_str()
                .is_some_and(|value| value.contains("RSS"))
        );
    }
}
