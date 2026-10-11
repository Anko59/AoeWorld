//! Real-world forest signal of one 192-tile landscape cell. "Wildness" is the
//! share of the cell's potential forest that survived in the year 600, in
//! thousandths: the potential-natural-vegetation biome says whether forest
//! could grow, and the HYDE crop, grazing and population fields say how much
//! of it people had cleared. It is a modeling input for stand size and
//! number, not an observation of where individual stands stood.
use super::{MapChunkGenerator, provider};
use crate::landscape_patches::CELL_TILES;
use crate::terrain::unsigned_noise;
use crate::{Biome, EnvironmentPageError, HistoricalLandUseObservation};
use aoe_core::TileCoord;

/// Cleared share (crop + grazing percent, plus a quarter of the population
/// per square kilometre) to surviving forest, piecewise linear.
const CLEARED_TO_WILDNESS: [(i64, i64); 10] = [
    (0, 1000),
    (2, 860),
    (5, 680),
    (8, 540),
    (12, 420),
    (20, 260),
    (35, 120),
    (60, 40),
    (100, 0),
    (i64::MAX, 0),
];

/// Wildness of one probe. Without a historical observation the biome's own
/// default applies, shifted by `variation` (-300..=300) so that maps without
/// history still have more and less forested regions.
pub(crate) fn probe_wildness(
    biome: Biome,
    history: Option<HistoricalLandUseObservation>,
    variation: i64,
) -> u16 {
    let potential = match biome {
        Biome::Temperate | Biome::Boreal | Biome::Tropical | Biome::Savanna => 1000,
        Biome::Woodland => 550,
        _ => return 0,
    };
    let observed = history.filter(|observed| {
        observed
            .coverage
            .is_none_or(|coverage| coverage.valid_land_percent > 0)
    });
    let survived = match observed {
        Some(observed) => {
            let cleared = i64::from(observed.crop_percent)
                + i64::from(observed.grazing_percent)
                + i64::from(observed.population_pressure) / 4;
            let upper = CLEARED_TO_WILDNESS
                .iter()
                .position(|(limit, _)| cleared < *limit)
                .unwrap_or(CLEARED_TO_WILDNESS.len() - 1)
                .max(1);
            let (low, high) = (CLEARED_TO_WILDNESS[upper - 1], CLEARED_TO_WILDNESS[upper]);
            if high.0 == i64::MAX {
                0
            } else {
                low.1 + (high.1 - low.1) * (cleared - low.0) / (high.0 - low.0)
            }
        }
        None => {
            match biome {
                // Scattered savanna trees keep their full density.
                Biome::Savanna => 1000,
                Biome::Tropical => 850 + variation,
                Biome::Boreal => 750 + variation,
                _ => 500 + variation,
            }
        }
    };
    (survived.clamp(0, 1000) * potential / 1000) as u16
}

impl MapChunkGenerator {
    /// Mean of four probes 48 tiles around the cell centre; probes outside
    /// the map are skipped. Bounded: at most four biome and four history
    /// lookups, no allocation; page failures and cancellation propagate.
    pub(in crate::terrain) fn landscape_cell_wildness(
        &self,
        cx: i64,
        cy: i64,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<u16, EnvironmentPageError> {
        if cancelled() {
            return Err(EnvironmentPageError::Cancelled);
        }
        let cell = |value: i64| value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
        let variation = (unsigned_noise(self.geography_key, b"forest-wildness", cell(cx), cell(cy))
            % 601) as i64
            - 300;
        let (mut total, mut probes) = (0_u32, 0_u32);
        for (dx, dy) in [(-48, -48), (48, -48), (-48, 48), (48, 48)] {
            let x = cx.saturating_mul(CELL_TILES) + CELL_TILES / 2 + dx;
            let y = cy.saturating_mul(CELL_TILES) + CELL_TILES / 2 + dy;
            let width = i64::from(self.width_tiles);
            if x < 0 || y < 0 || x >= width || y >= width {
                continue;
            }
            let tile = TileCoord::new(x as i32, y as i32);
            let biome = if self.provider.is_some() {
                provider::sample_biome(self, tile, cancelled)?
            } else {
                self.sample_biome(tile)
            };
            let history = self.land_use_observation_at(tile, cancelled)?;
            total += u32::from(probe_wildness(biome, history, variation));
            probes += 1;
        }
        Ok(total.checked_div(probes).unwrap_or(0) as u16)
    }
}
