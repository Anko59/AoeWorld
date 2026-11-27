use super::{page_residency, request_cancellation};
use crate::AppState;
use aoe_map::ELEVATION_LEVEL_CENTIMETERS;
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Serialize;

/// Highest level of the procedural relief, which is symmetric around zero and
/// not compressed. signed_noise is bounded to [-2000, 2000]: the broad relief
/// term is scaled by 25 cm and the detail term divides by 8 toward zero.
const PROCEDURAL_HEIGHT_LEVEL: i16 = (50_250 / ELEVATION_LEVEL_CENTIMETERS) as i16;

/// Inclusive game-height range across every source elevation sample and
/// modeled-water surface in the immutable package.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct HeightBounds {
    content_hash: String,
    minimum_height_level: i16,
    maximum_height_level: i16,
}

/// Returns conservative game-height limits for client-side terrain culling.
/// Opening the package already read every page they come from, so no page is
/// loaded here.
pub(crate) async fn height_bounds(
    Path(content_hash): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<HeightBounds>, (StatusCode, String)> {
    let package = state
        .map_packages
        .read()
        .await
        .get(&content_hash)
        .cloned()
        .ok_or((StatusCode::NOT_FOUND, "unknown map package".to_owned()))?;
    let (_cancellation_guard, cancelled) = request_cancellation();
    let provider = page_residency(&state, &package, cancelled).await?;
    let (minimum_height_level, maximum_height_level) = match provider {
        Some(provider) => provider.height_bounds().ok_or((
            StatusCode::INTERNAL_SERVER_ERROR,
            "no elevation samples were available".to_owned(),
        ))?,
        None => (-PROCEDURAL_HEIGHT_LEVEL, PROCEDURAL_HEIGHT_LEVEL),
    };
    Ok(Json(HeightBounds {
        content_hash,
        minimum_height_level,
        maximum_height_level,
    }))
}
