//! Landscape evaluation on dense or provider source inputs. The default
//! policy is the generator's; explicit policies support bounded what-if probes.
use super::{LandscapePoint, MapChunkGenerator, Tile, provider};
#[path = "queries/chunks.rs"]
mod chunks;
#[path = "wildness.rs"]
mod wildness;
use crate::historical_parcels::{LandUse, Parcels, SourceFractions};
use crate::landscape_ecology::{Assessment, Reservations, assess};
use crate::landscape_patches::{DensitySample, Input, Patches};
use crate::{EnvironmentPageError, HistoricalLandUseObservation};
use aoe_core::TileCoord;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LandscapePolicy {
    /// `None` derives each cell's wildness from the source signal. An explicit
    /// value is a what-if probe applied to every cell the queried tile reads.
    pub wildness_per_thousand: Option<u16>,
    pub support_per_thousand: u16,
}

/// Raw coverage remains available: partial coverage is not a tile-sized area
/// observation, and no-data must not become a measured zero cropping fraction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LandscapeSample {
    pub density: DensitySample,
    pub historical_observation: Option<HistoricalLandUseObservation>,
    pub historical_land_use: LandUse,
}

/// Base tiles and cell wildness for one evaluation, plus per-tile values a
/// bounded query shares between neighbouring tiles. Each shared value is a
/// pure function of its tile (a density also of the centre's explicit
/// wildness), so a hit returns exactly what recomputation would.
pub(super) trait LandscapeCache {
    fn base(&self, tile: TileCoord) -> Result<Tile, EnvironmentPageError>;
    fn wildness(&self, cx: i64, cy: i64) -> Result<u16, EnvironmentPageError>;
    fn input(&self, _tile: TileCoord) -> Option<Input> {
        None
    }
    fn set_input(&self, _tile: TileCoord, _input: Input) {}
    fn density(&self, _tile: TileCoord, _explicit: Option<u16>) -> Option<DensitySample> {
        None
    }
    fn set_density(&self, _tile: TileCoord, _explicit: Option<u16>, _density: DensitySample) {}
}

/// Direct source reads with nothing shared.
struct NoCache<'a> {
    generator: &'a MapChunkGenerator,
    cancelled: &'a dyn Fn() -> bool,
}
impl LandscapeCache for NoCache<'_> {
    fn base(&self, tile: TileCoord) -> Result<Tile, EnvironmentPageError> {
        if self.generator.provider.is_some() {
            provider::sample_base_tile(self.generator, tile, self.cancelled)
        } else {
            Ok(self.generator.sample_base_tile(tile))
        }
    }
    fn wildness(&self, cx: i64, cy: i64) -> Result<u16, EnvironmentPageError> {
        self.generator
            .landscape_cell_wildness(cx, cy, self.cancelled)
    }
}

impl MapChunkGenerator {
    /// Evaluate the candidate using only BASE terrain and one shared exclusion
    /// callback, including its neighbors. Callback must be coordinate-stable;
    /// never read decorated terrain/object queries from it. Allocation-free
    /// apart from page-provider loads; caller cancellation and errors propagate.
    /// Wildness/support are modeling inputs, not a new source observation.
    pub fn evaluate_landscape_with_cancel(
        &self,
        tile: TileCoord,
        policy: LandscapePolicy,
        reservations: &dyn Fn(TileCoord) -> Reservations,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<LandscapeSample>, EnvironmentPageError> {
        self.evaluate_landscape_inner(
            tile,
            &|_, _| policy,
            &|position| Ok(reservations(position)),
            cancelled,
        )
        .map(|sample| sample.map(|(candidate, _)| candidate))
    }

    fn evaluate_landscape_inner(
        &self,
        tile: TileCoord,
        policy_at: &dyn Fn(TileCoord, Tile) -> LandscapePolicy,
        reservations: &dyn Fn(TileCoord) -> Result<Reservations, EnvironmentPageError>,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<(LandscapeSample, Tile)>, EnvironmentPageError> {
        self.evaluate_landscape_inner_with_base(
            tile,
            policy_at,
            reservations,
            cancelled,
            &NoCache {
                generator: self,
                cancelled,
            },
        )
    }

    fn evaluate_landscape_inner_with_base(
        &self,
        tile: TileCoord,
        policy_at: &dyn Fn(TileCoord, Tile) -> LandscapePolicy,
        reservations: &dyn Fn(TileCoord) -> Result<Reservations, EnvironmentPageError>,
        cancelled: &dyn Fn() -> bool,
        cache: &dyn LandscapeCache,
    ) -> Result<Option<(LandscapeSample, Tile)>, EnvironmentPageError> {
        if cancelled() {
            return Err(EnvironmentPageError::Cancelled);
        }
        let base_at = &|tile: TileCoord| cache.base(tile);
        if !self.landscape_in_bounds(tile) {
            return Ok(None);
        }
        let patches = Patches::new(self.geography_key, self.procedural_seed);
        let parcels = Parcels::new(self.geography_key, self.procedural_seed);
        let (center, observation, base) = self.landscape_assessment(
            tile,
            policy_at,
            &parcels,
            reservations(tile)?,
            cancelled,
            base_at,
        )?;
        cache.set_input(tile, center.input);
        let explicit = policy_at(tile, base).wildness_per_thousand;
        let wildness = |cx, cy| explicit.map_or_else(|| cache.wildness(cx, cy), Ok);
        // Neighbour inputs and raw densities are pure functions of their tile
        // and of `explicit`, so a bounded query computes each one once.
        let input_at = |neighbor: TileCoord| {
            if let Some(input) = cache.input(neighbor) {
                return Ok(input);
            }
            let input = self
                .landscape_assessment(
                    neighbor,
                    policy_at,
                    &parcels,
                    reservations(neighbor)?,
                    cancelled,
                    base_at,
                )?
                .0
                .input;
            cache.set_input(neighbor, input);
            Ok(input)
        };
        let raw = patches.try_density_sample(tile.x, tile.y, center.input, &wildness)?;
        cache.set_density(tile, explicit, raw);
        let density = Patches::try_filter_singleton(tile.x, tile.y, center.input, raw, |x, y| {
            let neighbor = TileCoord::new(x, y);
            if !self.landscape_in_bounds(neighbor) {
                let mut excluded = center.input;
                excluded.eligible = false;
                return patches.try_density_sample(x, y, excluded, &wildness);
            }
            if let Some(density) = cache.density(neighbor, explicit) {
                return Ok(density);
            }
            let density = patches.try_density_sample(x, y, input_at(neighbor)?, &wildness)?;
            cache.set_density(neighbor, explicit, density);
            Ok(density)
        })?;
        Ok(Some((
            LandscapeSample {
                density,
                historical_observation: observation,
                historical_land_use: center.historical_land_use,
            },
            base,
        )))
    }

    fn landscape_in_bounds(&self, tile: TileCoord) -> bool {
        tile.x >= 0 && tile.y >= 0 && tile.x < self.width_tiles && tile.y < self.width_tiles
    }

    fn landscape_assessment(
        &self,
        tile: TileCoord,
        policy_at: &dyn Fn(TileCoord, Tile) -> LandscapePolicy,
        parcels: &Parcels,
        reservations: Reservations,
        cancelled: &dyn Fn() -> bool,
        base_at: &dyn Fn(TileCoord) -> Result<Tile, EnvironmentPageError>,
    ) -> Result<(Assessment, Option<HistoricalLandUseObservation>, Tile), EnvironmentPageError>
    {
        if cancelled() {
            return Err(EnvironmentPageError::Cancelled);
        }
        let base = base_at(tile)?;
        let observation = self.land_use_observation_at(tile, cancelled)?;
        let history = match observation {
            None => LandUse::Unobserved,
            Some(observed)
                if observed
                    .coverage
                    .is_some_and(|coverage| coverage.valid_land_percent == 0) =>
            {
                let coverage = observed.coverage.ok_or(EnvironmentPageError::Invalid)?;
                if coverage.land_percent > 0 || coverage.nodata_percent > 0 {
                    LandUse::Unobserved
                } else {
                    LandUse::Nonland
                }
            }
            Some(observed) => {
                let fractions =
                    SourceFractions::new(observed.crop_percent, observed.grazing_percent)
                        .map_err(|_| EnvironmentPageError::Corrupt)?;
                parcels.sample(tile.x, tile.y, Some(fractions), true)
            }
        };
        let policy = policy_at(tile, base);
        let assessment = assess(base, policy.support_per_thousand, history, reservations)
            .map_err(|_| EnvironmentPageError::Invalid)?;
        Ok((assessment, observation, base))
    }
}

impl MapChunkGenerator {
    /// Raw year-600 land-use observation at a tile, from pages or dense data.
    pub(super) fn land_use_observation_at(
        &self,
        tile: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<HistoricalLandUseObservation>, EnvironmentPageError> {
        if self.provider.is_none() {
            return Ok(self
                .historical_land_use
                .as_ref()
                .and_then(|field| field.at_observation(tile, self.width_tiles)));
        }
        match self
            .provider_environment
            .as_ref()
            .and_then(|e| e.historical_samples_per_axis())
        {
            Some(samples) => provider::sample_land_use_observation(self, samples, tile, cancelled),
            None => Ok(None),
        }
    }

    /// Raw source PNV class at a tile, from the same field the base biome uses.
    /// Missing vegetation fields return `None`; page failures propagate.
    pub(in crate::terrain) fn source_biome_class_with_cancel(
        &self,
        position: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<u8>, EnvironmentPageError> {
        if cancelled() {
            return Err(EnvironmentPageError::Cancelled);
        }
        if self.provider.is_none() {
            return Ok(self
                .biome
                .as_ref()
                .and_then(|biome| biome.class_at(position, self.width_tiles)));
        }
        let axis = self
            .provider_environment
            .as_ref()
            .ok_or(EnvironmentPageError::Invalid)?
            .vegetation_samples_per_axis();
        match axis {
            Some(axis) => provider::sample_biome_class(self, axis, position, cancelled),
            None => Ok(None),
        }
    }
}

#[path = "queries/tests.rs"]
#[cfg(test)]
mod tests;
