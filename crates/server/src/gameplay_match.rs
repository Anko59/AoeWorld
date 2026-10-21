//! Hosting a match: several seats on one map, each with its own village.
use crate::gameplay::SlotSetup;
use crate::{AppState, GameplayService, map_store, maps};
use aoe_core::{PlayerId, TileCoord};
use aoe_map::{EnvironmentPageProvider, MAP_SCHEMA_VERSION, MapPackage, MapRequest};
use aoe_protocol::ResumeToken;
use aoe_simulation::{EconomyError, GameWorld, GameWorldError};
use axum::{Json, extract::State, http::HeaderMap, http::StatusCode};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, atomic::Ordering};

/// Header carrying the match-hosting secret (`AOE_MATCH_TOKEN`).
pub(crate) const ADMIN_TOKEN_HEADER: &str = "x-aoeworld-admin-token";

/// Upper bound on seats of one match (the simulation's player capacity).
pub const MAX_MATCH_SLOTS: usize = aoe_protocol::MAX_ECONOMY_PLAYERS;

#[derive(Debug, thiserror::Error)]
pub enum MatchError {
    #[error("a match needs between 1 and {MAX_MATCH_SLOTS} slots")]
    SlotCount,
    #[error("map has no land start for slot {0}; use a larger or less watery map")]
    NoStart(usize),
    #[error(transparent)]
    World(#[from] GameWorldError),
}

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
        let starts = place_villages(&mut world, humans.len(), cancelled)?;
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
        Ok(Self::from_world_slots(
            world,
            setups,
            Some(content_hash),
            Some(metadata),
        ))
    }
}

/// Ideal village centres: a ring for a few players (neighbours equally far
/// apart, so two players start on opposite sides), a grid for many.
pub(crate) fn ideal_starts(side: i32, count: usize) -> Vec<TileCoord> {
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
                let angle = std::f64::consts::TAU * index as f64 / count as f64
                    + std::f64::consts::FRAC_PI_4;
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

/// Gives every seat a village near its ideal point, searching outward in
/// square rings and keeping villages apart. Returns each seat's scout.
fn place_villages(
    world: &mut GameWorld,
    count: usize,
    cancelled: &dyn Fn() -> bool,
) -> Result<Vec<aoe_core::EntityId>, MatchError> {
    let side = world.config().width_tiles;
    let ideals = ideal_starts(side, count);
    let reach = if count <= 8 {
        side / 6
    } else {
        side / (2 * (count as f64).sqrt().ceil() as i32)
    }
    .max(8);
    let separation = (reach / 2).max(10);
    let mut placed: Vec<TileCoord> = Vec::new();
    let mut scouts = Vec::new();
    for (index, ideal) in ideals.into_iter().enumerate() {
        let mut found = None;
        'search: for ring in (0..=reach).step_by(2) {
            for tile in ring_tiles(ideal, ring) {
                if cancelled() {
                    return Err(GameWorldError::StartSearchLimit.into());
                }
                if tile.x < 4
                    || tile.y < 4
                    || tile.x >= side - 4
                    || tile.y >= side - 4
                    || placed.iter().any(|other| {
                        (other.x - tile.x).abs().max((other.y - tile.y).abs()) < separation
                    })
                {
                    continue;
                }
                match world.initialize_village(PlayerId(index as u16), tile) {
                    Ok(scout) => {
                        found = Some((tile, scout));
                        break 'search;
                    }
                    Err(EconomyError::InvalidFootprint) => {}
                    Err(EconomyError::Movement(error)) => return Err(error.into()),
                    Err(_) => return Err(MatchError::NoStart(index)),
                }
            }
        }
        let (tile, scout) = found.ok_or(MatchError::NoStart(index))?;
        placed.push(tile);
        scouts.push(scout);
    }
    Ok(scouts)
}

/// The tiles at Chebyshev distance `ring` from `center`, every second one.
fn ring_tiles(center: TileCoord, ring: i32) -> Vec<TileCoord> {
    if ring == 0 {
        return vec![center];
    }
    let mut tiles = Vec::new();
    for offset in (-ring..=ring).step_by(2) {
        tiles.push(TileCoord::new(center.x + offset, center.y - ring));
        tiles.push(TileCoord::new(center.x + offset, center.y + ring));
        if offset.abs() != ring {
            tiles.push(TileCoord::new(center.x - ring, center.y + offset));
            tiles.push(TileCoord::new(center.x + ring, center.y + offset));
        }
    }
    tiles
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
    slots: Vec<SlotKind>,
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
    slots: Vec<MatchSlot>,
}

fn hex(token: ResumeToken) -> String {
    token.0.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(crate) async fn start_match(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<MatchRequest>,
) -> Result<Json<MatchResponse>, (StatusCode, String)> {
    let allowed = state.match_admin_token.as_deref().is_some_and(|secret| {
        headers
            .get(ADMIN_TOKEN_HEADER)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|given| given == secret)
    });
    if !allowed {
        return Err((
            StatusCode::FORBIDDEN,
            "match hosting needs the AOE_MATCH_TOKEN secret".to_owned(),
        ));
    }
    let package = match (&request.map_hash, request.map) {
        (Some(hash), None) => state
            .map_packages
            .read()
            .await
            .get(hash)
            .cloned()
            .ok_or((StatusCode::NOT_FOUND, "unknown map package".to_owned()))?,
        (None, Some(map)) => {
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
                "give exactly one of map_hash and map".to_owned(),
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
    let service = tokio::task::spawn_blocking(move || {
        GameplayService::from_match(
            package,
            provider.map(|value| value as Arc<dyn EnvironmentPageProvider>),
            &humans,
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
        slots,
    }))
}

#[path = "gameplay_match/tests.rs"]
#[cfg(test)]
mod tests;
