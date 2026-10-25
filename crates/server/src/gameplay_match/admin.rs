//! Token-protected, on-demand observations of the active game.
use super::authorize;
use crate::AppState;
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};

mod summary;

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/admin/match", get(match_state))
        .route("/admin/match/players/{id}", get(player_state))
        .route("/admin/fog", post(set_fog))
        .route("/admin/perf", get(perf))
}

type Error = (StatusCode, String);

async fn match_state(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Error> {
    authorize(&state, &headers)?;
    let service = state.gameplay.read().await.clone();
    let seats = service.slot_infos().await;
    let world = service.world.read().await;
    Ok(Json(summary::match_state(&world, &seats)))
}

#[derive(Deserialize)]
struct Page {
    #[serde(default)]
    offset: usize,
    #[serde(default = "default_limit")]
    limit: usize,
}
fn default_limit() -> usize {
    100
}

async fn player_state(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<u16>,
    Query(page): Query<Page>,
) -> Result<Json<Value>, Error> {
    authorize(&state, &headers)?;
    if !(1..=500).contains(&page.limit) {
        return Err((
            StatusCode::BAD_REQUEST,
            "limit must be between 1 and 500".into(),
        ));
    }
    let service = state.gameplay.read().await.clone();
    let seats = service.slot_infos().await;
    let world = service.world.read().await;
    if !seats.iter().any(|s| s.player.0 == id)
        && world.player_economy(aoe_core::PlayerId(id)).is_none()
    {
        return Err((StatusCode::NOT_FOUND, "unknown player".into()));
    }
    Ok(Json(summary::player_state(
        &world,
        aoe_core::PlayerId(id),
        page.offset,
        page.limit,
    )))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FogRequest {
    disabled: bool,
}

async fn set_fog(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<FogRequest>,
) -> Result<Json<Value>, Error> {
    authorize(&state, &headers)?;
    let service = state.gameplay.read().await.clone();
    service.set_fog_disabled(request.disabled).await;
    Ok(Json(json!({"disabled": request.disabled})))
}

async fn perf(State(state): State<AppState>, headers: HeaderMap) -> Result<Json<Value>, Error> {
    authorize(&state, &headers)?;
    let mut report = state.tick_stats.summary(state.tick_period);
    let service = state.gameplay.read().await.clone();
    report["gameplay_memory"] =
        serde_json::to_value(service.memory_usage().await).map_err(internal)?;
    report["terrain_cache"] =
        serde_json::to_value(state.terrain_cache.lock().await.usage()).map_err(internal)?;
    report["snapshot_store"] =
        serde_json::to_value(crate::map_store::navigation::snapshot_usage()).map_err(internal)?;
    Ok(Json(report))
}
fn internal(error: serde_json::Error) -> Error {
    (StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
}
