//! Synthetic performance evidence. Timing is informational on shared hosts.
use aoe_core::Region;
use aoe_protocol::{ClientMessage, ServerMessage, VERSION, decode_server, encode_client};
use aoe_scenario::{
    POPULATION_8K, POPULATION_32K, POPULATION_64K, POPULATION_128K, SMOKE, SPARSE_LARGE,
    SPARSE_SMALL, Scenario, TARGET_DISTRIBUTED, TARGET_HOTSPOT,
};
use aoe_server::{AppState, Config, app};
use aoe_simulation::World;
use futures_util::{SinkExt, StreamExt, future::join_all};
use serde::Serialize;
use std::{
    error::Error,
    fs,
    net::SocketAddr,
    process::Command,
    time::{Duration, Instant},
};
use tokio::{net::TcpListener, time::timeout};
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Verdict {
    Pass,
    Regression,
    Unbaselined,
    Inconclusive,
}

#[derive(Debug, Serialize)]
pub struct Comparison {
    pub metric: String,
    pub observed: Option<u64>,
    pub baseline: Option<u64>,
    pub threshold_percent: u8,
    pub verdict: Verdict,
}

pub fn compare(metric: &str, observed: Option<u64>, baseline: Option<u64>) -> Comparison {
    let verdict = match (observed, baseline) {
        (None, _) => Verdict::Inconclusive,
        (Some(_), None) => Verdict::Unbaselined,
        (Some(actual), Some(reference))
            if u128::from(actual) * 100 > u128::from(reference) * 105 =>
        {
            Verdict::Regression
        }
        _ => Verdict::Pass,
    };
    Comparison {
        metric: metric.to_owned(),
        observed,
        baseline,
        threshold_percent: 5,
        verdict,
    }
}

#[derive(Debug, Serialize)]
struct WorkloadResult {
    scenario: String,
    seed: u64,
    workload_hash: String,
    total_entities: u32,
    active_entities: u32,
    resident_entities: u32,
    replicated_entities: u64,
    max_visible_entities: usize,
    loaded_chunks: usize,
    query_visited_chunks: u32,
    query_candidates: u32,
    clients_expected: u16,
    clients_connected: u16,
    snapshots: u16,
    deltas: u64,
    encoded_bytes: u64,
    tick_duration_ns: Vec<u64>,
    elapsed_ms: u64,
    verdict: Verdict,
}

#[derive(Debug, Serialize)]
pub struct Report {
    version: u16,
    revision: String,
    dirty: bool,
    environment: Environment,
    workloads: Vec<WorkloadResult>,
    comparisons: Vec<Comparison>,
    notes: Vec<String>,
    hardware_qualification: &'static str,
    verdict: Verdict,
}

#[derive(Debug, Serialize)]
struct Environment {
    os: &'static str,
    architecture: &'static str,
    rustc: String,
    dedicated_hardware: bool,
}

fn git(args: &[&str]) -> String {
    Command::new("git")
        .args(args)
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
        .unwrap_or_else(|| "unknown".to_owned())
}

fn rustc_version() -> String {
    Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
        .unwrap_or_else(|| "unknown".to_owned())
}

pub(crate) fn region(scenario: Scenario, client: u16) -> Region {
    if scenario.hotspot_entities > 0 && client == 0 {
        return Region {
            x: 0,
            y: 0,
            width: 128,
            height: 128,
        };
    }
    let span = scenario.active_extent - 256;
    Region {
        x: (i32::from(client) * 157) % span,
        y: (i32::from(client) * 263) % span,
        width: 256,
        height: 256,
    }
}

struct OfflineResult {
    world: World,
    tick_duration_ns: Vec<u64>,
    visited_chunks: u32,
    candidates: u32,
    visible: usize,
}

fn offline(scenario: Scenario) -> Result<OfflineResult, Box<dyn Error>> {
    let mut world = World::new(scenario);
    if world.entities().len() != scenario.entities as usize {
        return Err("population mismatch".into());
    }
    let mut tick_duration_ns = Vec::new();
    for _ in 0..4 {
        let start = Instant::now();
        world.advance();
        tick_duration_ns.push(start.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64);
    }
    let (visible, stats) = world.query_with_stats(region(scenario, 0));
    if scenario.hotspot_entities > 0 && visible.len() < scenario.hotspot_entities as usize {
        return Err("hotspot visibility fell below workload definition".into());
    }
    Ok(OfflineResult {
        world,
        tick_duration_ns,
        visited_chunks: stats.visited_chunks,
        candidates: stats.candidate_entities,
        visible: visible.len(),
    })
}

pub(crate) async fn receive(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> Result<(ServerMessage, usize), Box<dyn Error + Send + Sync>> {
    let frame = timeout(Duration::from_secs(20), socket.next())
        .await?
        .ok_or("connection closed")??;
    let Message::Binary(bytes) = frame else {
        return Err("nonbinary protocol frame".into());
    };
    Ok((decode_server(&bytes)?, bytes.len()))
}

pub(crate) async fn send(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    message: ClientMessage,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    socket
        .send(Message::Binary(encode_client(&message)?.into()))
        .await?;
    Ok(())
}

#[derive(Default)]
pub(crate) struct ClientResult {
    pub(crate) snapshots: u16,
    pub(crate) deltas: u64,
    replicated: u64,
    pub(crate) bytes: u64,
    visible: usize,
}

fn initial_client_result(handshake_bytes: usize) -> ClientResult {
    ClientResult {
        bytes: handshake_bytes as u64,
        ..Default::default()
    }
}

async fn client(
    address: SocketAddr,
    scenario: Scenario,
    index: u16,
) -> Result<ClientResult, Box<dyn Error + Send + Sync>> {
    let (mut socket, _) = timeout(
        Duration::from_secs(20),
        connect_async(format!("ws://{address}/ws")),
    )
    .await??;
    send(&mut socket, ClientMessage::Hello { version: VERSION }).await?;
    let (hello, size) = receive(&mut socket).await?;
    if !matches!(
        hello,
        ServerMessage::Hello {
            version: VERSION,
            ..
        }
    ) {
        return Err("handshake mismatch".into());
    }
    let mut result = initial_client_result(size);
    send(
        &mut socket,
        ClientMessage::Subscribe {
            region: region(scenario, index),
        },
    )
    .await?;
    let (snapshot, size) = receive(&mut socket).await?;
    let ServerMessage::Snapshot {
        total_entities,
        entities,
        ..
    } = snapshot
    else {
        return Err("snapshot missing".into());
    };
    if total_entities != scenario.entities {
        return Err("server population mismatch".into());
    }
    result.snapshots = 1;
    result.replicated = entities.len() as u64;
    result.visible = entities.len();
    result.bytes += size as u64;
    for _ in 0..2 {
        let (message, size) = receive(&mut socket).await?;
        if !matches!(message, ServerMessage::Delta { .. }) {
            return Err("delta missing".into());
        }
        result.deltas += 1;
        result.bytes += size as u64;
    }
    Ok(result)
}

pub(crate) async fn network(scenario: Scenario) -> Result<ClientResult, Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let config = Config {
        bind: address,
        scenario,
        tick_hz: 20,
    };
    let state = AppState::new(&config, "perf-local");
    let ticker = tokio::spawn(state.clone().run_ticks());
    let server = tokio::spawn(async move { axum::serve(listener, app(state)).await });
    let clients =
        join_all((0..scenario.players).map(|index| client(address, scenario, index))).await;
    ticker.abort();
    server.abort();
    let mut combined = ClientResult::default();
    for item in clients {
        let item = item.map_err(|error| error.to_string())?;
        combined.snapshots += item.snapshots;
        combined.deltas += item.deltas;
        combined.replicated += item.replicated;
        combined.bytes += item.bytes;
        combined.visible = combined.visible.max(item.visible);
    }
    Ok(combined)
}

pub fn run(mode: &str) -> Result<(), Box<dyn Error>> {
    let scenarios: &[Scenario] = match mode {
        "smoke" => &[SMOKE],
        "ci" => &[SMOKE, TARGET_DISTRIBUTED, TARGET_HOTSPOT],
        "full" => &[
            SMOKE,
            TARGET_DISTRIBUTED,
            TARGET_HOTSPOT,
            POPULATION_8K,
            POPULATION_32K,
            POPULATION_64K,
            POPULATION_128K,
            SPARSE_SMALL,
            SPARSE_LARGE,
        ],
        _ => return Err("invalid performance mode".into()),
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let mut workloads = Vec::new();
    let mut comparisons = Vec::new();
    let mut notes = Vec::new();
    for scenario in scenarios {
        let start = Instant::now();
        let offline = offline(*scenario)?;
        let connection = runtime.block_on(network(*scenario))?;
        if connection.snapshots != scenario.players
            || connection.deltas != u64::from(scenario.players) * 2
        {
            return Err(format!("{} did not exercise all clients", scenario.name).into());
        }
        if scenario.hotspot_entities > 0 && connection.visible < scenario.hotspot_entities as usize
        {
            return Err("network hotspot was not observed".into());
        }
        let result = WorkloadResult {
            scenario: scenario.name.to_owned(),
            seed: scenario.seed.0,
            workload_hash: scenario.workload_hash(),
            total_entities: scenario.entities,
            active_entities: scenario.entities,
            resident_entities: offline.world.entities().len() as u32,
            replicated_entities: connection.replicated,
            max_visible_entities: offline.visible.max(connection.visible),
            loaded_chunks: offline.world.loaded_chunks(),
            query_visited_chunks: offline.visited_chunks,
            query_candidates: offline.candidates,
            clients_expected: scenario.players,
            clients_connected: connection.snapshots,
            snapshots: connection.snapshots,
            deltas: connection.deltas,
            encoded_bytes: connection.bytes,
            tick_duration_ns: offline.tick_duration_ns,
            elapsed_ms: start.elapsed().as_millis() as u64,
            verdict: Verdict::Pass,
        };
        println!(
            "{}: {} clients, {} entities, {} max visible, {} bytes",
            result.scenario,
            result.clients_connected,
            result.total_entities,
            result.max_visible_entities,
            result.encoded_bytes
        );
        workloads.push(result);
    }
    if mode == "full" {
        let small = workloads
            .iter()
            .find(|result| result.scenario == "sparse-small")
            .ok_or("sparse-small workload missing")?;
        let large = workloads
            .iter()
            .find(|result| result.scenario == "sparse-large")
            .ok_or("sparse-large workload missing")?;
        if small.loaded_chunks != large.loaded_chunks
            || small.query_visited_chunks != large.query_visited_chunks
            || small.query_candidates != large.query_candidates
        {
            return Err("empty-map scaling assertion failed".into());
        }
    }
    if mode != "smoke" {
        match crate::perf_micro::comparisons() {
            Ok(results) => comparisons.extend(results),
            Err(error) => {
                notes.push(format!("instruction measurements unavailable: {error}"));
                comparisons.push(compare("instructions", None, Some(1)));
            }
        }
        match crate::perf_size::comparison() {
            Ok(result) => comparisons.push(result),
            Err(error) => {
                notes.push(format!("optimized WASM measurement unavailable: {error}"));
                comparisons.push(compare("optimized_gzip_wasm_bytes", None, Some(1)));
            }
        }
    }
    let verdict = if comparisons
        .iter()
        .any(|item| item.verdict == Verdict::Regression)
    {
        Verdict::Regression
    } else if comparisons
        .iter()
        .any(|item| item.verdict == Verdict::Inconclusive)
    {
        Verdict::Inconclusive
    } else if comparisons
        .iter()
        .any(|item| item.verdict == Verdict::Unbaselined)
    {
        Verdict::Unbaselined
    } else {
        Verdict::Pass
    };
    let report = Report {
        version: 1,
        revision: git(&["rev-parse", "HEAD"]),
        dirty: !git(&["status", "--porcelain"]).is_empty(),
        environment: Environment {
            os: std::env::consts::OS,
            architecture: std::env::consts::ARCH,
            rustc: rustc_version(),
            dedicated_hardware: false,
        },
        workloads,
        comparisons,
        notes,
        hardware_qualification: "NOT_ESTABLISHED",
        verdict,
    };
    fs::create_dir_all("reports/perf")?;
    let json = serde_json::to_vec_pretty(&report)?;
    fs::write(format!("reports/perf/{mode}.json"), json)?;
    fs::write(format!("reports/perf/{mode}.md"), summary(&report))?;
    println!("report: reports/perf/{mode}.json ({:?})", report.verdict);
    if mode != "smoke" && verdict != Verdict::Pass {
        return Err(format!("required performance comparisons are {verdict:?}").into());
    }
    Ok(())
}

fn summary(report: &Report) -> String {
    let mut text = format!(
        "# Synthetic performance report\n\nRevision: `{}`; dirty: `{}`; verdict: `{:?}`.\n\n| Scenario | Entities | Clients | Max visible | Chunks | Bytes |\n|---|---:|---:|---:|---:|---:|\n",
        report.revision, report.dirty, report.verdict
    );
    for item in &report.workloads {
        text.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            item.scenario,
            item.total_entities,
            item.clients_connected,
            item.max_visible_entities,
            item.loaded_chunks,
            item.encoded_bytes
        ));
    }
    text.push_str("\nHardware qualification: not established. Tick timing is informational on shared hardware.\n");
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparator_rejects_missing_and_regressed_samples() {
        assert_eq!(initial_client_result(37).bytes, 37);
        assert_eq!(compare("x", None, Some(100)).verdict, Verdict::Inconclusive);
        assert_eq!(compare("x", Some(100), None).verdict, Verdict::Unbaselined);
        assert_eq!(compare("x", Some(105), Some(100)).verdict, Verdict::Pass);
        assert_eq!(compare("x", Some(0), Some(0)).verdict, Verdict::Pass);
        assert_eq!(compare("x", Some(1), Some(0)).verdict, Verdict::Regression);
        assert_eq!(
            compare("x", Some(106), Some(100)).verdict,
            Verdict::Regression
        );
        assert_eq!(
            compare("x", Some(u64::MAX), Some(1)).verdict,
            Verdict::Regression
        );
    }
}
