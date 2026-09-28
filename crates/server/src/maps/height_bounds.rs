use super::{page_residency, request_cancellation};
use crate::AppState;
use aoe_map::{
    ELEVATION_LEVEL_CENTIMETERS, ENVIRONMENT_PAGE_SAMPLES, EnvironmentPage, EnvironmentPageError,
    EnvironmentPageKey, EnvironmentPageProvider, HydrologyEvidencePage, MapPackage, PageLayer,
    Ratio,
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Serialize;
use std::sync::{Arc, atomic::Ordering};

/// Inclusive game-height range across every source elevation sample and
/// modeled-water surface in the immutable package.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct HeightBounds {
    content_hash: String,
    minimum_height_level: i16,
    maximum_height_level: i16,
}

impl HeightBounds {
    fn new(content_hash: String, minimum_height_level: i16, maximum_height_level: i16) -> Self {
        Self {
            content_hash,
            minimum_height_level,
            maximum_height_level,
        }
    }
}

/// Returns conservative game-height limits for client-side terrain culling.
/// The expensive page walk runs off Tokio workers and reuses the package's
/// bounded, hash-verifying page residency.
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
    let provider = page_residency(&state, &package, cancelled.clone()).await?;
    if let Some(provider) = provider.as_ref()
        && let Some((minimum, maximum)) = provider.cached_height_bounds()
    {
        return Ok(Json(HeightBounds::new(content_hash, minimum, maximum)));
    }
    let provider_for_task = provider.clone();
    let bounds = tokio::task::spawn_blocking(move || {
        let provider = provider_for_task
            .as_ref()
            .map(|value| value.as_ref() as &dyn EnvironmentPageProvider);
        package_height_bounds(&package, provider, &|| cancelled.load(Ordering::Acquire))
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "height bounds task failed".to_owned(),
        )
    })?
    .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;
    if let Some(provider) = provider.as_ref() {
        provider.cache_height_bounds(bounds.0, bounds.1);
    }
    Ok(Json(HeightBounds::new(content_hash, bounds.0, bounds.1)))
}

#[derive(Debug, Eq, PartialEq, thiserror::Error)]
pub(super) enum HeightBoundsError {
    #[error("prepared package has no level-zero elevation index")]
    MissingElevationIndex,
    #[error("verified environment page has an unexpected type or coordinate")]
    InvalidPage,
    #[error("no elevation samples were available")]
    NoElevationSamples,
    #[error(transparent)]
    Page(#[from] EnvironmentPageError),
}

pub(super) fn package_height_bounds(
    package: &MapPackage,
    provider: Option<&dyn EnvironmentPageProvider>,
    cancelled: &dyn Fn() -> bool,
) -> Result<(i16, i16), HeightBoundsError> {
    if package.environment.samples_per_axis == 0 {
        // signed_noise is bounded to [-2000, 2000]. The broad relief term is
        // scaled by 25 cm and the detail term divides by 8 toward zero.
        return Ok((
            quantize_height(
                -50_250,
                Ratio {
                    numerator: 1,
                    denominator: 1,
                },
            ),
            quantize_height(
                50_250,
                Ratio {
                    numerator: 1,
                    denominator: 1,
                },
            ),
        ));
    }
    let provider = provider.ok_or(HeightBoundsError::MissingElevationIndex)?;
    let level_zero = package
        .environment
        .elevation
        .levels
        .first()
        .ok_or(HeightBoundsError::MissingElevationIndex)?;
    let side = u16::from(ENVIRONMENT_PAGE_SAMPLES);
    let page_count = level_zero.samples_per_axis.div_ceil(side);
    let mut minimum = i16::MAX;
    let mut maximum = i16::MIN;
    let mut saw_elevation = false;
    for y in 0..page_count {
        for x in 0..page_count {
            let key = EnvironmentPageKey {
                layer: PageLayer::Elevation,
                level: 0,
                x,
                y,
            };
            let page = verified_page(provider, key, cancelled)?;
            let EnvironmentPage::Elevation(page) = page.as_ref() else {
                return Err(HeightBoundsError::InvalidPage);
            };
            for &height in &page.geographic_height_centimeters {
                let level = quantize_height(height, package.request.compression);
                minimum = minimum.min(level);
                maximum = maximum.max(level);
                saw_elevation = true;
            }
        }
    }
    if !saw_elevation {
        return Err(HeightBoundsError::NoElevationSamples);
    }

    if let Some(index) = package.environment.hydrology_evidence.as_ref()
        && index.water_model.is_some()
    {
        let page_count = index.samples_per_axis.div_ceil(side);
        for y in 0..page_count {
            for x in 0..page_count {
                let key = EnvironmentPageKey {
                    layer: PageLayer::HydrologyEvidence,
                    level: 0,
                    x,
                    y,
                };
                let page = verified_page(provider, key, cancelled)?;
                let EnvironmentPage::HydrologyEvidence(page) = page.as_ref() else {
                    return Err(HeightBoundsError::InvalidPage);
                };
                accumulate_water_surface_bounds(
                    page,
                    package.request.compression,
                    &mut minimum,
                    &mut maximum,
                )?;
            }
        }
    }
    Ok((minimum, maximum))
}

fn verified_page(
    provider: &dyn EnvironmentPageProvider,
    key: EnvironmentPageKey,
    cancelled: &dyn Fn() -> bool,
) -> Result<Arc<EnvironmentPage>, HeightBoundsError> {
    let page = provider.page(key, cancelled)?;
    if page.key() != key || page.validate().is_err() {
        return Err(HeightBoundsError::InvalidPage);
    }
    Ok(page)
}

fn accumulate_water_surface_bounds(
    page: &HydrologyEvidencePage,
    compression: Ratio,
    minimum: &mut i16,
    maximum: &mut i16,
) -> Result<(), HeightBoundsError> {
    let model = page
        .water_model
        .as_ref()
        .ok_or(HeightBoundsError::InvalidPage)?;
    for surface in model.surface_level_centimeters.iter().flatten() {
        let level = quantize_height(*surface, compression);
        *minimum = (*minimum).min(level);
        *maximum = (*maximum).max(level);
    }
    Ok(())
}

fn quantize_height(height: i32, compression: Ratio) -> i16 {
    let denominator =
        i64::from(ELEVATION_LEVEL_CENTIMETERS).saturating_mul(i64::from(compression.numerator));
    let level = i64::from(height).saturating_mul(i64::from(compression.denominator)) / denominator;
    level.clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16
}
