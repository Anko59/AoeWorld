use super::{HistoricalCoverage, HistoricalLandUse};
use crate::ENVIRONMENT_PAGE_SAMPLES;
use aoe_core::TileCoord;

/// Raw historical quantities and their source coverage, without a clearing policy.
/// Missing legacy coverage remains distinct from observed zero and source nodata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HistoricalLandUseObservation {
    pub crop_percent: u8,
    pub grazing_percent: u8,
    pub population_pressure: u16,
    pub coverage: Option<HistoricalCoverage>,
}

impl HistoricalLandUse {
    pub(crate) fn at_observation(
        &self,
        tile: TileCoord,
        width_tiles: i32,
    ) -> Option<HistoricalLandUseObservation> {
        let tile_axis = u64::try_from(width_tiles.checked_sub(1)?).ok()?;
        let source_axis = u64::from(self.samples_per_axis.checked_sub(1)?);
        let x =
            u16::try_from((u64::try_from(tile.x).ok()? * source_axis + tile_axis / 2) / tile_axis)
                .ok()?;
        let y =
            u16::try_from((u64::try_from(tile.y).ok()? * source_axis + tile_axis / 2) / tile_axis)
                .ok()?;
        let page_size = u16::from(ENVIRONMENT_PAGE_SAMPLES);
        let page = self.pages.get(&(x / page_size, y / page_size))?;
        let local_x = usize::from(x % page_size);
        let local_y = usize::from(y % page_size);
        if local_x >= usize::from(page.width) || local_y >= usize::from(page.height) {
            return None;
        }
        let index = local_y * usize::from(page.width) + local_x;
        Some(HistoricalLandUseObservation {
            crop_percent: page.crop_percent[index],
            grazing_percent: page.grazing_percent[index],
            population_pressure: page.population_pressure_per_square_kilometer[index],
            coverage: if page.coverage.is_empty() {
                None
            } else {
                Some(page.coverage[index])
            },
        })
    }
}

#[cfg(test)]
mod tests;
