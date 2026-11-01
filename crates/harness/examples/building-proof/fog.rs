//! Scout the opt-in showcase enemy, then return: memory comes from simulation.
use super::*;

pub async fn prepare(url: &str) -> Result<(), Box<dyn std::error::Error>> {
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
    let mut scout = None;
    let mut home = None;
    let mut enemy: Option<GameplayBuildingState> = None;
    let mut returning = false;
    let mut route = Vec::new();
    let mut destination = None;
    let mut touring = false;
    let mut sequence = 50_000;
    while let Some(message) = socket.next().await {
        let Message::Binary(bytes) = message? else {
            continue;
        };
        let message = aoe_protocol::decode_gameplay_server(&bytes)?;
        match &message {
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
                buildings,
                units,
                revision,
                ..
            } => {
                let tc = buildings
                    .iter()
                    .find(|b| b.kind == BuildingKind::TownCenter && b.player.0 == 0)
                    .ok_or("own Town Center missing")?;
                if let Some(unit) = units.iter().find(|u| u.kind == UnitKind::Scout) {
                    scout = Some(unit.id);
                    home = Some(unit.position);
                    sequence += 1;
                    let start = unit.position.tile_floor();
                    socket
                        .send(Message::Binary(
                            aoe_protocol::encode_gameplay_client(&Client::MoveOrder {
                                sequence,
                                entity_id: unit.id,
                                destination: aoe_core::WorldPosition::from_tile_center(
                                    TileCoord::new(start.x + 15, start.y + 8),
                                )?,
                            })?
                            .into(),
                        ))
                        .await?;
                    println!("scouting east from {},{}", start.x, start.y);
                } else {
                    socket
                        .send(Message::Binary(
                            aoe_protocol::encode_gameplay_client(&Client::Subscribe {
                                revision: revision + 1,
                                region: TileRect::from_xywh(
                                    tc.footprint.min.x - 48,
                                    tc.footprint.min.y - 48,
                                    128,
                                    128,
                                ),
                            })?
                            .into(),
                        ))
                        .await?;
                }
                enemy = buildings.iter().find(|b| b.player.0 == 1).cloned();
            }
            Server::Tick {
                changed_buildings, ..
            } => {
                if let Some(building) = changed_buildings.iter().find(|b| b.player.0 == 1) {
                    enemy = Some(building.clone());
                }
            }
            Server::CommandAck { result, .. }
                if *result != aoe_protocol::CommandResult::Accepted =>
            {
                return Err(format!("scouting command rejected: {result:?}").into());
            }
            Server::Error { message, .. } => return Err(message.clone().into()),
            _ => {}
        }
        let arrived = match &message {
            Server::Tick { changed_units, .. } => changed_units
                .iter()
                .any(|u| Some(u.id) == scout && !u.moving && Some(u.position) == destination),
            _ => false,
        };
        if let Some(building) = &enemy
            && !touring
        {
            let rect = building.footprint;
            route.push(home.ok_or("home missing")?);
            for at in [
                TileCoord::new(rect.max.x, rect.min.y + 2),
                TileCoord::new(rect.min.x - 1, rect.min.y + 2),
            ] {
                route.push(aoe_core::WorldPosition::from_tile_center(at)?);
            }
            touring = true;
        }
        if touring && !returning && (destination.is_none() || arrived) {
            let next = route.pop().ok_or("scouting route missing")?;
            destination = Some(next);
            returning = route.is_empty();
            sequence += 1;
            socket
                .send(Message::Binary(
                    aoe_protocol::encode_gameplay_client(&Client::MoveOrder {
                        sequence,
                        entity_id: scout.ok_or("scout missing")?,
                        destination: next,
                    })?
                    .into(),
                ))
                .await?;
            println!("enemy DAT Town Center seen; scouting footprint, returning={returning}");
        }
        if let Server::FogState(state) = message
            && returning
            && !state.disabled
            && let Some(building) = &enemy
        {
            let at = building.footprint.min;
            let remembered = (building.footprint.min.y..building.footprint.max.y).all(|y| {
                (building.footprint.min.x..building.footprint.max.x).all(|x| {
                    let tile = TileCoord::new(x, y);
                    let index = ((y - state.region.min.y) * state.region.width() + x
                        - state.region.min.x) as usize;
                    state.region.contains(tile) && state.tiles.get(index) == Some(&1)
                })
            });
            if remembered {
                fs::write(
                    ".cache/tc-shots/final-fog-memory.json",
                    serde_json::to_vec_pretty(&json!({
                        "building": building, "fog": state,
                        "method": "ordinary scout move, enemy seen, scout returned; real simulation memory",
                    }))?,
                )?;
                println!(
                    "remembered enemy Town Center in explored fog at {},{}",
                    at.x, at.y
                );
                return Ok(());
            }
        }
    }
    Err("connection closed before enemy was remembered".into())
}
