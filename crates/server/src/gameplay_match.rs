//! Hosting a match: several seats on one map, each with its own village.
use crate::gameplay::SlotSetup;
use crate::{AppState, GameplayService, map_store, maps};
use aoe_core::{PlayerId, TileCoord};
use aoe_map::{EnvironmentPageProvider, MAP_SCHEMA_VERSION, MapPackage, MapRequest};
use aoe_protocol::ResumeToken;
use aoe_simulation::{GameWorld, GameWorldError};
use axum::{Json, extract::State, http::HeaderMap, http::StatusCode};
use serde::{Deserialize, Serialize};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

/// Header carrying the match-hosting secret (`AOE_MATCH_TOKEN`).
pub(crate) const ADMIN_TOKEN_HEADER: &str = "x-aoeworld-admin-token";

/// Upper bound on seats of one match (the simulation's player capacity).
pub const MAX_MATCH_SLOTS: usize = aoe_protocol::MAX_ECONOMY_PLAYERS;

impl GameplayService {
    /// A match on `package`: `humans[i]` says whether slot `i` is a person
    /// (claimable by a plain Hello) or an AI process (token only). Player ids
    /// are the slot indices, and the villages are spread over the whole map.
    pub fn from_match(
        package: MapPackage,
        provider: Option<Arc<dyn EnvironmentPageProvider>>,
        humans: &[bool],
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Self, MatchError> {
        let rules = MatchRules::default();
        Self::from_match_rules(
            package,
            provider,
            humans,
            &rules,
            &AtomicUsize::new(0),
            cancelled,
        )
        .map(|(service, _)| service)
    }

    /// `from_match` under `rules`: the population ceiling (with the world
    /// capacities that grow with it) and the start units of every seat.
    /// Also returns how many start units were placed; `placed` counts the
    /// villages settled so far (a loading screen's progress).
    pub fn from_match_rules(
        package: MapPackage,
        provider: Option<Arc<dyn EnvironmentPageProvider>>,
        humans: &[bool],
        rules: &MatchRules,
        placed: &AtomicUsize,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<(Self, u64), MatchError> {
        if humans.is_empty() || humans.len() > MAX_MATCH_SLOTS {
            return Err(MatchError::SlotCount);
        }
        let content_hash = package.content_hash;
        let metadata = crate::gameplay_map::map_metadata(&package);
        let mut world = match provider {
            Some(provider) => GameWorld::from_page_provider(package, provider)?,
            None if package.environment.samples_per_axis == 0 => GameWorld::from_map(package)?,
            None => return Err(GameWorldError::InvalidTerrain.into()),
        };
        world.set_limits(rules.limits(humans.len()));
        let starts = place_villages(&mut world, humans.len(), rules.seed, placed, cancelled)?;
        let seeded = rules::seed_units(&mut world, &starts, rules, cancelled)?;
        let setups = starts
            .into_iter()
            .zip(humans)
            .enumerate()
            .map(|(index, (primary_unit_id, human))| SlotSetup {
                player: PlayerId(index as u16),
                primary_unit_id,
                human: *human,
            })
            .collect();
        Ok((
            Self::from_world_slots(world, setups, Some(content_hash), Some(metadata)),
            seeded,
        ))
    }
}

/// Ideal village centres: a ring for a few players (neighbours equally far
/// apart, so two players start on opposite sides), a grid for many.
#[cfg(test)]
pub(crate) fn ideal_starts(side: i32, count: usize) -> Vec<TileCoord> {
    ring_or_grid(side, count, std::f64::consts::FRAC_PI_4)
}

/// `ideal_starts` with the ring (for up to eight players) turned by `offset`.
pub(super) fn ring_or_grid(side: i32, count: usize, offset: f64) -> Vec<TileCoord> {
    let margin = (side / 10).max(12);
    let span = (side - 2 * margin).max(1);
    let middle = side / 2;
    if count == 1 {
        return vec![TileCoord::new(middle, middle)];
    }
    if count <= 8 {
        let radius = f64::from(span) / 2.0 * 0.85;
        return (0..count)
            .map(|index| {
                let angle = std::f64::consts::TAU * index as f64 / count as f64 + offset;
                TileCoord::new(
                    middle + (angle.cos() * radius).round() as i32,
                    middle + (angle.sin() * radius).round() as i32,
                )
            })
            .collect();
    }
    let columns = (count as f64).sqrt().ceil() as i32;
    let cell = span / columns;
    (0..count as i32)
        .map(|index| {
            TileCoord::new(
                margin + (index % columns) * cell + cell / 2,
                margin + (index / columns) * cell + cell / 2,
            )
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SlotKind {
    Human,
    Ai,
}

/// `POST /match`: replace the running game with a new match. Name the map by
/// the hash of a known package or give a `MapRequest` to create one.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MatchRequest {
    map_hash: Option<String>,
    map: Option<MapRequest>,
    /// A flat synthetic ground (see `flat.rs`), the usual place for AI tests.
    flat: Option<flat::FlatMap>,
    slots: Vec<SlotKind>,
    /// The most population any player may reach (5 to 5,000, default 200).
    /// World capacities for buildings and workers grow in proportion.
    population_cap: Option<u16>,
    /// Extra villagers and militia every player starts with.
    start_units: Option<StartUnits>,
    /// Seeds the village layout (which slot starts where). Omitted: a fresh
    /// seed per match, reported back so the layout can be reproduced.
    seed: Option<u64>,
}

#[derive(Serialize)]
pub(crate) struct MatchSlot {
    slot: usize,
    player: u16,
    kind: SlotKind,
    /// 48 hex digits; present it in `Hello`/`JoinSlot` to take this seat.
    token: String,
    primary_unit_id: u32,
    start_tile: (i32, i32),
}

#[derive(Serialize)]
pub(crate) struct MatchResponse {
    world_id: u64,
    content_hash: String,
    tiles_per_side: u64,
    population_cap: u16,
    /// Units placed by `start_units`, beyond the standard start kit.
    seeded_units: u64,
    /// Villages whose starting kit could not be placed in full (0 when every
    /// village has its complete kit).
    partial_starting_kits: u64,
    /// The seed the village layout was drawn from.
    seed: u64,
    slots: Vec<MatchSlot>,
}

fn hex(token: ResumeToken) -> String {
    token.0.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn authorize(state: &AppState, headers: &HeaderMap) -> Result<(), (StatusCode, String)> {
    let allowed = state.match_admin_token.as_deref().is_some_and(|secret| {
        headers
            .get(ADMIN_TOKEN_HEADER)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|given| given == secret)
    });
    if allowed {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            "match hosting needs the AOE_MATCH_TOKEN secret".to_owned(),
        ))
    }
}

pub(crate) fn routes() -> axum::Router<AppState> {
    use axum::routing::{get, post};
    axum::Router::new()
        .route("/match", post(start_match))
        .route("/match/stats", get(stats::match_stats))
        .merge(skirmish::routes())
}

async fn start_match(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<MatchRequest>,
) -> Result<Json<MatchResponse>, (StatusCode, String)> {
    authorize(&state, &headers)?;
    let rules = MatchRules::new(request.population_cap, request.start_units)
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
    let seed = request.seed.unwrap_or_else(fresh_seed);
    let rules = rules.with_seed(seed);
    let package = match (&request.map_hash, request.map, request.flat) {
        (None, None, Some(flat)) => {
            let directory = state.map_package_directory.clone().ok_or((
                StatusCode::SERVICE_UNAVAILABLE,
                "a flat map needs a configured map package directory".to_owned(),
            ))?;
            tokio::task::spawn_blocking(move || {
                let (package, pages) = flat::package(flat).map_err(|error| error.to_string())?;
                map_store::persist_prepared(Some(&directory), &package, &pages, &[], &[], &[])
                    .map_err(|error| error.to_string())?;
                Ok::<_, String>(package)
            })
            .await
            .map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "map task failed".to_owned(),
                )
            })?
            .map_err(|error| (StatusCode::BAD_REQUEST, error))?
        }
        (Some(hash), None, None) => state
            .map_packages
            .read()
            .await
            .get(hash)
            .cloned()
            .ok_or((StatusCode::NOT_FOUND, "unknown map package".to_owned()))?,
        (None, Some(map), None) => {
            let directory = state.map_package_directory.clone();
            tokio::task::spawn_blocking(move || {
                let package = MapPackage::new(MAP_SCHEMA_VERSION, map, Vec::new())
                    .map_err(|error| error.to_string())?;
                map_store::persist(directory.as_deref(), &package)
                    .map_err(|error| error.to_string())?;
                Ok::<_, String>(package)
            })
            .await
            .map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "map task failed".to_owned(),
                )
            })?
            .map_err(|error| (StatusCode::BAD_REQUEST, error))?
        }
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                "give exactly one of map_hash, map and flat".to_owned(),
            ));
        }
    };
    let (_guard, cancelled) = maps::request_cancellation();
    let provider = maps::page_residency(&state, &package, cancelled.clone()).await?;
    let humans = request
        .slots
        .iter()
        .map(|kind| *kind == SlotKind::Human)
        .collect::<Vec<_>>();
    let (summary_hash, tiles) = (package.content_hash_hex(), package.estimate.tiles_per_side);
    let stored = package.clone();
    let (service, seeded_units) = tokio::task::spawn_blocking(move || {
        GameplayService::from_match_rules(
            package,
            provider.map(|value| value as Arc<dyn EnvironmentPageProvider>),
            &humans,
            &rules,
            &AtomicUsize::new(0),
            &|| cancelled.load(Ordering::Acquire),
        )
    })
    .await
    .map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "match task failed".to_owned(),
        )
    })?
    .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
    state
        .map_packages
        .write()
        .await
        .insert(summary_hash.clone(), stored);
    let slots = service
        .slot_infos()
        .await
        .into_iter()
        .zip(request.slots.iter())
        .enumerate()
        .map(|(slot, (info, kind))| MatchSlot {
            slot,
            player: info.player.0,
            kind: *kind,
            token: hex(info.token),
            primary_unit_id: info.primary_unit_id.0,
            start_tile: service.start_tile(info.primary_unit_id),
        })
        .collect();
    let partial_starting_kits = service.partial_starting_kits();
    let world_id = service.world_id();
    let previous = {
        let mut active = state.gameplay.write().await;
        std::mem::replace(&mut *active, service)
    };
    previous.retire(world_id).await;
    state.generation.fetch_add(1, Ordering::SeqCst);
    Ok(Json(MatchResponse {
        world_id,
        content_hash: summary_hash,
        tiles_per_side: tiles,
        population_cap: rules.population_cap,
        seeded_units,
        partial_starting_kits,
        seed,
        slots,
    }))
}

mod flat;
mod rules;
mod seats;
mod skirmish;
mod stats;
pub use rules::{MatchError, MatchRules, StartUnits};
pub(crate) use seats::fresh_seed;
use seats::place_villages;
#[cfg(test)]
use seats::ring_tiles;
pub(crate) use skirmish::Skirmish;
pub use stats::{TickProbe, TickStats};

#[path = "gameplay_match/tests.rs"]
#[cfg(test)]
mod tests;
