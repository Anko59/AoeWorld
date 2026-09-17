//! Clock-scheduled protocol work, camera churn, slow reads, and reconnects.
mod network;
use crate::perf::Verdict;
use aoe_scenario::NETWORK_PRESSURE;
use aoe_simulation::World;
use serde::Serialize;
use std::{error::Error, fs, path::Path, process::Command, time::Instant};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
type ClientResult<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;
const OFFERS_PER_CLIENT: u16 = 10;
const OFFER_PERIOD_MS: u64 = 100;
const LATE_TOLERANCE_US: u64 = 20_000;
const RECONNECT_CYCLES: u16 = 4;
const SLOW_READER_PAUSE_MS: u64 = 500;

#[derive(Default)]
struct Sample {
    clients_connected: u16,
    offered_subscriptions: u64,
    sent_subscriptions: u64,
    snapshots: u64,
    deltas: u64,
    missed_offer_deadlines: u64,
    max_offer_lateness_us: u64,
    max_response_backlog: u64,
    replicated_entities: u64,
    max_visible_entities: usize,
    encoded_bytes: u64,
    snapshot_latency_us: Vec<u64>,
    reconnects: u16,
    slow_reader_snapshots: u16,
    slow_reader_deltas: u16,
    tick_deadline_misses: u64,
}

#[derive(Serialize)]
struct Report {
    version: u16,
    revision: String,
    dirty: bool,
    scenario: &'static str,
    workload_hash: String,
    seed: u64,
    total_entities: u32,
    active_entities: u32,
    resident_entities: usize,
    hotspot_visible_entities: usize,
    loaded_chunks: usize,
    clients_expected: u16,
    clients_connected: u16,
    offered_subscriptions: u64,
    sent_subscriptions: u64,
    snapshots: u64,
    deltas: u64,
    missed_offer_deadlines: u64,
    max_offer_lateness_us: u64,
    max_response_backlog: u64,
    replicated_entities: u64,
    max_visible_entities: usize,
    encoded_bytes: u64,
    snapshot_latency_us: Vec<u64>,
    reconnects: u16,
    slow_reader_pause_ms: u64,
    slow_reader_snapshots: u16,
    slow_reader_deltas: u16,
    tick_deadline_misses: u64,
    elapsed_ms: u64,
    verdict: Verdict,
    failure: Option<String>,
}

fn git(args: &[&str]) -> Result<String> {
    let output = Command::new("git").args(args).output()?;
    if !output.status.success() {
        return Err(format!("git {args:?} failed").into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn assess(report: &mut Report, sample: Sample) {
    report.clients_connected = sample.clients_connected;
    report.offered_subscriptions = sample.offered_subscriptions;
    report.sent_subscriptions = sample.sent_subscriptions;
    report.snapshots = sample.snapshots;
    report.deltas = sample.deltas;
    report.missed_offer_deadlines = sample.missed_offer_deadlines;
    report.max_offer_lateness_us = sample.max_offer_lateness_us;
    report.max_response_backlog = sample.max_response_backlog;
    report.replicated_entities = sample.replicated_entities;
    report.max_visible_entities = sample.max_visible_entities;
    report.encoded_bytes = sample.encoded_bytes;
    report.snapshot_latency_us = sample.snapshot_latency_us;
    report.reconnects = sample.reconnects;
    report.slow_reader_snapshots = sample.slow_reader_snapshots;
    report.slow_reader_deltas = sample.slow_reader_deltas;
    report.tick_deadline_misses = sample.tick_deadline_misses;
    let expected_offers = u64::from(NETWORK_PRESSURE.players) * u64::from(OFFERS_PER_CLIENT);
    if report.resident_entities != NETWORK_PRESSURE.entities as usize
        || report.hotspot_visible_entities < NETWORK_PRESSURE.hotspot_entities as usize
        || sample.clients_connected != NETWORK_PRESSURE.players
        || sample.offered_subscriptions != expected_offers
        || sample.sent_subscriptions != expected_offers
        || sample.snapshots != expected_offers
        || report.snapshot_latency_us.len() != expected_offers as usize
        || sample.reconnects != RECONNECT_CYCLES
        || sample.slow_reader_snapshots != 1
        || sample.slow_reader_deltas != 1
        || sample.encoded_bytes == 0
    {
        report.verdict = Verdict::Regression;
        report.failure = Some("offered work, responses, or lifecycle counts differ".into());
    }
}

fn run_to<F>(path: &Path, load: F) -> Result<()>
where
    F: FnOnce() -> Result<Sample>,
{
    let started = Instant::now();
    let world = World::new(NETWORK_PRESSURE);
    let mut report = Report {
        version: 1,
        revision: git(&["rev-parse", "HEAD"])?,
        dirty: !git(&["status", "--porcelain"])?.is_empty(),
        scenario: NETWORK_PRESSURE.name,
        workload_hash: NETWORK_PRESSURE.workload_hash(),
        seed: NETWORK_PRESSURE.seed.0,
        total_entities: NETWORK_PRESSURE.entities,
        active_entities: NETWORK_PRESSURE.entities,
        resident_entities: world.entities().len(),
        hotspot_visible_entities: world.query(network::camera_region(0, 0)).len(),
        loaded_chunks: world.loaded_chunks(),
        clients_expected: NETWORK_PRESSURE.players,
        clients_connected: 0,
        offered_subscriptions: 0,
        sent_subscriptions: 0,
        snapshots: 0,
        deltas: 0,
        missed_offer_deadlines: 0,
        max_offer_lateness_us: 0,
        max_response_backlog: 0,
        replicated_entities: 0,
        max_visible_entities: 0,
        encoded_bytes: 0,
        snapshot_latency_us: Vec::new(),
        reconnects: 0,
        slow_reader_pause_ms: SLOW_READER_PAUSE_MS,
        slow_reader_snapshots: 0,
        slow_reader_deltas: 0,
        tick_deadline_misses: 0,
        elapsed_ms: 0,
        verdict: Verdict::Pass,
        failure: None,
    };
    match load() {
        Ok(sample) => assess(&mut report, sample),
        Err(error) => {
            report.verdict = Verdict::Inconclusive;
            report.failure = Some(error.to_string());
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
            "# Scheduled network pressure\n\nRevision: `{}`; dirty: `{}`; verdict: `{:?}`.\n\n{} entities, {} clients, {} scheduled offers, {} snapshots, {} deltas, {} reconnects, and a {} ms slow reader. Missed offer deadlines: {}; maximum response backlog per client: {}; tick deadline misses: {}. Timings are informational on shared hardware.\n",
            report.revision,
            report.dirty,
            report.verdict,
            report.resident_entities,
            report.clients_connected,
            report.offered_subscriptions,
            report.snapshots,
            report.deltas,
            report.reconnects,
            report.slow_reader_pause_ms,
            report.missed_offer_deadlines,
            report.max_response_backlog,
            report.tick_deadline_misses
        ),
    )?;
    println!("network pressure: {:?}", report.verdict);
    if report.verdict != Verdict::Pass {
        return Err(report
            .failure
            .unwrap_or_else(|| "pressure failed".into())
            .into());
    }
    Ok(())
}

pub fn run() -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    run_to(Path::new("reports/perf/pressure.json"), || {
        runtime.block_on(network::network())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheduled_pressure_keeps_offered_work_and_failure_evidence() {
        let temp = tempfile::tempdir().expect("report directory");
        let path = temp.path().join("pressure.json");
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("runtime");
        run_to(&path, || runtime.block_on(network::network())).expect("real network pressure");
        let pass: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).expect("report")).expect("JSON");
        assert_eq!(pass["verdict"], "PASS");
        assert_eq!(pass["offered_subscriptions"], 640);
        assert_eq!(pass["sent_subscriptions"], 640);
        assert_eq!(pass["snapshots"], 640);
        assert_eq!(
            pass["snapshot_latency_us"]
                .as_array()
                .expect("samples")
                .len(),
            640
        );
        assert_eq!(pass["reconnects"], 4);
        assert!(path.with_extension("md").is_file());
        assert!(run_to(&path, || Ok(Sample::default())).is_err());
        let regression: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).expect("report")).expect("JSON");
        assert_eq!(regression["verdict"], "REGRESSION");
        assert!(run_to(&path, || Err("injected disconnect".into())).is_err());
        let inconclusive: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).expect("report")).expect("JSON");
        assert_eq!(inconclusive["verdict"], "INCONCLUSIVE");
    }
}
