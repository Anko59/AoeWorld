use crate::camera_demand::{self, PanMotion};
use aoe_core::{Camera, EntityId, ScreenPoint, TileRect, WorldConfig};
use aoe_protocol::{
    GAMEPLAY_VERSION, GameplayClientMessage, GameplayUnitState, ResumeToken, encode_gameplay_client,
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
#[path = "playground_game/memory.rs"]
mod memory;
#[path = "playground_game/network.rs"]
mod network;
#[path = "playground_game/perf.rs"]
mod perf;
#[path = "playground_game/subscription.rs"]
mod subscription;
use subscription::{retry_refused_view, subscribe, subscribed_to_view, subscription_due};
#[path = "playground_game/recovery.rs"]
mod recovery;
#[path = "playground_status.rs"]
mod status;
#[path = "playground_storage.rs"]
mod storage;
#[path = "playground_game/strategic.rs"]
pub(super) mod strategic;
#[path = "village/mod.rs"]
mod village;
use network::connect;

pub(super) struct Client {
    pub document: Document,
    pub canvas: HtmlCanvasElement,
    pub renderer: GameRenderer,
    pub art: GameArt,
    pub native: Option<crate::game_assets::NativeFrames>,
    pub team_colour: [u8; 3],
    pub socket: Option<WebSocket>,
    connection_callbacks: Option<network::ConnectionCallbacks>,
    pub connection_id: u64,
    pub camera: Camera,
    pub capture: capture::Capture,
    pub config: WorldConfig,
    pub primary: Option<EntityId>,
    pub map_content_hash: Option<[u8; 32]>,
    pub surface_fixture: bool,
    pub focus_map_hash: Option<[u8; 32]>,
    pub resources: crate::resource_state::ResourceStateCache,
    pub fog: crate::fog::Fog,
    pub terrain_scene: RefCell<Option<Rc<map::scene::PreparedScene>>>,
    terrain_patches: map::scene::PatchCache,
    pub pan_motion: PanMotion,
    pub terrain_ahead: [f64; 2],
    pub rendered_frame: map::render_cache::RenderFrameCache<map::scene::PreparedScene>,
    pub terrain_chunks: BTreeMap<(i32, i32), CachedChunk>,
    pub terrain_height_bounds: Option<(i16, i16)>,
    pub terrain_bounds: map::heights::MapHeightBounds,
    pub terrain_resident_height_bounds: Option<(i16, i16)>,
    pub terrain_requests: crate::chunk_requests::RequestWindow<Option<web_sys::AbortController>>,
    pub token: Option<ResumeToken>,
    pub revision: u64,
    pub sent_region: Option<TileRect>,
    /// Whether the last sent view drew single units (`Subscribe::unit_zoom`).
    pub sent_unit_zoom: bool,
    pub subscribe_after: f64,
    pub next_sequence: u64,
    pub order_feedback: map::feedback::OrderFeedback,
    pub units: BTreeMap<EntityId, GameplayUnitState>,
    pub presentation: map::presentation::Presentation,
    pub selected: village::selection::Selection,
    pub village: village::Village,
    pub grid: bool,
    pub hover_mode: u8,
    pub drag: Option<Drag>,
    pub pointer: Option<ScreenPoint>,
    pub last_frame: f64,
    pub status: String,
    /// Kept across reconnect attempts until a new Welcome or user order.
    pub last_error: Option<String>,
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
    let viewport = [f64::from(backing[0]), f64::from(backing[1])];
    let changed = client.canvas.width() != backing[0]
        || client.canvas.height() != backing[1]
        || client.camera.viewport != viewport;
    if client.canvas.width() != backing[0] {
        client.canvas.set_width(backing[0]);
    }
    if client.canvas.height() != backing[1] {
        client.canvas.set_height(backing[1]);
    }
    client.camera.viewport = viewport;
    if changed {
        client.renderer.resize(backing[0], backing[1]);
    }
}

pub(super) fn position_at(client: &Client, id: EntityId) -> [f64; 2] {
    let Some(state) = client.units.get(&id) else {
        return [0.0, 0.0];
    };
    position_of(client, state)
}

/// A known state needs no second lookup to reuse its displayed world pose.
pub(super) fn position_of(client: &Client, state: &GameplayUnitState) -> [f64; 2] {
    client
        .rendered_frame
        .position(state.id)
        .unwrap_or_else(|| client.presentation.position(*state))
}

pub(super) fn remember(client: &mut Client, state: &GameplayUnitState, tick: u64) -> bool {
    if client.units.len() >= aoe_protocol::MAX_SUBSCRIBED_UNITS
        && !client.units.contains_key(&state.id)
    {
        return false;
    }
    let owner = village::owner(client);
    client.village.remember_idle(owner, state);
    let Client {
        units,
        presentation,
        ..
    } = client;
    presentation.remember(units.get(&state.id), state, tick);
    units.insert(state.id, *state);
    true
}

pub(super) fn reconnect(shared: Rc<RefCell<Client>>) {
    let _ = connect(shared);
}

pub(super) fn center_on_primary(client: &mut Client) {
    if let Some(primary) = client.primary {
        map::center_on_world(client, position_at(client, primary));
    }
}

pub(super) fn send_order(client: &mut Client, point: ScreenPoint, queued: bool) {
    village::orders::at_pointer(client, point, queued);
}

fn animate(shared: Rc<RefCell<Client>>) -> Result<(), JsValue> {
    let callback = Rc::new(RefCell::new(None::<Closure<dyn FnMut(f64)>>));
    let next = callback.clone();
    let subscription_client = Rc::downgrade(&shared);
    let subscription = Closure::<dyn FnMut()>::new(move || {
        if let Some(client) = subscription_client.upgrade() {
            subscribe(&mut client.borrow_mut());
        }
    });
    *callback.borrow_mut() = Some(Closure::new(move |time: f64| {
        perf::perf_mark(0);
        #[cfg(feature = "frame-bench")]
        let _frame = perf::Scope::new(41);
        let mut client = shared.borrow_mut();
        if let Err(error) = recovery::check(&client.renderer) {
            match recovery::restart(&client.renderer, Some(client.camera), &error) {
                Ok(true) => return,
                _ => client.status = error,
            }
        }
        let update = perf::Scope::stage(0);
        resize(&mut client);
        let delta_ms = if client.last_frame == 0.0 {
            16.0
        } else {
            (time - client.last_frame).clamp(0.0, 100.0)
        };
        client.last_frame = time;
        client.resources.expire_effects(time);
        controls::edge_pan(&mut client, delta_ms);
        let center = client.camera.center;
        client.terrain_ahead = client.pan_motion.observe(center, time);
        let terrain = perf::Scope::stage(3);
        map::scene::advance_terrain(&mut client);
        drop(terrain);
        client.capture.current_view = false;
        let canvas = matches!(&client.renderer, GameRenderer::Canvas { .. });
        drop(update);
        let interpolation = perf::Scope::stage(1);
        client.presentation.advance(time, canvas);
        drop(interpolation);
        // Keep input, networking and the HUD live while the overview covers the scene.
        let draw = perf::Scope::new(2);
        let mut native_requests = Vec::new();
        let backlogged =
            matches!(&client.renderer, GameRenderer::WebGl(gl) if gl.frame_backlogged());
        let scene = if !controls::navigation::overview_active()
            && !strategic::render(&mut client)
            && !backlogged
        {
            let _terrain = perf::Scope::stage(3);
            map::scene::prepare_frame(&client)
        } else {
            None
        };
        if let Some(scene) = scene {
            let live = client
                .presentation
                .live_tick(time)
                .unwrap_or(client.village.tick as f64);
            let clocks = client
                .village
                .rubble
                .iter()
                .map(|(b, expiry)| {
                    let born = expiry.saturating_sub(map::scene::rubble_ticks(&client, b));
                    (
                        b.id.0,
                        (live - born as f64).max(0.0) * 1000.0
                            / f64::from(client.config.tick_hz.max(1)),
                    )
                })
                .collect();
            client.art.building_clocks = clocks;
            let interpolation = perf::Scope::stage(1);
            let units = map::scene::units(&client, &scene);
            drop(interpolation);
            let culling = perf::Scope::stage(2);
            let camera = SceneCamera {
                center: client.camera.center,
                zoom: client.camera.zoom,
                viewport: client.camera.viewport,
                focus_elevation_meters: client.camera.focus_elevation_meters,
            };
            let resources = map::scene_resources_for(&client, &scene);
            let mut required =
                aoe_rendering::scene_native_frames(&client.art, &units, camera, time as usize);
            #[cfg(feature = "frame-bench")]
            let terrain_plan = perf::Scope::stage(3);
            required.extend(scene.terrain_plan(&mut client.art).required());
            #[cfg(feature = "frame-bench")]
            drop(terrain_plan);
            required.extend(aoe_rendering::scene_resource_native_frames(
                &client.art,
                &resources,
                camera,
                time as usize,
            ));
            let substitutes = aoe_rendering::scene_native_substitutes(
                &client.art,
                &units,
                &resources,
                camera,
                time as usize,
            );
            let coverage = map::render_cache::NativeCoverage::new(&client.art, &substitutes);
            coverage.prioritize(&mut required);
            // Keep cold startup requests for visible pixels. Future animation
            // poses can stream after the first populated scene is presented.
            let prefetch = if client.rendered_frame.prefetch_ready() {
                aoe_rendering::scene_native_prefetch(&client.art, &units, camera, time as usize)
            } else {
                Vec::new()
            };
            drop(culling);
            #[cfg(feature = "frame-bench")]
            let streaming_detail = perf::Scope::new(55);
            let streaming = perf::Scope::stage(0);
            let Client {
                native,
                art,
                renderer,
                rendered_frame,
                ..
            } = &mut *client;
            if let Some(native) = native {
                match native.update(&required, &prefetch, &coverage.keep, art, renderer) {
                    Ok((changed, requests)) => {
                        native_requests = requests;
                        if changed {
                            rendered_frame.invalidate();
                        }
                    }
                    Err(error) => client.status = format!("Native art unavailable: {error:?}"),
                }
            }
            drop(streaming);
            #[cfg(feature = "frame-bench")]
            drop(streaming_detail);
            let grid = client.grid;
            let animation = time as usize;
            let result = if coverage.ready(&client.art, &required)
                && client.native.as_ref().is_none_or(|native| native.ready())
            {
                map::render_cache::render_if_changed(
                    &mut client,
                    &scene,
                    &units,
                    &resources,
                    camera,
                    animation,
                    grid,
                )
            } else if client.selected.units.is_empty() && client.selected.building_id.is_none() {
                map::render_cache::repaint_cleared_selection(&mut client)
            } else {
                Ok(false)
            };
            match result {
                Ok(true) => {
                    let substitutes = aoe_rendering::scene_native_substitutes(
                        &client.art,
                        &units,
                        &resources,
                        camera,
                        animation,
                    );
                    let mut used =
                        map::render_cache::NativeCoverage::new(&client.art, &substitutes).keep;
                    used.extend(required.iter().copied().filter(|&id| {
                        client
                            .art
                            .resident
                            .get(id as usize - 1)
                            .is_some_and(Option::is_some)
                    }));
                    if let Some(native) = &mut client.native {
                        native.pin_presented(&used);
                    }
                    strategic::present();
                }
                Ok(false) => {}
                Err(error) => {
                    if matches!(
                        recovery::restart(&client.renderer, Some(client.camera), &error),
                        Ok(true)
                    ) {
                        return;
                    }
                    client.status = error;
                }
            }
        }
        drop(draw);
        perf::contour(client.renderer.contour_stats());
        let ui_stage = perf::Scope::stage(5);
        let ui = perf::Scope::new(3);
        controls::navigation::present(&client);
        village::waypoints::present(&client);
        if time - client.village.last_hud >= 250.0 {
            controls::groups::update(&client);
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
        drop(ui);
        drop(ui_stage);
        let surface_fixture = client.surface_fixture;
        if subscription_due(&client)
            && let Some(window) = web_sys::window()
        {
            // A view change is sent in its own event-loop turn. The WebSocket
            // response can apply only after this animation callback returns.
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                subscription.as_ref().unchecked_ref(),
                0,
            );
        }
        drop(client);
        for request in native_requests {
            request.spawn();
        }
        if !surface_fixture {
            map::request_visible(shared.clone());
        }
        perf::perf_mark(21);
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
