//! Explicit candidate-only source capture; never substitutes the fixed visual matrix.
use super::{BrowserContainer, Server, process, ready};
use aoe_core::{EntityId, TileCoord, WorldPosition};
use aoe_protocol::{
    GAMEPLAY_VERSION, GameplayClientMessage, GameplayRole, GameplayServerMessage,
    decode_gameplay_client, decode_gameplay_server,
};
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Deserialize, Serialize)]
struct WireCapture {
    content_hash: String,
    destination_tile: [i32; 2],
    frame_count: usize,
    payload_bytes: usize,
    frames: Vec<WireFrame>,
}

#[derive(Deserialize, Serialize)]
struct WireFrame {
    direction: String,
    payload_hex: String,
}

fn decode_hex(hex: &str) -> Result<Vec<u8>> {
    if !hex.len().is_multiple_of(2) || hex.len() > aoe_protocol::GAMEPLAY_MAX_MESSAGE * 2 {
        return Err("invalid or oversized captured gameplay frame".into());
    }
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    for pair in hex.as_bytes().chunks_exact(2) {
        let digit = |byte: u8| (byte as char).to_digit(16).map(|value| value as u8);
        let high = digit(pair[0]).ok_or("invalid captured frame hex")?;
        let low = digit(pair[1]).ok_or("invalid captured frame hex")?;
        bytes.push((high << 4) | low);
    }
    Ok(bytes)
}

fn validate_live_wire(evidence: &Path, expected_hash: &str) -> Result<serde_json::Value> {
    let capture_path = evidence.join("live-wire.json");
    if fs::metadata(&capture_path)?.len() > 32 * 1_048_576 {
        return Err("live gameplay evidence file exceeds its bounded JSON limit".into());
    }
    let capture: WireCapture = serde_json::from_slice(&fs::read(capture_path)?)?;
    if capture.content_hash != expected_hash
        || capture.frame_count != capture.frames.len()
        || capture.frame_count == 0
        || capture.frame_count > 8_192
        || capture.payload_bytes > 8_388_608
    {
        return Err("live gameplay wire capture identity or bounds are invalid".into());
    }
    let expected_hash_bytes: [u8; 32] = (0..32)
        .map(|index| u8::from_str_radix(&expected_hash[index * 2..index * 2 + 2], 16))
        .collect::<std::result::Result<Vec<_>, _>>()?
        .try_into()
        .map_err(|_| "invalid map hash width")?;
    let target = WorldPosition::from_tile_center(TileCoord::new(
        capture.destination_tile[0],
        capture.destination_tile[1],
    ))?;
    let mut primary: Option<EntityId> = None;
    let mut welcome_matches = false;
    let mut order: Option<(u64, EntityId, WorldPosition)> = None;
    let mut accepted = false;
    let mut observed_motion_after_order = false;
    let mut arrived_after_motion = false;
    let mut observed_positions = 0usize;
    let mut decoded_payload_bytes = 0usize;
    for frame in &capture.frames {
        let bytes = decode_hex(&frame.payload_hex)?;
        decoded_payload_bytes = decoded_payload_bytes.saturating_add(bytes.len());
        if bytes.len() > aoe_protocol::GAMEPLAY_MAX_MESSAGE {
            return Err("captured gameplay frame exceeds the protocol bound".into());
        }
        match frame.direction.as_str() {
            "sent" => match decode_gameplay_client(&bytes)? {
                GameplayClientMessage::MoveOrder {
                    sequence,
                    entity_id,
                    destination,
                } => {
                    if order.replace((sequence, entity_id, destination)).is_some() {
                        return Err("browser issued more than one live candidate move".into());
                    }
                }
                GameplayClientMessage::Hello { version, .. } if version != GAMEPLAY_VERSION => {
                    return Err("live browser used an unexpected gameplay protocol version".into());
                }
                _ => {}
            },
            "received" => match decode_gameplay_server(&bytes)? {
                GameplayServerMessage::Welcome {
                    version,
                    map_content_hash,
                    role,
                    primary_unit_id,
                    tick_hz,
                    ..
                } => {
                    welcome_matches = version == GAMEPLAY_VERSION
                        && map_content_hash == Some(expected_hash_bytes)
                        && role == GameplayRole::Controller
                        && tick_hz == 20;
                    primary = Some(primary_unit_id);
                }
                GameplayServerMessage::CommandAck {
                    sequence, result, ..
                } => {
                    if let Some((expected_sequence, _, _)) = order
                        && sequence == expected_sequence
                    {
                        accepted = result == aoe_protocol::CommandResult::Accepted;
                    }
                }
                GameplayServerMessage::Snapshot { units, .. } => {
                    if let Some(id) = primary
                        && let Some(unit) = units.into_iter().find(|unit| unit.id == id)
                    {
                        observed_positions += 1;
                        let moving = unit.moving || unit.planning;
                        if order.is_some() {
                            observed_motion_after_order |= moving;
                            arrived_after_motion |=
                                observed_motion_after_order && unit.position == target && !moving;
                        }
                    }
                }
                GameplayServerMessage::Tick { changed_units, .. } => {
                    if let Some(id) = primary
                        && let Some(unit) = changed_units.into_iter().find(|unit| unit.id == id)
                    {
                        observed_positions += 1;
                        let moving = unit.moving || unit.planning;
                        if order.is_some() {
                            observed_motion_after_order |= moving;
                            arrived_after_motion |=
                                observed_motion_after_order && unit.position == target && !moving;
                        }
                    }
                }
                _ => {}
            },
            _ => return Err("captured gameplay frame has an invalid direction".into()),
        }
    }
    if decoded_payload_bytes != capture.payload_bytes {
        return Err("captured gameplay byte accounting does not match its frames".into());
    }
    let (sequence, order_entity, wire_destination) =
        order.ok_or("browser sent no candidate MoveOrder")?;
    let primary_unit = primary.ok_or("candidate Welcome omitted its primary unit")?;
    if !welcome_matches
        || order_entity != primary_unit
        || wire_destination != target
        || !accepted
        || !observed_motion_after_order
        || !arrived_after_motion
    {
        return Err(format!("decoded live candidate movement did not prove controller arrival: welcome={welcome_matches} sequence={sequence} target={target:?} wire_target={wire_destination:?} order_entity={order_entity:?} primary={primary_unit:?} accepted={accepted} moving={observed_motion_after_order} arrived={arrived_after_motion}").into());
    }
    Ok(serde_json::json!({
        "protocol_version": GAMEPLAY_VERSION,
        "map_content_hash": expected_hash,
        "role": "controller",
        "sequence": sequence,
        "primary_unit_id": primary_unit.0,
        "target_tile": capture.destination_tile,
        "target_subunits": {"x": target.x, "y": target.y},
        "accepted": accepted,
        "observed_motion": observed_motion_after_order,
        "arrived_idle_at_exact_target": arrived_after_motion,
        "authoritative_position_observations": observed_positions,
        "qualification": "one browser-issued source candidate move, not long-distance/global traversal"
    }))
}

fn validate_hash(hash: &str) -> Result<()> {
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("country content hash must be 64 lowercase hexadecimal digits".into());
    }
    Ok(())
}

fn monitor_server_rss(pid: u32) -> (Arc<AtomicBool>, Arc<AtomicU64>, thread::JoinHandle<()>) {
    let stop = Arc::new(AtomicBool::new(false));
    let peak = Arc::new(AtomicU64::new(0));
    let stop_thread = Arc::clone(&stop);
    let peak_thread = Arc::clone(&peak);
    let task = thread::spawn(move || {
        let status = PathBuf::from(format!("/proc/{pid}/status"));
        while !stop_thread.load(Ordering::Relaxed) {
            if let Ok(contents) = fs::read_to_string(&status)
                && let Some(kilobytes) = contents.lines().find_map(|line| {
                    line.strip_prefix("VmRSS:")
                        .and_then(|value| value.split_whitespace().next())
                        .and_then(|value| value.parse::<u64>().ok())
                })
            {
                peak_thread.fetch_max(kilobytes.saturating_mul(1024), Ordering::Relaxed);
            }
            thread::sleep(Duration::from_millis(50));
        }
    });
    (stop, peak, task)
}

pub(super) fn run(directory: &Path, hash: &str) -> Result<()> {
    validate_hash(hash)?;
    let root = env::current_dir()?.canonicalize()?;
    let directory = directory.canonicalize()?;
    let pack = env::var_os("AOE_ASSET_PACK")
        .filter(|value| !value.is_empty())
        .ok_or("country browser qualification requires an original AOE_ASSET_PACK")?;
    let pack = Path::new(&pack).canonicalize()?;
    let assets = aoe_assets::pack::verify(&pack)?;
    let native = aoe_server::run_source_country_probe(&directory, hash)?;
    if !native.typed_hydrology || native.source_lock_count != 9 || native.start.is_none() {
        return Err("country browser qualification requires typed nine-source evidence and an ordinary start".into());
    }
    fs::create_dir_all(root.join("target"))?;
    let staged = tempfile::Builder::new()
        .prefix("country-source-")
        .tempdir_in(root.join("target"))?;
    super::landscape::stage_candidate(&directory, staged.path(), hash)?;
    let evidence_root = root.join("reports/country-source");
    fs::create_dir_all(&evidence_root)?;
    // Keep each run's artifacts; prior captures cannot satisfy a new attempt.
    let evidence = tempfile::Builder::new()
        .prefix("run-")
        .tempdir_in(&evidence_root)?
        .keep();
    fs::write(
        evidence.join("native.json"),
        serde_json::to_vec_pretty(&native)?,
    )?;
    fs::write(
        evidence.join("asset-manifest.json"),
        serde_json::to_vec_pretty(&assets)?,
    )?;
    let revision = Command::new("git").args(["rev-parse", "HEAD"]).output()?;
    if !revision.status.success() {
        return Err("could not record country capture revision".into());
    }
    let status = Command::new("git")
        .args(["status", "--porcelain"])
        .output()?;
    if !status.status.success() {
        return Err("could not record country source dirty status".into());
    }
    fs::write(
        evidence.join("inputs.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "policy": "explicit-candidate-country-browser-v1", "content_hash": hash,
            "revision": String::from_utf8(revision.stdout)?.trim(), "dirty": !status.stdout.is_empty(),
            "package_directory": directory, "asset_pack": pack,
            "limits": "temporary isolated server; original default unchanged; software GPU; not global traversal or hardware qualification"
        }))?,
    )?;
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    drop(listener);
    let mut server = Server(
        Command::new(root.join("target/release/aoe-server"))
            .current_dir(&root)
            .env("AOE_BIND", address.to_string())
            .env("AOE_SCENARIO", "smoke")
            .env_remove("AOE_MAP_WORKER")
            .env("AOE_MAP_PACKAGE_DIRECTORY", staged.path())
            .env("AOE_ASSET_PACK", &pack)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()?,
    );
    let deadline = Instant::now() + Duration::from_secs(60);
    while !ready(address) {
        if let Some(status) = server.0.try_wait()? {
            return Err(format!("country test server exited: {status}").into());
        }
        if Instant::now() >= deadline {
            return Err("country test server readiness exceeded60s".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let name = format!("aoeworld-country-{}-{}", std::process::id(), address.port());
    let _cleanup = BrowserContainer(name.clone());
    let user = format!(
        "{}:{}",
        nix::unistd::Uid::current(),
        nix::unistd::Gid::current()
    );
    let args = vec![
        "run".to_owned(),
        "--rm".into(),
        "--init".into(),
        "--network".into(),
        "host".into(),
        "--ipc".into(),
        "host".into(),
        "--user".into(),
        user,
        "--name".into(),
        name,
        "-e".into(),
        format!("HOME={}/.cache/browser-home", root.display()),
        "-e".into(),
        format!("AOE_BASE_URL=http://{address}"),
        "-e".into(),
        format!("AOE_COUNTRY_SOURCE_HASH={hash}"),
        "-e".into(),
        format!("AOE_COUNTRY_SOURCE_OUTPUT={}", evidence.display()),
        "-v".into(),
        format!("{}:{}", root.display(), root.display()),
        "-w".into(),
        root.join("browser").display().to_string(),
        "aoeworld/browser-tools:1.63.0".into(),
        "xvfb-run".into(),
        "-a".into(),
        "npx".into(),
        "playwright".into(),
        "test".into(),
        "tests/country/source.spec.ts".into(),
        "--project=webgpu".into(),
    ];
    let refs: Vec<_> = args.iter().map(String::as_str).collect();
    let (stop_monitor, peak_rss, monitor) = monitor_server_rss(server.0.id());
    let browser_result = process::run("docker", &refs, Duration::from_secs(900));
    stop_monitor.store(true, Ordering::SeqCst);
    monitor
        .join()
        .map_err(|_| "candidate server RSS monitor panicked")?;
    browser_result?;
    server.shutdown()?;
    let server_peak_rss = peak_rss.load(Ordering::Relaxed);
    if server_peak_rss == 0 || server_peak_rss > 1_073_741_824 {
        return Err(format!(
            "candidate server RSS sample is missing or exceeds1GiB: {server_peak_rss}"
        )
        .into());
    }
    let browser_path = evidence.join("browser.json");
    if fs::metadata(&browser_path)?.len() > 4 * 1_048_576 {
        return Err("candidate browser summary exceeds its bounded JSON limit".into());
    }
    let result: serde_json::Value = serde_json::from_slice(&fs::read(browser_path)?)?;
    let observations = result["observations"]
        .as_array()
        .ok_or("country browser did not retain backend observations")?;
    if observations.len() != 3
        || result["content_hash"].as_str() != Some(hash)
        || ["webgpu", "webgl2", "canvas2d"].iter().any(|backend| {
            !observations.iter().any(|observation| {
                observation["backend"].as_str() == Some(backend)
                    && observation["content_hash"].as_str() == Some(hash)
                    && observation["errors"].as_array().is_some_and(Vec::is_empty)
            })
        })
    {
        return Err(
            "country browser did not retain all three current-run renderer captures".into(),
        );
    }
    let memory_path = evidence.join("memory.json");
    if fs::metadata(&memory_path)?.len() > 4 * 1_048_576 {
        return Err("candidate memory evidence exceeds its bounded JSON limit".into());
    }
    let memory: serde_json::Value = serde_json::from_slice(&fs::read(memory_path)?)?;
    let unique_chunks = memory["unique_source_chunks"].as_u64().unwrap_or(0);
    let maximum_chromium_rss = memory["maximum_sampled_chromium_rss_bytes"]
        .as_u64()
        .unwrap_or(u64::MAX);
    let final_spread = memory["final_four_sample_spread_bytes"]
        .as_u64()
        .unwrap_or(u64::MAX);
    if memory["content_hash"].as_str() != Some(hash)
        || memory["samples"]
            .as_array()
            .is_none_or(|samples| samples.len() != 24)
        || unique_chunks == 0
        || memory["eviction_limit_exercised"].as_bool() != Some(unique_chunks > 512)
        || maximum_chromium_rss > 1_073_741_824
        || final_spread > 67_108_864
    {
        return Err(
            "candidate browser source-memory observation did not meet its bounded evidence contract"
                .into(),
        );
    }
    fs::write(
        evidence.join("runtime-limits.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "content_hash": hash,
            "candidate_server_peak_rss_bytes": server_peak_rss,
            "candidate_server_rss_limit_bytes": 1_073_741_824,
            "browser_summary_bytes": fs::metadata(evidence.join("browser.json"))?.len(),
            "browser_source_memory": memory
        }))?,
    )?;
    let live = validate_live_wire(&evidence, hash)?;
    fs::write(
        evidence.join("live-movement.json"),
        serde_json::to_vec_pretty(&live)?,
    )?;
    println!("country source capture artifacts: {}", evidence.display());
    Ok(())
}

#[cfg(test)]
mod tests;
