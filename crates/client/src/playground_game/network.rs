use super::*;
use aoe_protocol::{GameplayServerMessage, decode_gameplay_server};
use js_sys::Uint8Array;
use web_sys::{Event, MessageEvent};

#[path = "network/tests.rs"]
#[cfg(test)]
mod tests;

#[path = "network/lifecycle.rs"]
mod lifecycle;
use lifecycle::{clear_world, reconnect};

pub(super) struct ConnectionCallbacks {
    socket: WebSocket,
    _onopen: Closure<dyn FnMut(Event)>,
    _onmessage: Closure<dyn FnMut(MessageEvent)>,
    _onclose: Closure<dyn FnMut(Event)>,
}

impl Drop for ConnectionCallbacks {
    fn drop(&mut self) {
        self.socket.set_onopen(None);
        self.socket.set_onmessage(None);
        self.socket.set_onclose(None);
        let _ = self.socket.close();
    }
}

fn incoming_bytes(data: &JsValue) -> Result<Vec<u8>, &'static str> {
    let buffer = data
        .dyn_ref::<js_sys::ArrayBuffer>()
        .ok_or("non-binary game frame; reconnect required")?;
    if buffer.byte_length() as usize > aoe_protocol::GAMEPLAY_MAX_MESSAGE {
        return Err("protocol frame too large; reconnect required");
    }
    Ok(Uint8Array::new(buffer).to_vec())
}

fn send_bytes(socket: &WebSocket, bytes: &[u8]) -> Result<(), &'static str> {
    const BUDGET: usize = 1024 * 1024;
    if (socket.buffered_amount() as usize).saturating_add(bytes.len()) > BUDGET {
        let _ = socket.close();
        return Err("connection stalled; reconnect required");
    }
    if socket.ready_state() != WebSocket::OPEN {
        return Err("connection closed; reconnect required");
    }
    socket
        .send_with_u8_array(bytes)
        .map_err(|_| "send failed; reconnect required")
}

pub(super) fn send(client: &mut Client, bytes: &[u8]) -> bool {
    let Some(socket) = client.socket.clone() else {
        return false;
    };
    match send_bytes(&socket, bytes) {
        Ok(()) => true,
        Err(message) => {
            reconnect(client, message);
            false
        }
    }
}

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
        client.connection_callbacks.take();
        client.connection_id = client.connection_id.saturating_add(1);
        if let Some(old) = client.socket.replace(socket.clone()) {
            let _ = old.close();
        }
        client.status = "connecting".to_owned();
        client.order_feedback.clear();
        client.village.pending_training.clear();
        client.sent_region = None;
        client.connection_id
    };
    let opened = Rc::downgrade(&shared);
    let onopen = Closure::<dyn FnMut(Event)>::new(move |_| {
        let Some(opened) = opened.upgrade() else {
            return;
        };
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
            let _ = send(client, &bytes);
        }
    });
    socket.set_onopen(Some(onopen.as_ref().unchecked_ref()));

    let received = Rc::downgrade(&shared);
    let onmessage = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
        let Some(received) = received.upgrade() else {
            return;
        };
        let mut client = received.borrow_mut();
        if client.connection_id != id {
            return;
        }
        let _decode = super::perf::Scope::new(1);
        let bytes = match incoming_bytes(&event.data()) {
            Ok(bytes) => bytes,
            Err(message) => {
                reconnect(&mut client, message);
                return;
            }
        };
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
            Ok(GameplayServerMessage::MoveQueues {
                revision,
                tick,
                queues,
            }) => {
                if revision == client.revision && tick.0 >= client.village.tick {
                    client.village.move_queues = queues;
                }
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
                client.last_error = None;
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
                client.village.move_queues.clear();
                client.village.players = players;
                client.village.idle_villagers.clear();
                client.village.idle_military.clear();
                let owner = village::owner(&client);
                let Client { units, village, .. } = &mut *client;
                for unit in units.values() {
                    village.remember_idle(owner, unit);
                }
                client.village.rubble.retain(|(_, until)| *until > tick.0);
                map::scene::replace_buildings(&mut client, buildings);
                client.village.tick = tick.0;
                client.village.pending_training.observe(tick.0);
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
                client.village.pending_training.observe(tick.0);
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
                    client.village.idle_villagers.remove(id);
                    client.village.idle_military.remove(id);
                    client.units.remove(&id);
                    client.selected.units.retain(|selected| *selected != id);
                    client.presentation.remove(id);
                }
                for unit in &changed_units {
                    if !remember(&mut client, unit, tick.0) {
                        reconnect(&mut client, "unit table full; reconnect required");
                        return;
                    }
                }
            }
            Ok(GameplayServerMessage::Tick { .. }) => {}
            Ok(GameplayServerMessage::ResourceState(state)) => {
                if state.subscription_revision == client.revision
                    && !client.resources.apply_at(state, super::now())
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
                client.village.pending_training.clear();
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
                sequence,
                result,
                applied_tick,
            }) => {
                let observed = client.village.tick;
                client.village.pending_training.acknowledge(
                    sequence,
                    result,
                    applied_tick.0,
                    observed,
                );
                client.status = client
                    .order_feedback
                    .acknowledge(sequence, result)
                    .to_owned();
                if result != aoe_protocol::CommandResult::Accepted {
                    client.last_error = Some(client.status.clone());
                }
            }
            Ok(GameplayServerMessage::Error { message, .. }) => {
                client.status = format!("server error: {message}");
                client.last_error = Some(client.status.clone());
            }
            _ => {
                reconnect(&mut client, "protocol error; reconnect required");
            }
        }
    });
    socket.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));

    let closed = Rc::downgrade(&shared);
    let onclose = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
        let Some(closed) = closed.upgrade() else {
            return;
        };
        let client = &mut *closed.borrow_mut();
        if client.connection_id == id {
            lifecycle::record_close(client, &event);
            client.status = "reconnecting".to_owned();
            client.socket = None;
        }
    });
    socket.set_onclose(Some(onclose.as_ref().unchecked_ref()));
    shared.borrow_mut().connection_callbacks = Some(ConnectionCallbacks {
        socket,
        _onopen: onopen,
        _onmessage: onmessage,
        _onclose: onclose,
    });
    Ok(())
}

pub(super) fn merge_buildings(
    retained: &[aoe_protocol::GameplayBuildingState],
    changed: &[aoe_protocol::GameplayBuildingState],
    removals: &[aoe_core::EntityId],
) -> Option<Vec<aoe_protocol::GameplayBuildingState>> {
    let mut merged = retained
        .iter()
        .map(|b| (b.id, b))
        .collect::<BTreeMap<_, _>>();
    for id in removals {
        merged.remove(id);
    }
    for b in changed {
        merged.insert(b.id, b);
    }
    (merged.len() <= aoe_protocol::MAX_RETAINED_BUILDINGS)
        .then(|| merged.into_values().cloned().collect())
}
