//! Saved worlds use the server's skirmish restore and its ordinary AI fleet.
use super::{BoxError, MapSource, Options, http, report, run::Server};
use aoe_core::TileRect;
use aoe_protocol::{
    GAMEPLAY_VERSION, GameplayClientMessage, GameplayServerMessage, decode_gameplay_server,
    encode_gameplay_client,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::{
    fs,
    process::Command,
    thread,
    time::{Duration, Instant},
};
use tokio::{runtime::Runtime, task::JoinHandle};
use tokio_tungstenite::{connect_async, tungstenite::Message};

pub(super) fn label(options: &Options) -> String {
    format!("aoe-scale-save-{}", options.port)
}

/// Validate before building or starting anything. The server validates the postcard
/// and terrain pages on restore; its card is the canonical map-package reference.
pub(super) fn prepare(options: &mut Options) -> Result<Option<Value>, BoxError> {
    let Some(path) = &options.save else {
        return Ok(None);
    };
    if path.extension().is_none_or(|s| s != "save") || !path.is_file() {
        return Err(format!("--save needs an existing .save file: {}", path.display()).into());
    }
    let card_path = path.with_extension("json");
    let bytes =
        fs::read(&card_path).map_err(|e| format!("save card {}: {e}", card_path.display()))?;
    let card: Value = serde_json::from_slice(&bytes)?;
    let id = card["id"].as_str().ok_or("save card has no id")?;
    if id.is_empty()
        || id.len() > 64
        || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        || path.file_stem().and_then(|s| s.to_str()) != Some(id)
    {
        return Err(
            "save card id must match the .save filename and use letters, digits or hyphens".into(),
        );
    }
    let hash = card["map_hash"]
        .as_str()
        .ok_or("save card has no map_hash")?;
    if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("save card map_hash must contain 64 hex digits".into());
    }
    let package_path = options.package_directory.join(format!("{hash}.json"));
    let package_bytes = fs::read(&package_path).map_err(|e| format!(
        "missing map package {hash} required by {}: {} ({e}); supply its directory with --package-directory",
        path.display(), package_path.display()))?;
    let package: aoe_map::MapPackage = serde_json::from_slice(&package_bytes)?;
    package.validate()?;
    if package.content_hash_hex() != hash {
        return Err("save map package identity differs from its card".into());
    }
    let opponents = card["opponents"]
        .as_u64()
        .filter(|n| *n <= 99)
        .ok_or("save card needs 0..99 opponents")? as usize;
    let directory = options.package_directory.join("saves");
    fs::create_dir_all(&directory)?;
    import_pair(path, &card_path, &directory, id)?;
    options.players = opponents + 1;
    options.humans = 1;
    options.hosts = opponents.div_ceil(13);
    options.tiles = u32::try_from(package.estimate.tiles_per_side)?;
    options.map = MapSource::Package;
    options.map_hash = Some(hash.to_owned());
    Ok(Some(card))
}

/// Reserve both names before copying; `create_new` also refuses races and dangling links.
fn import_pair(
    path: &std::path::Path,
    card: &std::path::Path,
    directory: &std::path::Path,
    id: &str,
) -> Result<(), BoxError> {
    let destinations = [
        directory.join(format!("{id}.save")),
        directory.join(format!("{id}.json")),
    ];
    let sources = [path, card];
    if sources
        .iter()
        .zip(&destinations)
        .all(|(source, destination)| {
            fs::canonicalize(source)
                .ok()
                .zip(fs::canonicalize(destination).ok())
                .is_some_and(|(a, b)| a == b)
        })
    {
        return Ok(());
    }
    for destination in &destinations {
        match fs::symlink_metadata(destination) {
            Ok(_) => return Err(format!("save destination already exists: {}; select the stored save directly or use another ID", destination.display()).into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
            Err(error) => return Err(error.into()),
        }
    }
    let mut reserved = Vec::new();
    let result = (|| -> Result<(), BoxError> {
        for destination in &destinations {
            reserved.push(fs::File::create_new(destination)?);
        }
        for (source, output) in sources.iter().zip(&mut reserved) {
            std::io::copy(&mut fs::File::open(source)?, output)?;
        }
        Ok(())
    })();
    if result.is_err() {
        for destination in destinations.iter().take(reserved.len()) {
            fs::remove_file(destination)?;
        }
    }
    result
}

/// A real human socket keeps the skirmish alive, without issuing any orders.
/// The runtime drains messages in the background and closes them on drop.
pub(super) struct Human {
    _runtime: Runtime,
    task: JoinHandle<()>,
    token: String,
}

impl Human {
    fn connect(address: &str, view: Option<i32>) -> Result<Self, BoxError> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()?;
        let (token, task) = runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(30), async {
                let (mut socket, _) = connect_async(format!("ws://{address}/game/ws")).await?;
                socket
                    .send(Message::Binary(
                        encode_gameplay_client(&GameplayClientMessage::Hello {
                            version: GAMEPLAY_VERSION,
                            resume_token: None,
                        })?
                        .into(),
                    ))
                    .await?;
                let frame = socket
                    .next()
                    .await
                    .ok_or("human socket closed before welcome")??;
                let GameplayServerMessage::Welcome {
                    resume_token,
                    width_tiles,
                    height_tiles,
                    ..
                } = decode_gameplay_server(&frame.into_data())?
                else {
                    return Err::<_, BoxError>("human socket was not welcomed".into());
                };
                let token = resume_token.0.iter().map(|b| format!("{b:02x}")).collect();
                // A session with no subscription gets no frames. A one-tile
                // view still lists the viewer's own units, so the first frame
                // tells where the primary unit stands; the camera follows.
                if view.is_some() {
                    let region = TileRect::from_xywh(0, 0, 1, 1);
                    let message = GameplayClientMessage::Subscribe {
                        revision: 1,
                        region,
                        unit_zoom: true,
                    };
                    socket
                        .send(Message::Binary(encode_gameplay_client(&message)?.into()))
                        .await?;
                }
                let task = tokio::spawn(async move {
                    let mut view = view;
                    while let Some(Ok(frame)) = socket.next().await {
                        if frame.is_close() {
                            break;
                        }
                        let Some(side) = view else { continue };
                        let Ok(GameplayServerMessage::Snapshot { units, .. }) =
                            decode_gameplay_server(&frame.into_data())
                        else {
                            continue;
                        };
                        // The one-tile view in the corner holds only the
                        // viewer's own units: the camera goes to their middle
                        // (the primary unit may be garrisoned or gone).
                        let middle = |axis: fn(&aoe_protocol::GameplayUnitState) -> i32| {
                            let mut values: Vec<_> = units.iter().map(axis).collect();
                            values.sort_unstable();
                            values.get(values.len() / 2).copied()
                        };
                        let (Some(x), Some(y)) = (
                            middle(|u| u.position.tile_floor().x),
                            middle(|u| u.position.tile_floor().y),
                        ) else {
                            continue;
                        };
                        let tile = aoe_core::TileCoord::new(x, y);
                        let region =
                            TileRect::from_xywh(tile.x - side / 2, tile.y - side / 2, side, side)
                                .clamp(width_tiles, height_tiles);
                        let message = GameplayClientMessage::Subscribe {
                            revision: 2,
                            region,
                            unit_zoom: true,
                        };
                        let Ok(bytes) = encode_gameplay_client(&message) else {
                            break;
                        };
                        if socket.send(Message::Binary(bytes.into())).await.is_err() {
                            break;
                        }
                        println!("scale-match: the human watches {region:?}");
                        view = None;
                    }
                });
                Ok((token, task))
            })
            .await?
        })?;
        Ok(Self {
            _runtime: runtime,
            task,
            token,
        })
    }
}

pub(super) fn resume(
    server: &Server,
    card: &Value,
    view: Option<i32>,
) -> Result<(Human, Value), BoxError> {
    let bootstrap = Human::connect(&server.address, None)?;
    let response = http::request(
        &server.address,
        "POST",
        "/skirmish",
        &[("X-AoeWorld-Controller-Token", &bootstrap.token)],
        Some(&serde_json::to_vec(&json!({"save":card["id"]}))?),
        Duration::from_secs(30),
    )?;
    if response.status != 202 {
        return Err(format!(
            "POST /skirmish: {}: {}",
            response.status,
            String::from_utf8_lossy(&response.body)
        )
        .into());
    }
    let started = Instant::now();
    let mut human = None;
    loop {
        let mut status = server.get("/skirmish")?;
        match status["phase"].as_str() {
            Some("ready") => {
                status["connect_seconds"] = json!(started.elapsed().as_secs_f64());
                return Ok((
                    human
                        .map(Ok)
                        .unwrap_or_else(|| Human::connect(&server.address, view))?,
                    status,
                ));
            }
            Some("connecting_ai") if human.is_none() => {
                human = Some(Human::connect(&server.address, view)?);
            }
            Some("failed" | "cancelled" | "stopped") => {
                return Err(format!("saved skirmish failed: {status}").into());
            }
            _ => {}
        }
        if started.elapsed() > Duration::from_secs(180) {
            return Err(format!("saved skirmish not ready within 180 s: {status}").into());
        }
        thread::sleep(Duration::from_millis(250));
    }
}

pub(super) fn check_running(server: &Server, human: Option<&Human>) -> Result<(), BoxError> {
    if human.is_some_and(|h| h.task.is_finished()) {
        return Err("saved skirmish human connection ended".into());
    }
    let status = server.get("/skirmish")?;
    if status["phase"] != "ready" || status["completed"] != status["total"] {
        return Err(format!("saved skirmish lost readiness: {status}").into());
    }
    Ok(())
}

pub(super) fn host_names(options: &Options) -> Result<Vec<String>, BoxError> {
    let output = Command::new("docker")
        .args([
            "ps",
            "--filter",
            &format!("label={}", label(options)),
            "--format",
            "{{.Names}}",
        ])
        .output()?;
    if !output.status.success() {
        return Err("could not list saved skirmish AI containers".into());
    }
    Ok(String::from_utf8(output.stdout)?
        .lines()
        .map(str::to_owned)
        .collect())
}

pub(super) fn sample(
    server: &Server,
    seconds: f64,
    strict: bool,
    development: bool,
) -> Result<Value, BoxError> {
    let perf = server.get("/admin/perf");
    let routes = server.get("/admin/match/routes");
    let mut row = if strict {
        report::phase_minute(Some(perf?), Some(routes?), seconds)
    } else {
        report::phase_minute(perf.ok(), routes.ok(), seconds)
    };
    // Work beside the speed numbers (#865 rule 2): kills and losses so far,
    // and the hits and deaths of the last game minute.
    let state = server.get("/admin/match");
    if let Ok(state) = &state {
        row["work"] = work(state);
    }
    println!("  phases (p50, p95 ms) {row}");
    if development {
        let state = state?;
        row["match_tick"] = state["tick"].clone();
        row["players"] = state["players"].clone();
    }
    Ok(row)
}

fn work(state: &Value) -> Value {
    let total = |key: &str| {
        state["players"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|player| player[key].as_u64())
            .sum::<u64>()
    };
    let minute = &state["combat_per_minute"];
    serde_json::json!({
        "kills": total("kills"),
        "losses": total("losses"),
        "melee_hits_per_minute": minute["melee_hits"],
        "ranged_hits_per_minute": minute["ranged_hits"],
        "deaths_per_minute": minute["deaths"],
    })
}

#[cfg(test)]
mod tests;
