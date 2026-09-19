use aoe_core::{
    Camera, EntityId, FIXED_SUBUNITS_PER_TILE, ScreenPoint, TileRect, WorldConfig, WorldPosition,
};
use aoe_map::Chunk;
use aoe_protocol::{
    GAMEPLAY_VERSION, GameplayClientMessage, GameplayRole, GameplayServerMessage,
    GameplayUnitState, ResumeToken, decode_gameplay_server, encode_gameplay_client,
};
use aoe_rendering::{GameArt, GameRenderer, SceneCamera, SceneUnit};
use js_sys::Uint8Array;
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, VecDeque},
    rc::Rc,
};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use web_sys::{Document, Event, HtmlCanvasElement, MessageEvent, WebSocket};

#[path = "playground_controls.rs"]
mod controls;
#[path = "playground_init.rs"]
mod init;
#[path = "playground_map.rs"]
mod map;
#[path = "playground_status.rs"]
mod status;

#[derive(Clone, Copy)]
pub(super) struct Sample {
    pub tick: u64,
    pub position: WorldPosition,
}

pub(super) struct Drag {
    pub start: ScreenPoint,
    pub current: ScreenPoint,
    pub middle: bool,
    pub center: [f64; 2],
}

pub(super) struct Client {
    pub document: Document,
    pub canvas: HtmlCanvasElement,
    pub renderer: GameRenderer,
    pub art: GameArt,
    pub socket: Option<WebSocket>,
    pub connection_id: u64,
    pub camera: Camera,
    pub config: WorldConfig,
    pub primary: Option<EntityId>,
    pub role: Option<GameplayRole>,
    pub map_content_hash: Option<[u8; 32]>,
    pub terrain_chunks: BTreeMap<(i32, i32), Chunk>,
    pub terrain_inflight: BTreeSet<(i32, i32)>,
    pub token: Option<ResumeToken>,
    pub revision: u64,
    pub sent_region: Option<TileRect>,
    pub last_subscribe: f64,
    pub next_sequence: u64,
    pub units: BTreeMap<EntityId, GameplayUnitState>,
    pub history: BTreeMap<EntityId, VecDeque<Sample>>,
    pub server_tick: u64,
    pub selected: Option<EntityId>,
    pub grid: bool,
    pub drag: Option<Drag>,
    pub pointer: Option<ScreenPoint>,
    pub last_frame: f64,
    pub status: String,
}

pub(super) fn set_text(document: &Document, id: &str, value: &str) {
    if let Some(element) = document.get_element_by_id(id) {
        element.set_text_content(Some(value));
    }
}

pub(super) fn now() -> f64 {
    web_sys::window()
        .and_then(|window| window.performance())
        .map_or(0.0, |performance| performance.now())
}

fn token_hex(token: ResumeToken) -> String {
    token.0.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn parse_token(value: String) -> Option<ResumeToken> {
    if value.len() != 48 {
        return None;
    }
    let mut result = [0_u8; 24];
    for (index, byte) in result.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(ResumeToken(result))
}

fn stored_token() -> Option<ResumeToken> {
    let storage = web_sys::window()?.session_storage().ok()??;
    parse_token(storage.get_item("aoeworld.resume-token").ok()??)
}

pub(super) fn save_token(token: Option<ResumeToken>) {
    let Some(storage) = web_sys::window().and_then(|window| window.session_storage().ok()?) else {
        return;
    };
    match token {
        Some(token) => {
            let _ = storage.set_item("aoeworld.resume-token", &token_hex(token));
        }
        None => {
            let _ = storage.remove_item("aoeworld.resume-token");
        }
    }
}

pub(super) fn dpr() -> f64 {
    web_sys::window().map_or(1.0, |window| window.device_pixel_ratio().clamp(1.0, 2.0))
}

pub(super) fn resize(client: &mut Client) {
    let width = client.canvas.client_width().max(1) as f64;
    let height = client.canvas.client_height().max(1) as f64;
    let scale = dpr();
    let backing = [
        (width * scale).round() as u32,
        (height * scale).round() as u32,
    ];
    if client.canvas.width() != backing[0] {
        client.canvas.set_width(backing[0]);
    }
    if client.canvas.height() != backing[1] {
        client.canvas.set_height(backing[1]);
    }
    client.camera.viewport = [f64::from(backing[0]), f64::from(backing[1])];
    client.renderer.resize(backing[0], backing[1]);
    client.camera = client.camera.clamp_center(client.config);
}

fn requested_region(client: &Client) -> TileRect {
    let visible = client.camera.visible_tiles(client.config, 8.0);
    let width = visible.width().clamp(1, 512);
    let height = visible.height().clamp(1, 512);
    TileRect::from_xywh(
        visible.min.x + (visible.width() - width) / 2,
        visible.min.y + (visible.height() - height) / 2,
        width,
        height,
    )
    .clamp(client.config.width_tiles, client.config.height_tiles)
}

pub(super) fn subscribe(client: &mut Client) {
    if client.status != "connected"
        || client
            .socket
            .as_ref()
            .is_none_or(|socket| socket.ready_state() != WebSocket::OPEN)
    {
        return;
    }
    let region = requested_region(client);
    if client.sent_region == Some(region) || now() - client.last_subscribe < 100.0 {
        return;
    }
    client.revision = client.revision.saturating_add(1);
    let message = GameplayClientMessage::Subscribe {
        revision: client.revision,
        region,
    };
    if let Ok(bytes) = encode_gameplay_client(&message)
        && client
            .socket
            .as_ref()
            .is_some_and(|socket| socket.send_with_u8_array(&bytes).is_ok())
    {
        client.sent_region = Some(region);
        client.last_subscribe = now();
    }
}

pub(super) fn position_at(client: &Client, id: EntityId) -> [f64; 2] {
    let Some(state) = client.units.get(&id) else {
        return [0.0, 0.0];
    };
    let Some(history) = client.history.get(&id) else {
        return state.position.as_tiles();
    };
    let target = client.server_tick.saturating_sub(2);
    let Some(next) = history.iter().find(|sample| sample.tick >= target).copied() else {
        return state.position.as_tiles();
    };
    let previous = history
        .iter()
        .rev()
        .find(|sample| sample.tick <= target)
        .copied()
        .unwrap_or(next);
    if next.tick == previous.tick {
        return next.position.as_tiles();
    }
    let amount = (target - previous.tick) as f64 / (next.tick - previous.tick) as f64;
    let start = previous.position.as_tiles();
    let end = next.position.as_tiles();
    [
        start[0] + (end[0] - start[0]) * amount,
        start[1] + (end[1] - start[1]) * amount,
    ]
}

pub(super) fn remember(client: &mut Client, state: GameplayUnitState, tick: u64) {
    client.units.insert(state.id, state);
    let history = client.history.entry(state.id).or_default();
    history.push_back(Sample {
        tick,
        position: state.position,
    });
    while history.len() > 8 {
        history.pop_front();
    }
}

fn connect(shared: Rc<RefCell<Client>>) -> Result<(), JsValue> {
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
                ..
            }) => {
                client.config.width_tiles = width_tiles;
                client.config.height_tiles = height_tiles;
                client.role = Some(role);
                client.map_content_hash = map_content_hash;
                client.terrain_chunks.clear();
                client.terrain_inflight.clear();
                client.primary = Some(primary_unit_id);
                client.token = resume_token;
                save_token(resume_token);
                client.units.clear();
                client.history.clear();
                client.status = "connected".to_owned();
                resize(&mut client);
                subscribe(&mut client);
            }
            Ok(GameplayServerMessage::Snapshot {
                revision,
                tick,
                units,
            }) if revision >= client.revision => {
                client.revision = revision;
                client.server_tick = tick.0;
                client.units.clear();
                client.history.clear();
                for unit in units {
                    remember(&mut client, unit, tick.0);
                }
            }
            Ok(GameplayServerMessage::Tick {
                revision,
                tick,
                changed_units,
                removals,
            }) if revision == client.revision => {
                client.server_tick = client.server_tick.max(tick.0);
                for id in removals {
                    client.units.remove(&id);
                    client.history.remove(&id);
                }
                for unit in changed_units {
                    remember(&mut client, unit, tick.0);
                }
            }
            Ok(GameplayServerMessage::RoleChange {
                role, resume_token, ..
            }) => {
                client.role = Some(role);
                client.token = resume_token;
                save_token(resume_token);
            }
            Ok(GameplayServerMessage::WorldReset { .. }) => {
                client.units.clear();
                client.history.clear();
                client.role = None;
                client.primary = None;
                client.map_content_hash = None;
                client.terrain_chunks.clear();
                client.terrain_inflight.clear();
                client.token = None;
                save_token(None);
                client.status = "map changed; reconnecting".to_owned();
            }
            Ok(GameplayServerMessage::CommandAck { result, .. }) => {
                client.status = match result {
                    aoe_protocol::CommandResult::Accepted => "connected",
                    aoe_protocol::CommandResult::RejectedUnreachable => "order unreachable",
                    aoe_protocol::CommandResult::RejectedPathBudgetExceeded => {
                        "path planning limit reached"
                    }
                    _ => "order rejected",
                }
                .to_owned();
            }
            Ok(GameplayServerMessage::Error { message, .. }) => {
                client.status = format!("server error: {message}")
            }
            _ => client.status = "protocol error".to_owned(),
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

pub(super) fn reconnect(shared: Rc<RefCell<Client>>) {
    let _ = connect(shared);
}

pub(super) fn center_on_primary(client: &mut Client) {
    if let Some(primary) = client.primary {
        client.camera.center = position_at(client, primary);
    }
    client.camera = client.camera.clamp_center(client.config);
}

pub(super) fn send_order(client: &mut Client, point: ScreenPoint) {
    if client.role != Some(GameplayRole::Controller) || client.selected != client.primary {
        return;
    }
    let world = client.camera.screen_to_world(point);
    if world[0] < 0.0
        || world[1] < 0.0
        || world[0] >= f64::from(client.config.width_tiles)
        || world[1] >= f64::from(client.config.height_tiles)
    {
        return;
    }
    let destination = WorldPosition::from_i64(
        (world[0] * f64::from(FIXED_SUBUNITS_PER_TILE)).round() as i64,
        (world[1] * f64::from(FIXED_SUBUNITS_PER_TILE)).round() as i64,
    );
    let Ok(destination) = destination else { return };
    let destination = client.config.snap_ground_position(destination);
    let message = GameplayClientMessage::MoveOrder {
        sequence: client.next_sequence,
        entity_id: client.primary.unwrap_or(EntityId(0)),
        destination,
    };
    client.next_sequence = client.next_sequence.saturating_add(1);
    if let Ok(bytes) = encode_gameplay_client(&message) {
        let _ = client
            .socket
            .as_ref()
            .and_then(|socket| socket.send_with_u8_array(&bytes).ok());
    }
}

fn animate(shared: Rc<RefCell<Client>>) -> Result<(), JsValue> {
    let callback = Rc::new(RefCell::new(None::<Closure<dyn FnMut(f64)>>));
    let next = callback.clone();
    *callback.borrow_mut() = Some(Closure::new(move |time: f64| {
        let mut client = shared.borrow_mut();
        resize(&mut client);
        let delta_ms = if client.last_frame == 0.0 {
            16.0
        } else {
            (time - client.last_frame).clamp(0.0, 100.0)
        };
        client.last_frame = time;
        controls::edge_pan(&mut client, delta_ms);
        subscribe(&mut client);
        let units = client
            .units
            .values()
            .map(|unit| SceneUnit {
                id: unit.id,
                position: position_at(&client, unit.id),
                moving: unit.moving,
                facing: unit.facing,
                selected: client.selected == Some(unit.id),
            })
            .collect::<Vec<_>>();
        let camera = SceneCamera {
            center: client.camera.center,
            zoom: client.camera.zoom,
            viewport: client.camera.viewport,
        };
        let terrain = map::scene_terrain(&client);
        let resources = map::scene_resources(&client);
        let grid = client.grid;
        let animation = (time / 100.0) as usize;
        let Client { renderer, art, .. } = &mut *client;
        if let Err(error) =
            renderer.render_world(art, &terrain, &resources, &units, camera, animation, grid)
        {
            client.status = error;
        }
        set_text(&client.document, "connection", &client.status);
        set_text(&client.document, "unit-state", &status::label(&client));
        set_text(
            &client.document,
            "terrain-cache",
            &map::cache_status(&client),
        );
        set_text(
            &client.document,
            "world-position",
            &format!(
                "{:.1}, {:.1}",
                client.camera.center[0], client.camera.center[1]
            ),
        );
        drop(client);
        map::request_visible(shared.clone());
        if let Some(window) = web_sys::window()
            && let Some(cb) = next.borrow().as_ref()
        {
            let _ = window.request_animation_frame(cb.as_ref().unchecked_ref());
        }
    }));
    web_sys::window()
        .ok_or("No window")?
        .request_animation_frame(
            callback
                .borrow()
                .as_ref()
                .ok_or("No animation callback")?
                .as_ref()
                .unchecked_ref(),
        )?;
    Ok(())
}

pub async fn initialize(document: Document) -> Result<(), JsValue> {
    init::initialize(document).await
}
