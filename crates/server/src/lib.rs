//! HTTP and bounded WebSocket adapter for the synthetic world.
mod config;
mod gameplay;
mod gameplay_map;
mod gameplay_sessions;
mod gameplay_transport;
mod map_jobs;
mod map_store;
mod map_worker;
mod maps;
mod terrain_cache;
pub use config::Config;
pub use gameplay::GameplayService;
pub use map_store::MapStoreError;

use aoe_core::{EntityId, Region, Tick};
use aoe_map::MapPackage;
use aoe_protocol::{
    ClientMessage, EntityState, MAX_ENTITIES, ServerMessage, VERSION, decode_client, encode_server,
};
use aoe_scenario::Scenario;
use aoe_simulation::{Entity, GameWorldError, World};
use axum::{
    Json, Router,
    extract::{
        Path, Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{
    sync::{Mutex, RwLock},
    time::{MissedTickBehavior, timeout},
};
use tower_http::services::ServeDir;

#[derive(Clone)]
pub struct AppState {
    world: Arc<RwLock<Option<World>>>,
    diagnostic_scenario: Scenario,
    generation: Arc<AtomicU64>,
    tick_deadline_misses: Arc<AtomicU64>,
    build: Arc<str>,
    tick_period: Duration,
    asset_pack: Option<PathBuf>,
    map_package_directory: Option<PathBuf>,
    map_worker: Option<PathBuf>,
    geodata_cache_directory: PathBuf,
    map_packages: Arc<RwLock<BTreeMap<String, MapPackage>>>,
    terrain_cache: Arc<Mutex<terrain_cache::TerrainCache>>,
    map_jobs: Arc<Mutex<map_jobs::Manager>>,
    gameplay: Arc<RwLock<GameplayService>>,
}

impl AppState {
    pub fn new(config: &Config, build: impl Into<Arc<str>>) -> Result<Self, AppStateError> {
        Ok(Self {
            world: Arc::new(RwLock::new(None)),
            diagnostic_scenario: config.scenario,
            generation: Arc::new(AtomicU64::new(0)),
            tick_deadline_misses: Arc::new(AtomicU64::new(0)),
            build: build.into(),
            tick_period: Duration::from_secs_f64(1.0 / f64::from(config.tick_hz)),
            asset_pack: config.asset_pack.clone(),
            map_package_directory: config.map_package_directory.clone(),
            map_worker: config.map_worker.clone(),
            geodata_cache_directory: config.geodata_cache_directory.clone(),
            map_packages: Arc::new(RwLock::new(map_store::load(
                config.map_package_directory.as_deref(),
            )?)),
            terrain_cache: Arc::new(Mutex::new(terrain_cache::TerrainCache::default())),
            map_jobs: Arc::new(Mutex::new(map_jobs::Manager::default())),
            gameplay: Arc::new(RwLock::new(GameplayService::new(config.scenario.seed))),
        })
    }

    pub fn with_gameplay_population(
        config: &Config,
        build: impl Into<Arc<str>>,
    ) -> Result<Self, AppStateError> {
        let mut state = Self::new(config, build)?;
        let gameplay_config = aoe_core::WorldConfig {
            width_tiles: config.scenario.world_size,
            height_tiles: config.scenario.world_size,
            seed: config.scenario.seed,
            ..aoe_core::WorldConfig::default()
        };
        state.gameplay = Arc::new(RwLock::new(GameplayService::with_population(
            gameplay_config,
            config.scenario.entities,
            config.scenario.hotspot_entities,
            config.scenario.players,
            config.scenario.active_extent,
        )?));
        Ok(state)
    }

    async fn ensure_diagnostic_world(&self) {
        let mut world = self.world.write().await;
        if world.is_none() {
            *world = Some(World::new(self.diagnostic_scenario));
        }
    }

    pub async fn run_ticks(self) {
        let mut interval = tokio::time::interval(self.tick_period);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            let scheduled = interval.tick().await;
            let started = tokio::time::Instant::now();
            if let Some(world) = self.world.write().await.as_mut() {
                world.advance();
            }
            self.gameplay.read().await.clone().tick().await;
            if started.duration_since(scheduled) > self.tick_period
                || started.elapsed() > self.tick_period
            {
                self.tick_deadline_misses.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    pub fn tick_deadline_misses(&self) -> u64 {
        self.tick_deadline_misses.load(Ordering::Relaxed)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AppStateError {
    #[error(transparent)]
    MapStore(#[from] MapStoreError),
    #[error(transparent)]
    GameWorld(#[from] GameWorldError),
}

#[derive(Serialize)]
struct Health {
    status: &'static str,
    build: String,
    scenario: String,
    tick: u64,
    entities: usize,
    loaded_chunks: usize,
    tick_deadline_misses: u64,
    terrain_cache: terrain_cache::Usage,
}

async fn health(State(state): State<AppState>) -> Json<Health> {
    let terrain_cache = state.terrain_cache.lock().await.usage();
    state.ensure_diagnostic_world().await;
    let world_guard = state.world.read().await;
    let Some(world) = world_guard.as_ref() else {
        return Json(Health {
            status: "unavailable",
            build: state.build.to_string(),
            scenario: state.diagnostic_scenario.name.to_owned(),
            tick: 0,
            entities: 0,
            loaded_chunks: 0,
            tick_deadline_misses: state.tick_deadline_misses(),
            terrain_cache,
        });
    };
    Json(Health {
        status: "ok",
        build: state.build.to_string(),
        scenario: world.scenario().name.to_owned(),
        tick: world.tick().0,
        entities: world.entities().len(),
        loaded_chunks: world.loaded_chunks(),
        tick_deadline_misses: state.tick_deadline_misses(),
        terrain_cache,
    })
}

#[derive(Deserialize)]
struct ReplayQuery {
    scenario: String,
    ticks: u32,
}

#[derive(Serialize)]
struct ReplayResult {
    scenario: String,
    ticks: u32,
    hash: String,
}

async fn replay_hash(Query(query): Query<ReplayQuery>) -> Result<Json<ReplayResult>, StatusCode> {
    if query.ticks > 64 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let scenario = aoe_scenario::named(&query.scenario).ok_or(StatusCode::BAD_REQUEST)?;
    let ticks = query.ticks;
    tokio::task::spawn_blocking(move || {
        let mut world = World::new(scenario);
        for _ in 0..ticks {
            world.advance();
        }
        Json(ReplayResult {
            scenario: scenario.name.to_owned(),
            ticks,
            hash: world.canonical_hash_hex(),
        })
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn select_scenario(
    Path(name): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Health>, StatusCode> {
    let scenario = aoe_scenario::named(&name).ok_or(StatusCode::BAD_REQUEST)?;
    let next = tokio::task::spawn_blocking(move || World::new(scenario))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    *state.world.write().await = Some(next);
    state.generation.fetch_add(1, Ordering::SeqCst);
    Ok(health(State(state)).await)
}

async fn websocket(State(state): State<AppState>, upgrade: WebSocketUpgrade) -> impl IntoResponse {
    state.ensure_diagnostic_world().await;
    upgrade
        .max_message_size(aoe_protocol::MAX_MESSAGE)
        .on_upgrade(move |socket| client(socket, state))
}

async fn gameplay_websocket(
    State(state): State<AppState>,
    upgrade: WebSocketUpgrade,
) -> impl IntoResponse {
    let gameplay = state.gameplay.read().await.clone();
    upgrade
        .max_message_size(aoe_protocol::GAMEPLAY_MAX_MESSAGE)
        .on_upgrade(move |socket| gameplay_transport::handle_socket(gameplay, socket))
}

pub fn app(state: AppState) -> Router {
    let asset_pack = state.asset_pack.clone();
    let router = Router::new()
        .route("/health", get(health))
        .route("/replay-hash", get(replay_hash))
        .route("/maps/estimate", post(maps::estimate))
        .route("/maps/footprint", post(maps::footprint))
        .route("/maps/jobs", get(maps::list_jobs).post(maps::create_job))
        .route("/maps/jobs/{job_id}", get(maps::job_status))
        .route("/maps/jobs/{job_id}/cancel", post(maps::cancel_job))
        .route("/maps/activate", post(maps::activate))
        .route("/maps/reset", post(maps::reset))
        .route("/maps", get(maps::list))
        .route("/maps/{content_hash}/preview", get(maps::preview))
        .route(
            "/maps/{content_hash}",
            get(maps::package).post(maps::activate_package),
        )
        .route("/maps/{content_hash}/chunks/{x}/{y}", get(maps::chunk))
        .route("/scenario/{name}", post(select_scenario))
        .route("/ws", get(websocket))
        .route("/game/ws", get(gameplay_websocket))
        .fallback_service(ServeDir::new("web").append_index_html_on_directories(true))
        .with_state(state);
    if let Some(pack) = asset_pack {
        router.nest_service("/asset-pack", ServeDir::new(pack))
    } else {
        router
    }
}

fn states(entities: Vec<Entity>) -> BTreeMap<EntityId, EntityState> {
    entities
        .into_iter()
        .map(|e| {
            (
                e.id,
                EntityState {
                    id: e.id,
                    player: e.player,
                    position: e.position,
                },
            )
        })
        .collect()
}

async fn send(socket: &mut WebSocket, message: ServerMessage) -> bool {
    let Ok(bytes) = encode_server(&message) else {
        return false;
    };
    timeout(
        Duration::from_millis(500),
        socket.send(Message::Binary(bytes.into())),
    )
    .await
    .is_ok_and(|result| result.is_ok())
}

async fn error(socket: &mut WebSocket, code: u16, message: &str) {
    let _ = send(
        socket,
        ServerMessage::Error {
            code,
            message: message.to_owned(),
        },
    )
    .await;
}

async fn snapshot(
    socket: &mut WebSocket,
    state: &AppState,
    region: Region,
) -> Option<(Tick, BTreeMap<EntityId, EntityState>)> {
    let world_guard = state.world.read().await;
    let world = world_guard.as_ref()?;
    let tick = world.tick();
    let entities = states(world.query(region));
    if entities.len() > MAX_ENTITIES {
        drop(world_guard);
        error(
            socket,
            413,
            "subscribed region exceeds protocol entity limit",
        )
        .await;
        return None;
    }
    let response = ServerMessage::Snapshot {
        tick,
        region,
        entities: entities.values().copied().collect(),
        loaded_chunks: world.loaded_chunks() as u32,
        total_entities: world.entities().len() as u32,
    };
    drop(world_guard);
    send(socket, response).await.then_some((tick, entities))
}

async fn client(mut socket: WebSocket, state: AppState) {
    let hello = timeout(Duration::from_secs(5), socket.next()).await;
    let Ok(Some(Ok(Message::Binary(bytes)))) = hello else {
        return;
    };
    match decode_client(&bytes) {
        Ok(ClientMessage::Hello { version: VERSION }) => {}
        Ok(ClientMessage::Hello { .. }) => {
            error(&mut socket, 426, "protocol version mismatch").await;
            return;
        }
        _ => {
            error(&mut socket, 400, "expected binary hello").await;
            return;
        }
    }
    let scenario = state
        .world
        .read()
        .await
        .as_ref()
        .map(World::scenario)
        .unwrap_or(state.diagnostic_scenario);
    if !send(
        &mut socket,
        ServerMessage::Hello {
            version: VERSION,
            build: state.build.to_string(),
            scenario: scenario.name.to_owned(),
            world_size: scenario.world_size,
        },
    )
    .await
    {
        return;
    }

    let mut observed_generation = state.generation.load(Ordering::SeqCst);

    let mut region: Option<Region> = None;
    let mut previous = BTreeMap::new();
    let mut last_tick = Tick(0);
    let mut interval = tokio::time::interval(state.tick_period);
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            incoming = socket.next() => {
                let Some(Ok(Message::Binary(bytes))) = incoming else { break; };
                match decode_client(&bytes) {
                    Ok(ClientMessage::Subscribe { region: requested }) if requested.valid(state.world.read().await.as_ref().map_or(state.diagnostic_scenario.world_size, |world| world.scenario().world_size)) => {
                        region = Some(requested);
                        if let Some((tick, entities)) = snapshot(&mut socket, &state, requested).await { last_tick = tick; previous = entities; } else { break; }
                    }
                    Ok(ClientMessage::Resync) => {
                        if let Some(requested) = region {
                            if let Some((tick, entities)) = snapshot(&mut socket, &state, requested).await { last_tick = tick; previous = entities; } else { break; }
                        } else { error(&mut socket, 400, "subscribe before resync").await; }
                    }
                    _ => { error(&mut socket, 400, "invalid request or region").await; break; }
                }
            }
            _ = interval.tick(), if region.is_some() => {
                let generation = state.generation.load(Ordering::SeqCst);
                if generation != observed_generation {
                    observed_generation = generation;
                    region = None;
                    previous.clear();
                    let scenario = state
                        .world
                        .read()
                        .await
                        .as_ref()
                        .map(World::scenario)
                        .unwrap_or(state.diagnostic_scenario);
                    if !send(&mut socket, ServerMessage::Hello { version: VERSION, build: state.build.to_string(), scenario: scenario.name.to_owned(), world_size: scenario.world_size }).await { break; }
                    continue;
                }
                let Some(requested) = region else { continue; };
                let world_guard = state.world.read().await;
                let Some(world) = world_guard.as_ref() else { continue; };
                if world.tick() == last_tick { continue; }
                let tick = world.tick();
                let next = states(world.query(requested));
                drop(world_guard);
                if next.len() > MAX_ENTITIES {
                    error(&mut socket, 413, "subscribed region exceeds protocol entity limit").await;
                    break;
                }
                let upserts = next.iter().filter_map(|(id, value)| (previous.get(id) != Some(value)).then_some(*value)).collect();
                let removals = previous.keys().filter(|id| !next.contains_key(id)).copied().collect();
                if !send(&mut socket, ServerMessage::Delta { tick, upserts, removals }).await { break; }
                previous = next;
                last_tick = tick;
            }
        }
    }
}
