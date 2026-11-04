//! Bounded HTTP lifecycle for progressive strategic rasters.
use super::*;
use crate::{
    AppState,
    maps::{page_residency, request_cancellation},
};
use axum::{
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use std::sync::atomic::Ordering;
/// Waits for one raster up to `budget`. A computation past its deadline keeps
/// running in the background so a retry finds it cached, but at most
/// `MAX_BACKGROUND_MINIMAPS` do so at once (and `pending` holds one per hash);
/// beyond that the work is cancelled when the guard drops.
pub(in crate::maps) async fn finish<G: Send + 'static>(
    cache: &mut MinimapCache,
    minimaps: &Arc<tokio::sync::Mutex<MinimapCache>>,
    content_hash: String,
    budget: Duration,
    mut task: tokio::task::JoinHandle<Result<Vec<u8>, MinimapError>>,
    cancellation_guard: G,
) -> Result<Arc<[u8]>, StatusCode> {
    match tokio::time::timeout(budget, &mut task).await {
        Ok(joined) => {
            let raster: Arc<[u8]> = joined
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .map_err(|error| match error {
                    MinimapError::Page(EnvironmentPageError::Cancelled) => {
                        StatusCode::SERVICE_UNAVAILABLE
                    }
                    _ => StatusCode::NOT_FOUND,
                })?
                .into();
            cache.insert(content_hash, raster.clone());
            Ok(raster)
        }
        Err(_) => {
            if cache.pending.len() < MAX_BACKGROUND_MINIMAPS
                && cache.pending.insert(content_hash.clone())
            {
                let minimaps = minimaps.clone();
                tokio::spawn(async move {
                    let _guard = cancellation_guard;
                    let result = task.await;
                    let mut cache = minimaps.lock().await;
                    cache.pending.remove(&content_hash);
                    if let Ok(Ok(raster)) = result {
                        cache.insert(content_hash, raster.into());
                    }
                });
            }
            Err(StatusCode::SERVICE_UNAVAILABLE)
        }
    }
}

/// Serves the cached raster, computing it once per package. The cache lock
/// is held while computing, so at most one raster is generated at a time and
/// concurrent requests for the same map reuse the first result.
pub(in crate::maps) async fn minimap(
    Path(content_hash): Path<String>,
    State(state): State<AppState>,
) -> Result<Response, StatusCode> {
    serve_minimap(content_hash, state, false).await
}

pub(in crate::maps) async fn coarse_minimap(
    Path(content_hash): Path<String>,
    State(state): State<AppState>,
) -> Result<Response, StatusCode> {
    serve_minimap(content_hash, state, true).await
}

async fn serve_minimap(
    content_hash: String,
    state: AppState,
    coarse: bool,
) -> Result<Response, StatusCode> {
    let package = state
        .map_packages
        .read()
        .await
        .get(&content_hash)
        .cloned()
        .ok_or(StatusCode::NOT_FOUND)?;
    let key = if coarse {
        format!("{content_hash}:coarse")
    } else {
        content_hash
    };
    let mut cache = state.minimaps.lock().await;
    let raster = if let Some(raster) = cache.get(&key) {
        raster
    } else if cache.pending.contains(&key) {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    } else {
        let (cancellation_guard, cancelled) = request_cancellation();
        let provider = page_residency(&state, &package, cancelled.clone())
            .await
            .map_err(|_| StatusCode::NOT_FOUND)?;
        let task = tokio::task::spawn_blocking(move || {
            if coarse {
                package_raster(&package, provider, 64, &|| {
                    cancelled.load(Ordering::Acquire)
                })
            } else {
                package_minimap(&package, provider, &|| cancelled.load(Ordering::Acquire))
            }
        });
        finish(
            &mut cache,
            &state.minimaps,
            key,
            MINIMAP_TIME_BUDGET,
            task,
            cancellation_guard,
        )
        .await?
    };
    drop(cache);
    Ok((
        [(header::CONTENT_TYPE, "application/octet-stream")],
        raster.to_vec(),
    )
        .into_response())
}
