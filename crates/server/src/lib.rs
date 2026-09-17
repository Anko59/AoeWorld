//! HTTP and bounded WebSocket adapter for the synthetic world.
mod config;
pub use config::Config;

use aoe_core::{EntityId, Region, Tick};
use aoe_protocol::{
    ClientMessage, EntityState, ServerMessage, VERSION, decode_client, encode_server,
};
use aoe_simulation::{Entity, World};
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
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{
    sync::RwLock,
    time::{MissedTickBehavior, timeout},
};
use tower_http::services::ServeDir;

#[derive(Clone)]
pub struct AppState {
    world: Arc<RwLock<World>>,
    generation: Arc<AtomicU64>,
    build: Arc<str>,
    tick_period: Duration,
}

impl AppState {
    pub fn new(config: &Config, build: impl Into<Arc<str>>) -> Self {
        Self {
            world: Arc::new(RwLock::new(World::new(config.scenario))),
            generation: Arc::new(AtomicU64::new(0)),
            build: build.into(),
            tick_period: Duration::from_secs_f64(1.0 / f64::from(config.tick_hz)),
        }
    }

    pub async fn run_ticks(self) {
        let mut interval = tokio::time::interval(self.tick_period);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            self.world.write().await.advance();
        }
    }
}

#[derive(Serialize)]
struct Health {
    status: &'static str,
    build: String,
    scenario: String,
    tick: u64,
    entities: usize,
    loaded_chunks: usize,
}

async fn health(State(state): State<AppState>) -> Json<Health> {
    let world = state.world.read().await;
    Json(Health {
        status: "ok",
        build: state.build.to_string(),
        scenario: world.scenario().name.to_owned(),
        tick: world.tick().0,
        entities: world.entities().len(),
        loaded_chunks: world.loaded_chunks(),
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
    *state.world.write().await = next;
    state.generation.fetch_add(1, Ordering::SeqCst);
    Ok(health(State(state)).await)
}

async fn websocket(State(state): State<AppState>, upgrade: WebSocketUpgrade) -> impl IntoResponse {
    upgrade
        .max_message_size(aoe_protocol::MAX_MESSAGE)
        .on_upgrade(move |socket| client(socket, state))
}

pub fn app(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/replay-hash", get(replay_hash))
        .route("/scenario/{name}", post(select_scenario))
        .route("/ws", get(websocket))
        .fallback_service(ServeDir::new("web").append_index_html_on_directories(true))
        .with_state(state)
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
    let world = state.world.read().await;
    let tick = world.tick();
    let entities = states(world.query(region));
    let response = ServerMessage::Snapshot {
        tick,
        region,
        entities: entities.values().copied().collect(),
        loaded_chunks: world.loaded_chunks() as u32,
        total_entities: world.entities().len() as u32,
    };
    drop(world);
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
    let scenario = state.world.read().await.scenario();
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
                    Ok(ClientMessage::Subscribe { region: requested }) if requested.valid(state.world.read().await.scenario().world_size) => {
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
                    let scenario = state.world.read().await.scenario();
                    if !send(&mut socket, ServerMessage::Hello { version: VERSION, build: state.build.to_string(), scenario: scenario.name.to_owned(), world_size: scenario.world_size }).await { break; }
                    continue;
                }
                let Some(requested) = region else { continue; };
                let world = state.world.read().await;
                if world.tick() == last_tick { continue; }
                let tick = world.tick();
                let next = states(world.query(requested));
                drop(world);
                let upserts = next.iter().filter_map(|(id, value)| (previous.get(id) != Some(value)).then_some(*value)).collect();
                let removals = previous.keys().filter(|id| !next.contains_key(id)).copied().collect();
                if !send(&mut socket, ServerMessage::Delta { tick, upserts, removals }).await { break; }
                previous = next;
                last_tick = tick;
            }
        }
    }
}
