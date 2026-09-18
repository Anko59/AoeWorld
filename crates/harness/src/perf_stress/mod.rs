//! Beyond-target characterization with an explicit protocol overload outcome.
use crate::perf::{self, Verdict};
use aoe_protocol::{ClientMessage, ServerMessage, VERSION};
use aoe_scenario::BEYOND_TARGET;
use aoe_server::{AppState, Config, app};
use aoe_simulation::World;
use futures_util::future::join_all;
use serde::Serialize;
use std::{
    error::Error,
    fs,
    net::SocketAddr,
    path::Path,
    process::Command,
    time::{Duration, Instant},
};
use tokio::{net::TcpListener, time::timeout};
use tokio_tungstenite::connect_async;

type Result<T> = std::result::Result<T, Box<dyn Error>>;
type ClientResult<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Default)]
struct Sample {
    handshakes: u16,
    snapshots: u16,
    deltas: u64,
    overload_errors: u16,
    replicated_entities: u64,
    encoded_bytes: u64,
    max_visible_entities: usize,
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
    snapshots: u16,
    deltas: u64,
    overload_errors: u16,
    replicated_entities: u64,
    encoded_bytes: u64,
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

async fn client(address: SocketAddr, index: u16) -> ClientResult<Sample> {
    let (mut socket, _) = timeout(
        Duration::from_secs(20),
        connect_async(format!("ws://{address}/ws")),
    )
    .await??;
    perf::send(&mut socket, ClientMessage::Hello { version: VERSION }).await?;
    let (hello, size) = perf::receive(&mut socket).await?;
    if !matches!(
        hello,
        ServerMessage::Hello {
            version: VERSION,
            ..
        }
    ) {
        return Err("beyond-target handshake mismatch".into());
    }
    let mut sample = Sample {
        handshakes: 1,
        encoded_bytes: size as u64,
        ..Default::default()
    };
    perf::send(
        &mut socket,
        ClientMessage::Subscribe {
            region: perf::region(BEYOND_TARGET, index),
        },
    )
    .await?;
    let (response, size) = perf::receive(&mut socket).await?;
    sample.encoded_bytes += size as u64;
    if index == 0 {
        if !matches!(response, ServerMessage::Error { code: 413, .. }) {
            return Err("hotspot did not report protocol overload".into());
        }
        sample.overload_errors = 1;
        return Ok(sample);
    }
    let ServerMessage::Snapshot {
        total_entities,
        entities,
        ..
    } = response
    else {
        return Err("beyond-target snapshot missing".into());
    };
    if total_entities != BEYOND_TARGET.entities {
        return Err("beyond-target server population mismatch".into());
    }
    sample.snapshots = 1;
    sample.replicated_entities = entities.len() as u64;
    sample.max_visible_entities = entities.len();
    for _ in 0..2 {
        let (response, size) = perf::receive(&mut socket).await?;
        if !matches!(response, ServerMessage::Delta { .. }) {
            return Err("beyond-target delta missing".into());
        }
        sample.deltas += 1;
        sample.encoded_bytes += size as u64;
    }
    Ok(sample)
}

async fn network() -> Result<Sample> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let config = Config {
        bind: address,
        scenario: BEYOND_TARGET,
        tick_hz: 20,
        asset_pack: None,
        map_package_directory: None,
    };
    let state = AppState::new(&config, "perf-stress-local")?;
    let ticker = tokio::spawn(state.clone().run_ticks());
    let server = tokio::spawn(async move { axum::serve(listener, app(state)).await });
    let clients = join_all((0..BEYOND_TARGET.players).map(|index| client(address, index))).await;
    ticker.abort();
    server.abort();
    let mut combined = Sample::default();
    for client in clients {
        let value = client.map_err(|error| error.to_string())?;
        combined.handshakes += value.handshakes;
        combined.snapshots += value.snapshots;
        combined.deltas += value.deltas;
        combined.overload_errors += value.overload_errors;
        combined.replicated_entities += value.replicated_entities;
        combined.encoded_bytes += value.encoded_bytes;
        combined.max_visible_entities = combined
            .max_visible_entities
            .max(value.max_visible_entities);
    }
    Ok(combined)
}

fn assess(report: &mut Report, sample: Sample) {
    report.clients_connected = sample.handshakes;
    report.snapshots = sample.snapshots;
    report.deltas = sample.deltas;
    report.overload_errors = sample.overload_errors;
    report.replicated_entities = sample.replicated_entities;
    report.encoded_bytes = sample.encoded_bytes;
    if report.resident_entities != BEYOND_TARGET.entities as usize
        || report.hotspot_visible_entities < BEYOND_TARGET.hotspot_entities as usize
        || sample.handshakes != BEYOND_TARGET.players
        || sample.snapshots != BEYOND_TARGET.players - 1
        || sample.deltas != u64::from(BEYOND_TARGET.players - 1) * 2
        || sample.overload_errors != 1
        || sample.replicated_entities == 0
        || sample.encoded_bytes == 0
    {
        report.verdict = Verdict::Regression;
        report.failure =
            Some("beyond-target population, participation, or overload mismatch".into());
    }
}

fn run_to<F>(path: &Path, load: F) -> Result<()>
where
    F: FnOnce() -> Result<Sample>,
{
    let started = Instant::now();
    let world = World::new(BEYOND_TARGET);
    let hotspot_visible = world.query(perf::region(BEYOND_TARGET, 0)).len();
    let mut report = Report {
        version: 1,
        revision: git(&["rev-parse", "HEAD"])?,
        dirty: !git(&["status", "--porcelain"])?.is_empty(),
        scenario: BEYOND_TARGET.name,
        workload_hash: BEYOND_TARGET.workload_hash(),
        seed: BEYOND_TARGET.seed.0,
        total_entities: BEYOND_TARGET.entities,
        active_entities: BEYOND_TARGET.entities,
        resident_entities: world.entities().len(),
        hotspot_visible_entities: hotspot_visible,
        loaded_chunks: world.loaded_chunks(),
        clients_expected: BEYOND_TARGET.players,
        clients_connected: 0,
        snapshots: 0,
        deltas: 0,
        overload_errors: 0,
        replicated_entities: 0,
        encoded_bytes: 0,
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
            "# Beyond-target synthetic stress\n\nRevision: `{}`; dirty: `{}`; verdict: `{:?}`.\n\n{} resident entities, {} hotspot visible, {} of {} clients connected, {} snapshots, {} deltas, {} explicit overload errors, {} encoded bytes in {} ms.\n\nAn expected 413 response is bounded overload behavior, not evidence that the beyond-target scene renders or simulates within a timing budget.\n",
            report.revision,
            report.dirty,
            report.verdict,
            report.resident_entities,
            report.hotspot_visible_entities,
            report.clients_connected,
            report.clients_expected,
            report.snapshots,
            report.deltas,
            report.overload_errors,
            report.encoded_bytes,
            report.elapsed_ms
        ),
    )?;
    println!("beyond-target stress: {:?}", report.verdict);
    if report.verdict != Verdict::Pass {
        return Err(report
            .failure
            .unwrap_or_else(|| "stress failed".into())
            .into());
    }
    Ok(())
}

pub fn run() -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    run_to(Path::new("reports/perf/stress.json"), || {
        runtime.block_on(network())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beyond_target_stress_requires_exact_participation_and_reports_failures() {
        let temp = tempfile::tempdir().expect("report directory");
        let path = temp.path().join("stress.json");
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("runtime");
        run_to(&path, || runtime.block_on(network())).expect("real stress");
        let pass: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).expect("report")).expect("JSON");
        assert_eq!(pass["verdict"], "PASS");
        assert_eq!(pass["total_entities"], 256_000);
        assert_eq!(pass["clients_connected"], 128);
        assert_eq!(pass["snapshots"], 127);
        assert_eq!(pass["deltas"], 254);
        assert_eq!(pass["overload_errors"], 1);
        assert!(path.with_extension("md").is_file());
        assert!(run_to(&path, || Ok(Sample::default())).is_err());
        let regression: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).expect("report")).expect("JSON");
        assert_eq!(regression["verdict"], "REGRESSION");
        assert!(run_to(&path, || Err("injected network failure".into())).is_err());
        let inconclusive: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).expect("report")).expect("JSON");
        assert_eq!(inconclusive["verdict"], "INCONCLUSIVE");
    }
}
