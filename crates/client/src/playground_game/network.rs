use super::*;
use aoe_protocol::{GameplayServerMessage, decode_gameplay_server};
use js_sys::Uint8Array;
use web_sys::{Event, MessageEvent};

pub(super) fn connect(shared: Rc<RefCell<Client>>) -> Result<(), JsValue> {
    let location = shared
        .borrow()
        .document
        .location()
        .ok_or("No document location")?;
    let scheme = if location.protocol()? == "https:" {
        "wss"
    } else {
        "ws"
    };
    let socket = WebSocket::new(&format!("{scheme}://{}/game/ws", location.host()?))?;
    socket.set_binary_type(web_sys::BinaryType::Arraybuffer);
    let id = {
        let mut client = shared.borrow_mut();
        client.connection_id = client.connection_id.saturating_add(1);
        if let Some(old) = client.socket.replace(socket.clone()) {
            let _ = old.close();
        }
        client.status = "connecting".to_owned();
        client.order_feedback.clear();
        client.sent_region = None;
        client.connection_id
    };
    let opened = shared.clone();
    let onopen = Closure::<dyn FnMut(Event)>::new(move |_| {
        let client = &mut *opened.borrow_mut();
        if client.connection_id != id {
            return;
        }
        client.status = "negotiating".to_owned();
        let hello = GameplayClientMessage::Hello {
            version: GAMEPLAY_VERSION,
            resume_token: client.token,
        };
        if let Ok(bytes) = encode_gameplay_client(&hello) {
            let _ = client
                .socket
                .as_ref()
                .and_then(|socket| socket.send_with_u8_array(&bytes).ok());
        }
    });
    socket.set_onopen(Some(onopen.as_ref().unchecked_ref()));
    onopen.forget();

    let received = shared.clone();
    let onmessage = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
        let mut client = received.borrow_mut();
        if client.connection_id != id {
            return;
        }
        let bytes = Uint8Array::new(&event.data()).to_vec();
        match decode_gameplay_server(&bytes) {
            Ok(GameplayServerMessage::Welcome {
                width_tiles,
                height_tiles,
                map_content_hash,
                role,
                primary_unit_id,
                resume_token,
                tick_hz,
                ..
            }) => {
                let map_changed = client.map_content_hash != map_content_hash;
                client.config.width_tiles = width_tiles;
                client.config.height_tiles = height_tiles;
                client.config.tick_hz = tick_hz;
                client.role = Some(role);
                if !client.surface_fixture {
                    client.map_content_hash = map_content_hash;
                }
                controls::map_changed(map_content_hash);
                if map_changed && !client.surface_fixture {
                    client.camera.center =
                        [f64::from(width_tiles) / 2.0, f64::from(height_tiles) / 2.0];
                    client.camera.focus_elevation_meters = 0.0;
                    client.focus_map_hash = None;
                }
                let focused = client.village.focused && !map_changed;
                clear_world(&mut client);
                client.village.focused = focused;
                client.primary = Some(primary_unit_id);
                client.token = resume_token;
                storage::save_token(resume_token);
                client.status = "connected".to_owned();
                resize(&mut client);
                capture::world_received(&mut client, map_changed);
                subscribe(&mut client);
            }
            Ok(GameplayServerMessage::Snapshot {
                revision,
                tick,
                units,
                players,
                buildings,
            }) if revision >= client.revision => {
                if !client.presentation.receive(tick.0, now()) {
                    return;
                }
                client.revision = revision;
                let mut resident = BTreeMap::new();
                for unit in units {
                    resident.insert(unit.id, unit);
                }
                let Client {
                    units,
                    presentation,
                    ..
                } = &mut *client;
                for id in units.keys() {
                    if !resident.contains_key(id) {
                        presentation.remove(*id);
                    }
                }
                for unit in resident.values() {
                    presentation.remember(units.get(&unit.id), unit, tick.0);
                }
                client.units = resident;
                client.village.players = players;
                client.village.rubble.retain(|(_, until)| *until > tick.0);
                map::scene::replace_buildings(&mut client, buildings);
                client.village.tick = tick.0;
                if !client.village.focused {
                    client.village.focused = true;
                    if !capture::current().has_camera() {
                        village::orders::focus_town_center(&mut client, false);
                    }
                }
                let Client {
                    units,
                    selected,
                    village,
                    ..
                } = &mut *client;
                selected.units.retain(|id| {
                    units.contains_key(id) || village.animals.iter().any(|a| a.id == *id)
                });
            }
            Ok(GameplayServerMessage::Snapshot { .. }) => {}
            Ok(GameplayServerMessage::Tick {
                revision,
                tick,
                changed_units,
                removals,
                changed_players,
                changed_buildings,
                building_removals,
            }) if revision == client.revision => {
                let Some(buildings) = merge_buildings(
                    &client.village.buildings,
                    &changed_buildings,
                    &building_removals,
                ) else {
                    reconnect(&mut client, "protocol error; reconnect required");
                    return;
                };
                if !client.presentation.receive(tick.0, now()) {
                    return;
                }
                client.village.tick = tick.0;
                for player in changed_players {
                    client.village.players.retain(|p| p.player != player.player);
                    client.village.players.push(player);
                }
                let expiry = tick.0.saturating_add(u64::from(client.config.tick_hz) * 3);
                let destroyed: Vec<_> = client
                    .village
                    .buildings
                    .iter()
                    .filter(|b| {
                        building_removals.contains(&b.id)
                            && (b.kind != aoe_protocol::BuildingKind::Farm
                                || b.food_remaining.is_some_and(|food| food > 0))
                    })
                    .cloned()
                    .collect();
                client.village.rubble.retain(|(_, until)| *until > tick.0);
                for b in destroyed {
                    if client.village.rubble.len() < aoe_protocol::MAX_ECONOMY_BUILDINGS {
                        client.village.rubble.push((b, expiry));
                    }
                }
                map::scene::replace_buildings(&mut client, buildings);
                for id in removals {
                    client.units.remove(&id);
                    client.selected.units.retain(|selected| *selected != id);
                    client.presentation.remove(id);
                }
                for unit in &changed_units {
                    remember(&mut client, unit, tick.0);
                }
            }
            Ok(GameplayServerMessage::Tick { .. }) => {}
            Ok(GameplayServerMessage::ResourceState(state)) => {
                if state.subscription_revision == client.revision && !client.resources.apply(state)
                {
                    reconnect(
                        &mut client,
                        "resource state out of sync; reconnect required",
                    );
                }
            }
            Ok(GameplayServerMessage::FogState(state)) => {
                if client.surface_fixture || state.revision != client.revision {
                    return;
                }
                super::map::fog::minimap(&client, &state);
                let was_disabled = client.fog.disabled();
                if client.fog.apply(state) {
                    client.rendered_frame.clear();
                    if was_disabled != client.fog.disabled() {
                        client.terrain_scene.borrow_mut().take();
                    }
                }
            }
            Ok(GameplayServerMessage::Projectiles {
                revision,
                tick,
                projectiles,
            }) => {
                if revision == client.revision && tick.0 >= client.village.projectile_tick {
                    let _ = client
                        .canvas
                        .set_attribute("data-projectiles", &projectiles.len().to_string());
                    client.village.projectile_tick = tick.0;
                    client.village.projectiles = projectiles;
                    client.rendered_frame.clear();
                }
            }
            Ok(GameplayServerMessage::WildlifeState {
                revision,
                tick,
                animals,
            }) => {
                if revision == client.revision && tick.0 >= client.village.wildlife_tick {
                    let _ = client
                        .canvas
                        .set_attribute("data-animals", &animals.len().to_string());
                    let _ = client.canvas.set_attribute(
                        "data-wolves",
                        &animals
                            .iter()
                            .filter(|a| a.kind == aoe_protocol::AnimalKind::Wolf)
                            .count()
                            .to_string(),
                    );
                    client.village.wildlife_tick = tick.0;
                    client.village.animals = animals;
                    let Client {
                        units,
                        selected,
                        village,
                        ..
                    } = &mut *client;
                    selected.units.retain(|id| {
                        units.contains_key(id) || village.animals.iter().any(|a| a.id == *id)
                    });
                }
            }
            Ok(GameplayServerMessage::RoleChange {
                role, resume_token, ..
            }) => {
                client.role = Some(role);
                client.token = resume_token;
                storage::save_token(resume_token);
            }
            Ok(GameplayServerMessage::WorldReset { .. }) => {
                client.order_feedback.clear();
                clear_world(&mut client);
                client.role = None;
                client.primary = None;
                if !client.surface_fixture {
                    client.map_content_hash = None;
                    client.focus_map_hash = None;
                }
                client.camera.focus_elevation_meters = 0.0;
                client.token = None;
                storage::save_token(None);
                controls::map_changed(None);
                client.status = "map changed; reconnecting".to_owned();
            }
            Ok(GameplayServerMessage::CommandAck {
                sequence, result, ..
            }) => {
                client.status = client
                    .order_feedback
                    .acknowledge(sequence, result)
                    .to_owned();
            }
            Ok(GameplayServerMessage::Error { message, .. }) => {
                client.status = format!("server error: {message}")
            }
            _ => {
                reconnect(&mut client, "protocol error; reconnect required");
            }
        }
    });
    socket.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
    onmessage.forget();

    let closed = shared.clone();
    let onclose = Closure::<dyn FnMut(Event)>::new(move |_| {
        let client = &mut *closed.borrow_mut();
        if client.connection_id == id {
            client.status = "reconnecting".to_owned();
            client.socket = None;
        }
    });
    socket.set_onclose(Some(onclose.as_ref().unchecked_ref()));
    onclose.forget();
    Ok(())
}

pub(super) fn merge_buildings(
    retained: &[aoe_protocol::GameplayBuildingState],
    changed: &[aoe_protocol::GameplayBuildingState],
    removals: &[aoe_core::EntityId],
) -> Option<Vec<aoe_protocol::GameplayBuildingState>> {
    let mut merged = retained.to_vec();
    merged.retain(|building| !removals.contains(&building.id));
    for building in changed {
        merged.retain(|existing| existing.id != building.id);
        merged.push(building.clone());
    }
    (merged.len() <= aoe_protocol::MAX_ECONOMY_BUILDINGS).then_some(merged)
}

fn reconnect(client: &mut Client, status: &str) {
    client.status = status.to_owned();
    client.resources.clear();
    if let Some(socket) = &client.socket {
        let _ = socket.close();
    }
}

// The pieces of a connection's world that a new Welcome or a WorldReset drops.
fn clear_world(client: &mut Client) {
    client.terrain_scene.get_mut().take();
    client.resources.clear();
    client.fog.clear();
    if !client.surface_fixture {
        map::clear_terrain_cache(client);
    }
    client.units.clear();
    client.village = Default::default();
    client.selected = Default::default();
    let tick_hz = client.config.tick_hz;
    client.presentation.reset(tick_hz);
    client.rendered_frame.clear();
}
