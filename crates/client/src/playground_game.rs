use aoe_core::{Camera, EntityId, ScreenPoint, TileRect, WorldConfig};
use aoe_protocol::{
    GAMEPLAY_VERSION, GameplayClientMessage, GameplayRole, GameplayUnitState, ResumeToken,
    encode_gameplay_client,
};
use aoe_rendering::{GameArt, GameRenderer, SceneCamera};
use map::CachedChunk;
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use web_sys::{Document, HtmlCanvasElement, WebSocket};

#[path = "playground_controls/capture.rs"]
mod capture;
#[path = "playground_controls.rs"]
mod controls;
#[path = "playground_fixture.rs"]
mod fixture;
#[path = "playground_init.rs"]
mod init;
#[path = "playground_map.rs"]
mod map;
use map::{state::Drag, ui::set_text};
#[path = "playground_game/network.rs"]
mod network;
#[path = "playground_status.rs"]
mod status;
#[path = "playground_storage.rs"]
mod storage;
#[path = "village/mod.rs"]
mod village;
use network::connect;

pub(super) struct Client {
    pub document: Document,
    pub canvas: HtmlCanvasElement,
    pub renderer: GameRenderer,
    pub art: GameArt,
    pub socket: Option<WebSocket>,
    pub connection_id: u64,
    pub camera: Camera,
    pub capture: capture::Capture,
    pub config: WorldConfig,
    pub primary: Option<EntityId>,
    pub role: Option<GameplayRole>,
    pub map_content_hash: Option<[u8; 32]>,
    pub surface_fixture: bool,
    pub focus_map_hash: Option<[u8; 32]>,
    pub resources: crate::resource_state::ResourceStateCache,
    pub terrain_scene: RefCell<Option<Rc<map::scene::PreparedScene>>>,
    pub rendered_frame: map::render_cache::RenderFrameCache<map::scene::PreparedScene>,
    pub terrain_chunks: BTreeMap<(i32, i32), CachedChunk>,
    pub terrain_height_bounds: Option<(i16, i16)>,
    pub terrain_bounds: map::heights::MapHeightBounds,
    pub terrain_resident_height_bounds: Option<(i16, i16)>,
    pub terrain_requests: crate::chunk_requests::RequestWindow<Option<web_sys::AbortController>>,
    pub token: Option<ResumeToken>,
    pub revision: u64,
    pub sent_region: Option<TileRect>,
    pub last_subscribe: f64,
    pub next_sequence: u64,
    pub order_feedback: map::feedback::OrderFeedback,
    pub units: BTreeMap<EntityId, GameplayUnitState>,
    pub presentation: map::presentation::Presentation,
    pub selected: village::selection::Selection,
    pub village: village::Village,
    pub grid: bool,
    pub drag: Option<Drag>,
    pub pointer: Option<ScreenPoint>,
    pub last_frame: f64,
    pub status: String,
}

pub(super) fn now() -> f64 {
    web_sys::window()
        .and_then(|window| window.performance())
        .map_or(0.0, |performance| performance.now())
}

pub(super) fn resize(client: &mut Client) {
    let width = client.canvas.client_width().max(1) as f64;
    let height = client.canvas.client_height().max(1) as f64;
    let ratio = web_sys::window().map_or(1.0, |window| window.device_pixel_ratio());
    let backing = controls::backing_size(width, height, ratio);
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
    let visible = map::terrain_visible_tiles(client);
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
    if client.role.is_none()
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
    client
        .rendered_frame
        .position(id)
        .unwrap_or_else(|| client.presentation.position(*state))
}

pub(super) fn remember(client: &mut Client, state: &GameplayUnitState, tick: u64) {
    let Client {
        units,
        presentation,
        ..
    } = client;
    presentation.remember(units.get(&state.id), state, tick);
    units.insert(state.id, *state);
}

pub(super) fn reconnect(shared: Rc<RefCell<Client>>) {
    let _ = connect(shared);
}

pub(super) fn center_on_primary(client: &mut Client) {
    if let Some(primary) = client.primary {
        map::center_on_world(client, position_at(client, primary));
    }
    client.camera = client.camera.clamp_center(client.config);
}

pub(super) fn send_order(client: &mut Client, point: ScreenPoint) {
    village::orders::at_pointer(client, point);
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
        let canvas = matches!(&client.renderer, GameRenderer::Canvas { .. });
        client.presentation.advance(time, canvas);
        let scene = map::scene::prepare(&client);
        let units = map::scene::units(&client, &scene);
        let camera = SceneCamera {
            center: client.camera.center,
            zoom: client.camera.zoom,
            viewport: client.camera.viewport,
            focus_elevation_meters: client.camera.focus_elevation_meters,
        };
        let resources = map::scene_resources(&client);
        let grid = client.grid;
        let animation = (time / 100.0) as usize;
        if let Err(error) = map::render_cache::render_if_changed(
            &mut client,
            &scene,
            &units,
            &resources,
            camera,
            animation,
            grid,
        ) {
            client.status = error;
        }
        if time - client.village.last_hud >= 250.0 {
            client.village.last_hud = time;
            status::update(&client);
            village::hud::update(&client);
        }
        crate::web::numeric::set_coordinates(
            &client.document,
            client.camera.center[0],
            client.camera.center[1],
        );
        capture::update(&mut client);
        let surface_fixture = client.surface_fixture;
        drop(client);
        if !surface_fixture {
            map::request_visible(shared.clone());
        }
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

#[cfg(test)]
#[path = "village/tests/orders.rs"]
mod village_order_tests;
