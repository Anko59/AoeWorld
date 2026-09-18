use crate::{AppState, GameplayService, map_jobs, map_store};
use aoe_map::{MAP_SCHEMA_VERSION, MapEstimate, MapPackage, MapRequest};
use aoe_protocol::ResumeToken;
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use serde::Serialize;
use std::sync::atomic::Ordering;

const CONTROLLER_TOKEN_HEADER: &str = "x-aoeworld-controller-token";

fn controller_token(headers: &HeaderMap) -> Option<ResumeToken> {
    let value = headers.get(CONTROLLER_TOKEN_HEADER)?.to_str().ok()?;
    if value.len() != 48 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let mut bytes = [0_u8; 24];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(ResumeToken(bytes))
}

async fn require_controller(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(), (StatusCode, String)> {
    let token = controller_token(headers).ok_or((
        StatusCode::FORBIDDEN,
        "current gameplay controller authorization is required".to_owned(),
    ))?;
    let gameplay = state.gameplay.read().await.clone();
    gameplay.is_controller(token).await.then_some(()).ok_or((
        StatusCode::FORBIDDEN,
        "current gameplay controller authorization is required".to_owned(),
    ))
}

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

#[derive(Serialize)]
pub(super) struct PackageSummary {
    content_hash: String,
    request: MapRequest,
    estimate: MapEstimate,
    source_lock_count: usize,
    uses_fallback_data: bool,
}

pub(super) async fn activate(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<MapRequest>,
) -> Result<Json<Activation>, (StatusCode, String)> {
    require_controller(&state, &headers).await?;
    let map_package_directory = state.map_package_directory.clone();
    let package = tokio::task::spawn_blocking(move || {
        let package = MapPackage::new(MAP_SCHEMA_VERSION, request, Vec::new())
            .map_err(|error| error.to_string())?;
        map_store::persist(map_package_directory.as_deref(), &package)
            .map_err(|error| error.to_string())?;
        Ok::<_, String>(package)
    })
    .await
    .map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "map creation task failed".to_owned(),
        )
    })?
    .map_err(|error| (StatusCode::BAD_REQUEST, error))?;
    activate_completed(state, package).await
}

pub(super) async fn create_job(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<MapRequest>,
) -> Result<(StatusCode, Json<map_jobs::Job>), (StatusCode, String)> {
    require_controller(&state, &headers).await?;
    request
        .estimate()
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
    let job = map_jobs::start(&state, request)
        .await
        .map_err(|error| (StatusCode::TOO_MANY_REQUESTS, error))?;
    Ok((StatusCode::ACCEPTED, Json(job)))
}

pub(super) async fn job_status(
    Path(job_id): Path<u64>,
    State(state): State<AppState>,
) -> Result<Json<map_jobs::Job>, StatusCode> {
    map_jobs::status(&state, job_id)
        .await
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

pub(super) async fn list_jobs(State(state): State<AppState>) -> Json<Vec<map_jobs::Job>> {
    Json(map_jobs::list(&state).await)
}

pub(super) async fn cancel_job(
    Path(job_id): Path<u64>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<map_jobs::Job>, (StatusCode, String)> {
    require_controller(&state, &headers).await?;
    map_jobs::cancel(&state, job_id)
        .await
        .map(Json)
        .ok_or((StatusCode::NOT_FOUND, "unknown map creation job".to_owned()))
}

pub(super) async fn activate_package(
    Path(content_hash): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Activation>, (StatusCode, String)> {
    require_controller(&state, &headers).await?;
    let package = state
        .map_packages
        .read()
        .await
        .get(&content_hash)
        .cloned()
        .ok_or((StatusCode::NOT_FOUND, "unknown map package".to_owned()))?;
    activate_completed(state, package).await
}

async fn activate_completed(
    state: AppState,
    package: MapPackage,
) -> Result<Json<Activation>, (StatusCode, String)> {
    let activation = Activation {
        content_hash: package.content_hash_hex(),
        tiles_per_side: package.estimate.tiles_per_side,
        source_lock_count: package.source_locks.len(),
        uses_fallback_data: package.source_locks.is_empty(),
    };
    let gameplay = tokio::task::spawn_blocking({
        let package = package.clone();
        move || GameplayService::from_map(package).map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "map activation task failed".to_owned(),
        )
    })?
    .map_err(|error| (StatusCode::BAD_REQUEST, error))?;
    state
        .map_packages
        .write()
        .await
        .insert(activation.content_hash.clone(), package);
    let previous = {
        let mut active = state.gameplay.write().await;
        let previous = active.clone();
        *active = gameplay.clone();
        previous
    };
    previous.retire(gameplay.world_id()).await;
    state.generation.fetch_add(1, Ordering::SeqCst);
    Ok(Json(activation))
}

pub(super) async fn list(State(state): State<AppState>) -> Json<Vec<PackageSummary>> {
    Json(
        state
            .map_packages
            .read()
            .await
            .values()
            .map(|package| PackageSummary {
                content_hash: package.content_hash_hex(),
                request: package.request,
                estimate: package.estimate,
                source_lock_count: package.source_locks.len(),
                uses_fallback_data: package.source_locks.is_empty(),
            })
            .collect(),
    )
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
