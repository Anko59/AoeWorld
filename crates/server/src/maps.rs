use crate::{AppState, GameplayService, map_jobs, map_store, map_worker, terrain_cache};
use aoe_core::TileCoord;
use aoe_map::{CompactChunk, MAP_SCHEMA_VERSION, MapEstimate, MapPackage, MapRequest};
use aoe_protocol::ResumeToken;
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use serde::Serialize;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

const CONTROLLER_TOKEN_HEADER: &str = "x-aoeworld-controller-token";
const PREVIEW_SAMPLES_PER_AXIS: u16 = 16;
struct RequestCancellation(Arc<AtomicBool>);

impl Drop for RequestCancellation {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

fn request_cancellation() -> (RequestCancellation, Arc<AtomicBool>) {
    let cancelled = Arc::new(AtomicBool::new(false));
    (RequestCancellation(cancelled.clone()), cancelled)
}

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
pub(super) struct GeographicFootprint {
    points: Vec<map_worker::GeographicPoint>,
    distortion: map_worker::ProjectionDistortion,
}

/// Computes the picker boundary through the native WGS84 projection worker;
/// it does not acquire source data or create a map package.
pub(super) async fn footprint(
    State(state): State<AppState>,
    Json(request): Json<MapRequest>,
) -> Result<Json<GeographicFootprint>, (StatusCode, String)> {
    let request = request
        .normalized()
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
    let worker = state.map_worker.clone().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "the native geographic projection worker is not configured".to_owned(),
    ))?;
    let footprint =
        tokio::task::spawn_blocking(move || map_worker::geographic_footprint(&worker, request))
            .await
            .map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "geographic projection task failed".to_owned(),
                )
            })?
            .map_err(|error| (StatusCode::BAD_REQUEST, error))?;
    Ok(Json(GeographicFootprint {
        points: footprint.points,
        distortion: footprint.distortion,
    }))
}

#[derive(Serialize)]
pub(super) struct Activation {
    content_hash: String,
    tiles_per_side: u64,
    source_lock_count: usize,
    uses_fallback_data: bool,
    start_available: bool,
    message: Option<&'static str>,
}

#[derive(Serialize)]
pub(super) struct PackageSummary {
    content_hash: String,
    request: MapRequest,
    estimate: MapEstimate,
    source_lock_count: usize,
    uses_fallback_data: bool,
}

/// A compact, read-only terrain sample. It deliberately reports tile facts,
/// not a gameplay start decision: activation owns the latter search.
#[derive(Serialize)]
pub(super) struct MapPreview {
    samples_per_axis: u16,
    minimum_height_centimeters: i32,
    maximum_height_centimeters: i32,
    source_backed: bool,
    cells: Vec<PreviewCell>,
}

#[derive(Serialize)]
struct PreviewCell {
    geographic_height_centimeters: i32,
    material: u8,
    biome: u8,
    water: u8,
    passable: bool,
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
    let (_cancellation_guard, cancelled) = request_cancellation();
    let provider = page_residency(&state, &package, cancelled.clone()).await?;
    let activation = Activation {
        content_hash: package.content_hash_hex(),
        tiles_per_side: package.estimate.tiles_per_side,
        source_lock_count: package.source_locks.len(),
        uses_fallback_data: package.source_locks.is_empty(),
        start_available: true,
        message: None,
    };
    let gameplay = tokio::task::spawn_blocking({
        let package = package.clone();
        move || {
            if let Some(provider) = provider {
                GameplayService::from_prepared_provider_with_cancel(package, provider, &|| {
                    cancelled.load(Ordering::Acquire)
                })
                .map_err(|error| error.to_string())
            } else {
                GameplayService::from_prepared_map(
                    package,
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                )
                .map_err(|error| error.to_string())
            }
        }
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
    let Some(gameplay) = gameplay else {
        return Ok(Json(Activation {
            start_available: false,
            message: Some("no suitable land start."),
            ..activation
        }));
    };
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

/// Samples immutable map terrain without activation. This lets an all-water
/// or all-ice package remain inspectable even when it has no valid start.
pub(super) async fn preview(
    Path(content_hash): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<MapPreview>, StatusCode> {
    let package = state
        .map_packages
        .read()
        .await
        .get(&content_hash)
        .cloned()
        .ok_or(StatusCode::NOT_FOUND)?;
    let (_cancellation_guard, cancelled) = request_cancellation();
    let provider = page_residency(&state, &package, cancelled.clone())
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    tokio::task::spawn_blocking(move || {
        preview_package(package, provider, &|| cancelled.load(Ordering::Acquire))
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .map(Json)
    .map_err(|_| StatusCode::NOT_FOUND)
}

fn preview_package(
    package: MapPackage,
    provider: Option<Arc<map_store::PageResidency>>,
    cancelled: &dyn Fn() -> bool,
) -> Result<MapPreview, String> {
    let generator = if package.environment.samples_per_axis == 0 {
        package.generator()
    } else {
        package
            .generator_with_page_provider(
                provider.ok_or_else(|| "prepared package has no page provider".to_owned())?,
            )
            .map_err(|error| error.to_string())?
    };
    let mut cells = Vec::with_capacity(usize::from(PREVIEW_SAMPLES_PER_AXIS).pow(2));
    let mut minimum_height_centimeters = i32::MAX;
    let mut maximum_height_centimeters = i32::MIN;
    for y in 0..PREVIEW_SAMPLES_PER_AXIS {
        for x in 0..PREVIEW_SAMPLES_PER_AXIS {
            let tile = TileCoord::new(
                preview_coordinate(package.estimate.tiles_per_side, x)?,
                preview_coordinate(package.estimate.tiles_per_side, y)?,
            );
            let tile = generator
                .tile_at_with_cancel(tile, cancelled)
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "preview sample was outside the package".to_owned())?;
            minimum_height_centimeters =
                minimum_height_centimeters.min(tile.geographic_height_centimeters);
            maximum_height_centimeters =
                maximum_height_centimeters.max(tile.geographic_height_centimeters);
            cells.push(PreviewCell {
                geographic_height_centimeters: tile.geographic_height_centimeters,
                material: tile.material as u8,
                biome: tile.biome as u8,
                water: tile.water as u8,
                passable: tile.passable,
            });
        }
    }
    Ok(MapPreview {
        samples_per_axis: PREVIEW_SAMPLES_PER_AXIS,
        minimum_height_centimeters,
        maximum_height_centimeters,
        source_backed: !package.source_locks.is_empty(),
        cells,
    })
}

fn preview_coordinate(tiles_per_side: u64, sample: u16) -> Result<i32, String> {
    let numerator = u64::from(sample)
        .checked_mul(tiles_per_side)
        .and_then(|value| value.checked_add(tiles_per_side / 2))
        .ok_or_else(|| "preview coordinate overflowed".to_owned())?;
    let coordinate = numerator
        .checked_div(u64::from(PREVIEW_SAMPLES_PER_AXIS))
        .ok_or_else(|| "preview sample count was zero".to_owned())?
        .min(tiles_per_side.saturating_sub(1));
    i32::try_from(coordinate).map_err(|_| "preview coordinate exceeded map bounds".to_owned())
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
) -> Result<Json<CompactChunk>, StatusCode> {
    let cache_key = terrain_cache::Key {
        content_hash: content_hash.clone(),
        x,
        y,
    };
    if let Some(chunk) = state.terrain_cache.lock().await.get(&cache_key) {
        return Ok(Json(chunk));
    }
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
    let (_cancellation_guard, cancelled) = request_cancellation();
    let provider = page_residency(&state, &package, cancelled.clone())
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let chunk = tokio::task::spawn_blocking(move || {
        let generator = if let Some(provider) = provider {
            package
                .generator_with_page_provider(provider)
                .map_err(|_| StatusCode::NOT_FOUND)?
        } else {
            package.generator()
        };
        let chunk = generator
            .chunk_with_cancel(x, y, &|| cancelled.load(Ordering::Acquire))
            .map_err(|_| StatusCode::NOT_FOUND)?;
        CompactChunk::encode(&chunk).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)??;
    state
        .terrain_cache
        .lock()
        .await
        .insert(cache_key, chunk.clone());
    Ok(Json(chunk))
}

async fn page_residency(
    state: &AppState,
    package: &MapPackage,
    cancelled: Arc<AtomicBool>,
) -> Result<Option<Arc<map_store::PageResidency>>, (StatusCode, String)> {
    if package.environment.samples_per_axis == 0 {
        return Ok(None);
    }
    let hash = package.content_hash_hex();
    let mut providers = state.page_residencies.write().await;
    if let Some(provider) = providers.get(&hash) {
        return Ok(Some(provider));
    }
    let directory = state.map_package_directory.clone().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "prepared map package storage is not configured".to_owned(),
    ))?;
    let package_copy = package.clone();
    let provider = tokio::task::spawn_blocking(move || {
        map_store::PageResidency::open(&directory, &package_copy, &|| {
            cancelled.load(Ordering::Acquire)
        })
        .map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "map page index task failed".to_owned(),
        )
    })?
    .map_err(|error| (StatusCode::NOT_FOUND, error))?;
    providers.insert(hash, provider.clone());
    Ok(Some(provider))
}
#[cfg(test)]
mod tests;
