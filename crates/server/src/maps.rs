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
use std::sync::atomic::Ordering;

const CONTROLLER_TOKEN_HEADER: &str = "x-aoeworld-controller-token";
const PREVIEW_SAMPLES_PER_AXIS: u16 = 16;

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
        let directory = state.map_package_directory.clone();
        move || {
            let elevation_pages = map_store::load_elevation_pages(directory.as_deref(), &package)
                .map_err(|error| error.to_string())?;
            let water_pages = map_store::load_water_pages(directory.as_deref(), &package)
                .map_err(|error| error.to_string())?;
            let vegetation_pages = map_store::load_vegetation_pages(directory.as_deref(), &package)
                .map_err(|error| error.to_string())?;
            let land_use_pages = map_store::load_land_use_pages(directory.as_deref(), &package)
                .map_err(|error| error.to_string())?;
            GameplayService::from_prepared_map(
                package,
                elevation_pages,
                water_pages,
                vegetation_pages,
                land_use_pages,
            )
            .map_err(|error| error.to_string())
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
    let directory = state.map_package_directory.clone();
    tokio::task::spawn_blocking(move || preview_package(directory.as_deref(), package))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map(Json)
        .map_err(|_| StatusCode::NOT_FOUND)
}

fn preview_package(
    directory: Option<&std::path::Path>,
    package: MapPackage,
) -> Result<MapPreview, String> {
    let elevation_pages =
        map_store::load_elevation_pages(directory, &package).map_err(|error| error.to_string())?;
    let water_pages =
        map_store::load_water_pages(directory, &package).map_err(|error| error.to_string())?;
    let vegetation_pages =
        map_store::load_vegetation_pages(directory, &package).map_err(|error| error.to_string())?;
    let land_use_pages =
        map_store::load_land_use_pages(directory, &package).map_err(|error| error.to_string())?;
    let generator = package
        .generator_with_environment(
            elevation_pages,
            water_pages,
            vegetation_pages,
            land_use_pages,
        )
        .map_err(|error| error.to_string())?;
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
                .tile_at(tile)
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
    let directory = state.map_package_directory.clone();
    let chunk = tokio::task::spawn_blocking(move || {
        let elevation_pages = map_store::load_elevation_pages(directory.as_deref(), &package)
            .map_err(|_| StatusCode::NOT_FOUND)?;
        let water_pages = map_store::load_water_pages(directory.as_deref(), &package)
            .map_err(|_| StatusCode::NOT_FOUND)?;
        let vegetation_pages = map_store::load_vegetation_pages(directory.as_deref(), &package)
            .map_err(|_| StatusCode::NOT_FOUND)?;
        let land_use_pages = map_store::load_land_use_pages(directory.as_deref(), &package)
            .map_err(|_| StatusCode::NOT_FOUND)?;
        let generator = package
            .generator_with_environment(
                elevation_pages,
                water_pages,
                vegetation_pages,
                land_use_pages,
            )
            .map_err(|_| StatusCode::NOT_FOUND)?;
        CompactChunk::encode(&generator.chunk(x, y)).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_preview_is_bounded_and_does_not_require_activation() {
        let package = MapPackage::new(MAP_SCHEMA_VERSION, MapRequest::default(), Vec::new())
            .expect("fallback package");
        let preview = preview_package(None, package).expect("fallback preview");
        assert_eq!(preview.samples_per_axis, PREVIEW_SAMPLES_PER_AXIS);
        assert_eq!(
            preview.cells.len(),
            usize::from(PREVIEW_SAMPLES_PER_AXIS).pow(2)
        );
        assert!(!preview.source_backed);
        assert!(preview.minimum_height_centimeters <= preview.maximum_height_centimeters);
    }

    #[test]
    fn preview_coordinates_stay_inside_tiny_and_large_maps() {
        assert_eq!(preview_coordinate(1, 0).expect("coordinate"), 0);
        assert_eq!(
            preview_coordinate(500, PREVIEW_SAMPLES_PER_AXIS - 1).expect("coordinate"),
            484
        );
    }
}
