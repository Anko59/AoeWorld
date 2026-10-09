use super::{
    MapChunkGenerator,
    helpers::{load_page, page_index, source_coordinate},
};
use crate::{
    ENVIRONMENT_PAGE_SAMPLES, EnvironmentPage, EnvironmentPageError, EnvironmentPageKey, PageLayer,
    land_use::HistoricalLandUseObservation,
};
use aoe_core::TileCoord;

/// Reads raw source quantities and coverage before applying legacy clearing policy.
pub(in crate::terrain) fn sample_land_use_observation(
    generator: &MapChunkGenerator,
    samples: u16,
    tile: TileCoord,
    cancelled: &dyn Fn() -> bool,
) -> Result<Option<HistoricalLandUseObservation>, EnvironmentPageError> {
    let (source_x, source_y) = source_coordinate(tile.x, tile.y, samples, generator.width_tiles)?;
    let page = load_page(
        generator,
        EnvironmentPageKey {
            layer: PageLayer::HistoricalLandUse,
            level: 0,
            x: source_x / u16::from(ENVIRONMENT_PAGE_SAMPLES),
            y: source_y / u16::from(ENVIRONMENT_PAGE_SAMPLES),
        },
        cancelled,
    )?;
    let page = match page.as_ref() {
        EnvironmentPage::HistoricalLandUse(page) => page,
        _ => return Err(EnvironmentPageError::Corrupt),
    };
    let index = page_index(page.width, page.height, source_x, source_y)?;
    Ok(Some(HistoricalLandUseObservation {
        crop_percent: page.crop_percent[index],
        grazing_percent: page.grazing_percent[index],
        population_pressure: page.population_pressure_per_square_kilometer[index],
        coverage: if page.coverage.is_empty() {
            None
        } else {
            Some(page.coverage[index])
        },
    }))
}

#[cfg(test)]
mod tests;
