//! Local-only building proof preparation. Uses the normal gameplay build order;
//! no scenario, game state, sprite or terrain is fabricated for the recording.
use aoe_core::{EntityId, TileCoord, TileRect};
use aoe_protocol::{
    BuildingKind, ConstructionState, GameplayBuildingState, GameplayClientMessage as Client,
    GameplayServerMessage as Server, UnitKind,
};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{fs, path::PathBuf, time::Duration};
use tokio_tungstenite::{connect_async, tungstenite::Message};
#[path = "building-proof/fog.rs"]
mod fog;
#[path = "building-proof/maps.rs"]
mod maps;
#[path = "building-proof/packing.rs"]
mod packing;
#[path = "building-proof/servers.rs"]
mod servers;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    mode: String,
    package: Option<PathBuf>,
    url: Option<String>,
    base: Option<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let request: Request = serde_json::from_slice(&fs::read(
        std::env::args().nth(1).ok_or("request JSON required")?,
    )?)?;
    fs::create_dir_all(".cache/tc-shots")?;
    if request.mode == "pack" {
        return packing::check(request.package.ok_or("package required")?);
    }
    if request.mode == "servers" {
        return servers::run(
            request.package.ok_or("asset pack required")?,
            request.base.as_deref(),
        );
    }
    if request.mode == "maps" {
        return maps(request.package.ok_or("package required")?);
    }
    let url = request.url.ok_or("url required")?;
    let url = url::Url::parse(&url)?;
    if url.scheme() != "http"
        || !matches!(url.host_str(), Some("127.0.0.1" | "localhost"))
        || url.port().is_none_or(|p| matches!(p, 8080 | 8081))
    {
        return Err("use an explicit loopback HTTP port other than 8080/8081".into());
    }
    if !matches!(
        request.mode.as_str(),
        "camera" | "house" | "village" | "fog"
    ) {
        return Err("unknown proof mode".into());
    }
    if request.mode == "fog" {
        return tokio::runtime::Runtime::new()?.block_on(async {
            tokio::time::timeout(Duration::from_secs(180), fog::prepare(url.as_str())).await?
        });
    }
    tokio::runtime::Runtime::new()?.block_on(async {
        tokio::time::timeout(
            Duration::from_secs(if request.mode == "village" { 900 } else { 180 }),
            village(
                url.as_str(),
                request.mode != "camera",
                request.mode == "village",
            ),
        )
        .await?
    })
}

fn maps(package: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    maps::prepare(
        package,
        std::path::Path::new(".cache/tc-shots/maps"),
        std::path::Path::new(".cache/tc-shots/bookmarks.json"),
    )
}

fn record(buildings: &[GameplayBuildingState]) -> Result<(), Box<dyn std::error::Error>> {
    fs::write(
        ".cache/tc-shots/village.json",
        serde_json::to_vec_pretty(buildings)?,
    )?;
    let tc = buildings
        .iter()
        .find(|b| b.kind == BuildingKind::TownCenter)
        .ok_or("Town Center missing")?;
    let [x, y] = [
        f64::from(tc.footprint.min.x) + f64::from(tc.footprint.width()) / 2.0,
        f64::from(tc.footprint.min.y) + f64::from(tc.footprint.height()) / 2.0,
    ];
    let path = ".cache/tc-shots/bookmarks.json";
    let mut marks: Value = serde_json::from_slice(&fs::read(path)?)?;
    for mark in marks["bookmarks"].as_array_mut().ok_or("bookmarks")? {
        mark["x"] = x.into();
        mark["y"] = y.into();
    }
    fs::write(path, serde_json::to_vec_pretty(&marks)?)?;
    println!("Town Center camera {x},{y}; buildings saved to .cache/tc-shots/village.json");
    Ok(())
}

async fn village(url: &str, build: bool, full: bool) -> Result<(), Box<dyn std::error::Error>> {
    let (mut socket, _) =
        connect_async(format!("{}game/ws", url.replacen("http", "ws", 1))).await?;
    socket
        .send(Message::Binary(
            aoe_protocol::encode_gameplay_client(&Client::Hello {
                version: aoe_protocol::GAMEPLAY_VERSION,
                resume_token: None,
            })?
            .into(),
        ))
        .await?;
    let mut buildings = Vec::<GameplayBuildingState>::new();
    let mut builders = Vec::<EntityId>::new();
    let mut candidates = Vec::<TileCoord>::new();
    let mut sequence = 0;
    let mut next_sequence = 100_u64;
    let plan = if full {
        vec![
            BuildingKind::House,
            BuildingKind::Mill,
            BuildingKind::Barracks,
            BuildingKind::Farm,
        ]
    } else {
        vec![BuildingKind::House]
    };
    let mut target = BuildingKind::House;
    let mut wood = 0;
    let mut trees = Vec::new();
    let mut gathering = false;
    while let Some(message) = socket.next().await {
        let Message::Binary(bytes) = message? else {
            continue;
        };
        match aoe_protocol::decode_gameplay_server(&bytes)? {
            Server::Welcome {
                width_tiles,
                height_tiles,
                ..
            } => {
                socket
                    .send(Message::Binary(
                        aoe_protocol::encode_gameplay_client(&Client::Subscribe {
                            revision: 1,
                            region: TileRect::from_xywh(
                                width_tiles / 2 - 64,
                                height_tiles / 2 - 64,
                                128,
                                128,
                            ),
                        })?
                        .into(),
                    ))
                    .await?;
            }
            Server::Snapshot {
                revision,
                units,
                buildings: received,
                players,
                ..
            } => {
                wood = players.first().map_or(0, |p| p.stockpile.wood);
                buildings = received;
                if !build {
                    record(&buildings)?;
                    return Ok(());
                }
                builders = units
                    .iter()
                    .filter(|u| u.kind == UnitKind::Villager)
                    .map(|u| u.id)
                    .collect();
                let tc = buildings
                    .iter()
                    .find(|b| b.kind == BuildingKind::TownCenter)
                    .ok_or("Town Center missing")?;
                let at = tc.footprint.min;
                if builders.is_empty() {
                    socket
                        .send(Message::Binary(
                            aoe_protocol::encode_gameplay_client(&Client::Subscribe {
                                revision: revision + 1,
                                region: TileRect::from_xywh(at.x - 32, at.y - 32, 64, 64),
                            })?
                            .into(),
                        ))
                        .await?;
                    continue;
                }
                candidates.clear();
                for radius in [5, 7, 9, 11] {
                    for (dx, dy) in [
                        (radius, 0),
                        (0, radius),
                        (-radius, 0),
                        (0, -radius),
                        (radius, radius),
                        (-radius, radius),
                    ] {
                        candidates.push(TileCoord::new(at.x + dx, at.y + dy));
                    }
                }
                candidates.reverse();
            }
            Server::Tick {
                changed_buildings,
                changed_players,
                ..
            } => {
                if let Some(p) = changed_players.first() {
                    wood = p.stockpile.wood;
                }
                for b in changed_buildings {
                    if let Some(slot) = buildings.iter().position(|old| old.id == b.id) {
                        buildings[slot] = b;
                    } else {
                        buildings.push(b);
                    }
                }
            }
            Server::CommandAck { result, .. }
                if result != aoe_protocol::CommandResult::Accepted =>
            {
                println!("placement rejected: {result:?}");
                sequence = 0;
            }
            Server::ResourceState(state) => {
                trees.extend(
                    state
                        .changes
                        .iter()
                        .filter(|a| a.remaining > 0)
                        .filter_map(|a| aoe_map::ResourceNode::starting_from_id(a.id))
                        .filter(|n| n.kind == aoe_map::ResourceKind::Wood)
                        .map(|n| n.id),
                );
                trees.sort_unstable();
                trees.dedup();
            }
            Server::Error { message, .. } => return Err(message.into()),
            _ => {}
        }
        let pending = plan.iter().copied().find(|kind| {
            !buildings
                .iter()
                .any(|b| b.kind == *kind && b.construction == ConstructionState::Complete)
        });
        let Some(kind) = pending else {
            // Farm builders start harvesting automatically; park them before capture.
            if full {
                let at = buildings
                    .iter()
                    .find(|b| b.kind == BuildingKind::TownCenter)
                    .ok_or("Town Center missing")?
                    .footprint
                    .min;
                for id in &builders {
                    next_sequence += 1;
                    socket
                        .send(Message::Binary(
                            aoe_protocol::encode_gameplay_client(&Client::MoveOrder {
                                sequence: next_sequence,
                                entity_id: *id,
                                destination: aoe_core::WorldPosition::from_tile_center(
                                    TileCoord::new(at.x - 1, at.y),
                                )?,
                            })?
                            .into(),
                        ))
                        .await?;
                    while let Some(message) = socket.next().await {
                        let Message::Binary(bytes) = message? else {
                            continue;
                        };
                        if let Server::CommandAck {
                            sequence, result, ..
                        } = aoe_protocol::decode_gameplay_server(&bytes)?
                            && sequence == next_sequence
                        {
                            if result != aoe_protocol::CommandResult::Accepted {
                                return Err(format!("parking villager: {result:?}").into());
                            }
                            break;
                        }
                    }
                }
            }
            record(&buildings)?;
            println!("Requested village buildings completed through normal build orders");
            return Ok(());
        };
        if kind != target {
            target = kind;
            sequence = 0;
            let at = buildings
                .iter()
                .find(|b| b.kind == BuildingKind::TownCenter)
                .ok_or("Town Center missing")?
                .footprint
                .min;
            candidates.clear();
            for radius in [5, 7, 9, 11] {
                for (dx, dy) in [
                    (radius, 0),
                    (0, radius),
                    (-radius, 0),
                    (0, -radius),
                    (radius, radius),
                    (-radius, radius),
                ] {
                    candidates.push(TileCoord::new(at.x + dx, at.y + dy));
                }
            }
            candidates.reverse();
        }
        let needed: u32 = plan
            .iter()
            .filter(|kind| !buildings.iter().any(|b| b.kind == **kind))
            .map(|kind| match kind {
                BuildingKind::House => 25,
                BuildingKind::Mill => 100,
                BuildingKind::Barracks => 175,
                BuildingKind::Farm => 60,
                _ => 0,
            })
            .sum();
        if full && sequence == 0 && wood < needed {
            if !gathering && !trees.is_empty() && !builders.is_empty() {
                for (i, id) in builders.iter().enumerate() {
                    next_sequence += 1;
                    socket
                        .send(Message::Binary(
                            aoe_protocol::encode_gameplay_client(&Client::GatherOrder {
                                sequence: next_sequence,
                                entity_id: *id,
                                resource_id: trees[i % trees.len()],
                            })?
                            .into(),
                        ))
                        .await?;
                }
                println!("Gathering wood for village: {wood}/{needed}");
                gathering = true;
            }
            continue;
        }
        if build && sequence == 0 && !builders.is_empty() {
            gathering = false;
            let origin = candidates
                .pop()
                .ok_or("no legal nearby building placement found")?;
            next_sequence += 1;
            let next = next_sequence;
            println!(
                "request {kind:?} at {},{} with {} villagers (wood {wood})",
                origin.x,
                origin.y,
                builders.len()
            );
            socket
                .send(Message::Binary(
                    aoe_protocol::encode_gameplay_client(&Client::BuildOrder {
                        sequence: next,
                        builder_ids: builders.clone(),
                        kind,
                        origin,
                        queued: false,
                    })?
                    .into(),
                ))
                .await?;
            sequence = next;
        }
    }
    Err("gameplay connection closed before proof preparation completed".into())
}
