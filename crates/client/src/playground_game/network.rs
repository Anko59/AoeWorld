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
                client.resources.clear();
                if !client.surface_fixture {
                    map::clear_terrain_cache(&mut client);
                }
                client.primary = Some(primary_unit_id);
                client.token = resume_token;
                storage::save_token(resume_token);
                client.units.clear();
                let focused = client.village.focused && !map_changed;
                client.village = Default::default();
                client.village.focused = focused;
                client.selected = Default::default();
                let tick_hz = client.config.tick_hz;
                client.presentation.reset(tick_hz);
                client.rendered_frame.clear();
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
                client.village.buildings = buildings;
                client.village.tick = tick.0;
                if !client.village.focused {
                    client.village.focused = true;
                    if !capture::current().has_camera() {
                        village::orders::focus_town_center(&mut client, false);
                    }
                }
                let live = client.units.keys().copied().collect::<Vec<_>>();
                client.selected.units.retain(|id| live.contains(id));
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
                if !client.presentation.receive(tick.0, now()) {
                    return;
                }
                client.village.tick = tick.0;
                for player in changed_players {
                    client.village.players.retain(|p| p.player != player.player);
                    client.village.players.push(player);
                }
                for id in building_removals {
                    client.village.buildings.retain(|b| b.id != id);
                }
                for building in changed_buildings {
                    client.village.buildings.retain(|b| b.id != building.id);
                    client.village.buildings.push(building);
                }
                for id in removals {
                    client.units.remove(&id);
                    client.selected.units.retain(|selected| *selected != id);
                    client.village.jobs.remove(&id);
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
                    client.status = "resource state out of sync; reconnect required".to_owned();
                    client.resources.clear();
                    if let Some(socket) = &client.socket {
                        let _ = socket.close();
                    }
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
                client.units.clear();
                client.village = Default::default();
                client.selected = Default::default();
                let tick_hz = client.config.tick_hz;
                client.presentation.reset(tick_hz);
                client.rendered_frame.clear();
                client.role = None;
                client.primary = None;
                if !client.surface_fixture {
                    client.map_content_hash = None;
                    client.focus_map_hash = None;
                }
                client.camera.focus_elevation_meters = 0.0;
                client.resources.clear();
                if !client.surface_fixture {
                    map::clear_terrain_cache(&mut client);
                }
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
                client.status = "protocol error; reconnect required".to_owned();
                client.resources.clear();
                if let Some(socket) = &client.socket {
                    let _ = socket.close();
                }
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
