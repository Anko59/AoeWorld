use crate::{AppState, GameplayService, map_store};
use aoe_map::{MAP_SCHEMA_VERSION, MapEstimate, MapPackage, MapRequest};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Serialize;
use std::sync::atomic::Ordering;

pub(super) async fn estimate(
    Json(request): Json<MapRequest>,
) -> Result<Json<MapEstimate>, (StatusCode, String)> {
    request
        .estimate()
        .map(Json)
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))
}

#[derive(Serialize)]
pub(super) struct Activation {
    content_hash: String,
    tiles_per_side: u64,
    source_lock_count: usize,
    uses_fallback_data: bool,
}

pub(super) async fn activate(
    State(state): State<AppState>,
    Json(request): Json<MapRequest>,
) -> Result<Json<Activation>, (StatusCode, String)> {
    let map_package_directory = state.map_package_directory.clone();
    let (activation, package, gameplay) = tokio::task::spawn_blocking(move || {
        let package = MapPackage::new(MAP_SCHEMA_VERSION, request, Vec::new())
            .map_err(|error| error.to_string())?;
        let activation = Activation {
            content_hash: package.content_hash_hex(),
            tiles_per_side: package.estimate.tiles_per_side,
            source_lock_count: package.source_locks.len(),
            uses_fallback_data: package.source_locks.is_empty(),
        };
        map_store::persist(map_package_directory.as_deref(), &package)
            .map_err(|error| error.to_string())?;
        let gameplay =
            GameplayService::from_map(package.clone()).map_err(|error| error.to_string())?;
        Ok::<_, String>((activation, package, gameplay))
    })
    .await
    .map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "map creation task failed".to_owned(),
        )
    })?
    .map_err(|error| (StatusCode::BAD_REQUEST, error))?;
    state
        .map_packages
        .write()
        .await
        .insert(activation.content_hash.clone(), package);
    *state.gameplay.write().await = gameplay;
    state.generation.fetch_add(1, Ordering::SeqCst);
    Ok(Json(activation))
}

pub(super) async fn list(State(state): State<AppState>) -> Json<Vec<MapPackage>> {
    Json(state.map_packages.read().await.values().cloned().collect())
}

pub(super) async fn reset(State(state): State<AppState>) -> StatusCode {
    *state.gameplay.write().await = GameplayService::new(state.diagnostic_scenario.seed);
    state.generation.fetch_add(1, Ordering::SeqCst);
    StatusCode::NO_CONTENT
}

pub(super) async fn package(
    Path(content_hash): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<MapPackage>, StatusCode> {
    state
        .map_packages
        .read()
        .await
        .get(&content_hash)
        .cloned()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

pub(super) async fn chunk(
    Path((content_hash, x, y)): Path<(String, i32, i32)>,
    State(state): State<AppState>,
) -> Result<Json<aoe_map::Chunk>, StatusCode> {
    let package = state
        .map_packages
        .read()
        .await
        .get(&content_hash)
        .cloned()
        .ok_or(StatusCode::NOT_FOUND)?;
    let chunks =
        i32::try_from(package.chunk_count_per_side()).map_err(|_| StatusCode::NOT_FOUND)?;
    if x < 0 || y < 0 || x >= chunks || y >= chunks {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(Json(package.generator().chunk(x, y)))
}
