//! Dedicated-machine timing interface. No baseline exists until hardware is supplied.
use aoe_scenario::TARGET_HOTSPOT;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, error::Error, fs, path::Path, process::Command};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct Environment {
    version: u16,
    cpu_model: String,
    gpu_model: String,
    gpu_driver: String,
    os: String,
    kernel: String,
    browser: String,
    browser_version: String,
    rustc: String,
    container_digests: BTreeMap<String, String>,
    memory_limit_bytes: u64,
    cpu_limit_millicores: u32,
    cpu_governor: String,
    gpu_power_mode: String,
    concurrent_jobs: u16,
    viewport_width: u16,
    viewport_height: u16,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Samples {
    version: u16,
    revision: String,
    workload_hash: String,
    duration_seconds: u32,
    total_entities: u32,
    resident_entities: u32,
    visible_entities: u32,
    tick_ns: Vec<u64>,
    frame_interval_ns: Vec<u64>,
    cpu_submission_ns: Vec<u64>,
    gpu_frame_ns: Option<Vec<u64>>,
    missed_tick_deadlines: u32,
    dropped_frames: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Baseline {
    version: u16,
    environment: Environment,
    workload_hash: String,
    runs: Vec<BaselineRun>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BaselineRun {
    sample_hash: String,
    tick_p99_ns: u64,
    frame_p99_ns: u64,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum Verdict {
    Pass,
    Regression,
    Unbaselined,
}

#[derive(Debug, Serialize)]
struct Report {
    version: u16,
    environment: Environment,
    samples: Samples,
    sample_hash: String,
    tick_p99_ns: u64,
    frame_p99_ns: u64,
    baseline_runs: usize,
    verdict: Verdict,
}

fn valid_sha256(value: &str) -> bool {
    value
        .strip_prefix("sha256:")
        .is_some_and(|digest| digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()))
}

fn valid_blake3(value: &str) -> bool {
    value
        .strip_prefix("blake3:")
        .is_some_and(|digest| digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()))
}

fn validate_environment(value: &Environment) -> Result<()> {
    if value.version != 1
        || [
            &value.cpu_model,
            &value.gpu_model,
            &value.gpu_driver,
            &value.os,
            &value.kernel,
            &value.browser,
            &value.browser_version,
            &value.rustc,
            &value.cpu_governor,
            &value.gpu_power_mode,
        ]
        .iter()
        .any(|text| text.trim().is_empty() || text.trim() == "unknown")
        || value.container_digests.is_empty()
        || value
            .container_digests
            .iter()
            .any(|(name, digest)| name.trim().is_empty() || !valid_sha256(digest))
        || value.memory_limit_bytes == 0
        || value.cpu_limit_millicores == 0
        || value.concurrent_jobs != 1
        || value.viewport_width != 1920
        || value.viewport_height != 1080
    {
        return Err("incomplete or incompatible dedicated-hardware environment".into());
    }
    Ok(())
}

fn p99(values: &[u64]) -> Result<u64> {
    if values.is_empty() || values.contains(&0) {
        return Err("timing samples are missing or zero".into());
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let index = (sorted.len() * 99).div_ceil(100) - 1;
    Ok(sorted[index])
}

fn validate_samples(value: &Samples, revision: &str) -> Result<()> {
    if value.version != 1
        || value.revision != revision
        || value.workload_hash != TARGET_HOTSPOT.workload_hash()
        || value.duration_seconds < 60
        || value.total_entities != TARGET_HOTSPOT.entities
        || value.resident_entities != TARGET_HOTSPOT.entities
        || value.visible_entities < 10_000
        || value.tick_ns.len() < value.duration_seconds as usize * 20
        || value.frame_interval_ns.len() < value.duration_seconds as usize * 60
        || value.cpu_submission_ns.len() != value.frame_interval_ns.len()
        || value
            .gpu_frame_ns
            .as_ref()
            .is_some_and(|times| times.len() != value.frame_interval_ns.len())
    {
        return Err("hardware samples do not prove the target workload and duration".into());
    }
    p99(&value.tick_ns)?;
    p99(&value.frame_interval_ns)?;
    p99(&value.cpu_submission_ns)?;
    if let Some(times) = &value.gpu_frame_ns {
        p99(times)?;
    }
    Ok(())
}

fn stable(values: impl Iterator<Item = u64>) -> Result<u64> {
    let mut sorted: Vec<_> = values.collect();
    if sorted.len() < 3 || sorted.contains(&0) {
        return Err("three valid dedicated-hardware baseline runs are required".into());
    }
    sorted.sort_unstable();
    let median = sorted[sorted.len() / 2];
    if u128::from(*sorted.last().ok_or("empty baseline")?) * 100 > u128::from(sorted[0]) * 105 {
        return Err("dedicated-hardware baseline runs are not stable within 5%".into());
    }
    Ok(median)
}

fn compare(
    environment: &Environment,
    samples: &Samples,
    baseline: Option<&Baseline>,
) -> Result<Report> {
    validate_environment(environment)?;
    validate_samples(samples, &samples.revision)?;
    let ticks = p99(&samples.tick_ns)?;
    let frames = p99(&samples.frame_interval_ns)?;
    let (baseline_runs, references) = if let Some(baseline) = baseline {
        if baseline.version != 1
            || baseline.environment != *environment
            || baseline.workload_hash != samples.workload_hash
        {
            return Err("dedicated-hardware baseline environment or workload differs".into());
        }
        let mut seen = std::collections::BTreeSet::new();
        if baseline
            .runs
            .iter()
            .any(|run| !valid_blake3(&run.sample_hash) || !seen.insert(&run.sample_hash))
        {
            return Err("duplicate or invalid baseline sample hash".into());
        }
        let tick = stable(baseline.runs.iter().map(|run| run.tick_p99_ns))?;
        let frame = stable(baseline.runs.iter().map(|run| run.frame_p99_ns))?;
        (baseline.runs.len(), Some((tick, frame)))
    } else {
        (0, None)
    };
    let targets_pass = ticks <= 50_000_000
        && frames <= 16_666_667
        && samples.missed_tick_deadlines == 0
        && samples.dropped_frames == 0;
    let verdict = match references {
        None => Verdict::Unbaselined,
        Some((tick, frame))
            if targets_pass
                && u128::from(ticks) * 100 <= u128::from(tick) * 105
                && u128::from(frames) * 100 <= u128::from(frame) * 105 =>
        {
            Verdict::Pass
        }
        Some(_) => Verdict::Regression,
    };
    Ok(Report {
        version: 1,
        environment: environment.clone(),
        samples: samples.clone(),
        sample_hash: format!(
            "blake3:{}",
            blake3::hash(&serde_json::to_vec(samples)?).to_hex()
        ),
        tick_p99_ns: ticks,
        frame_p99_ns: frames,
        baseline_runs,
        verdict,
    })
}

fn git(args: &[&str]) -> Result<String> {
    let output = Command::new("git").args(args).output()?;
    if !output.status.success() {
        return Err("git identity query failed".into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

pub fn check(environment: &Path, samples: &Path, baseline: &Path) -> Result<()> {
    if !git(&["status", "--porcelain"])?.is_empty() {
        return Err("hardware qualification requires a clean source tree".into());
    }
    let environment: Environment = serde_json::from_slice(&fs::read(environment)?)?;
    let samples: Samples = serde_json::from_slice(&fs::read(samples)?)?;
    let revision = git(&["rev-parse", "HEAD"])?;
    validate_samples(&samples, &revision)?;
    let baseline = if baseline.is_file() {
        Some(serde_json::from_slice::<Baseline>(&fs::read(baseline)?)?)
    } else {
        None
    };
    let report = compare(&environment, &samples, baseline.as_ref())?;
    fs::create_dir_all("reports/perf")?;
    fs::write(
        "reports/perf/hardware.json",
        serde_json::to_vec_pretty(&report)?,
    )?;
    fs::write(
        "reports/perf/hardware.md",
        format!(
            "# Dedicated-hardware qualification\n\nRevision: `{revision}`\n\nTick p99: {} ns; frame p99: {} ns; baseline runs: {}.\n\nVerdict: `{:?}`.\n",
            report.tick_p99_ns, report.frame_p99_ns, report.baseline_runs, report.verdict
        ),
    )?;
    println!("hardware qualification: {:?}", report.verdict);
    if report.verdict == Verdict::Pass {
        Ok(())
    } else {
        Err("dedicated-hardware qualification did not pass".into())
    }
}

#[cfg(test)]
mod tests;
