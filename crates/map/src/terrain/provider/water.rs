//! Shaped provider water: prepared coverage and modeled hydrology are read
//! through the shared shoreline shaping, never as raw nearest cells.
use super::helpers::{load_page, page_index};
use super::water_model::{self, ModeledWater};
use crate::terrain::fallback::{fallback_water, fallback_water_near, water_from_coverage};
use crate::terrain::shoreline::{self, CellGrid};
use crate::terrain::{MapChunkGenerator, Provenance, WaterKind, surface};
use crate::{
    ENVIRONMENT_PAGE_SAMPLES, EnvironmentPage, EnvironmentPageError, EnvironmentPageKey,
    HydrologyEvidenceMethod, HydrologyKind, HydrologyObservation, PageLayer, PreparedEnvironment,
    Ratio, TileSurface,
};
use aoe_core::TileCoord;
use std::sync::Arc;

/// Level-zero cell reader that keeps the last page: the four cells of one
/// shaped sample almost always share it.
struct Cells<'a> {
    generator: &'a MapChunkGenerator,
    cancelled: &'a dyn Fn() -> bool,
    layer: PageLayer,
    page: Option<Arc<EnvironmentPage>>,
}

impl<'a> Cells<'a> {
    fn new(
        generator: &'a MapChunkGenerator,
        layer: PageLayer,
        cancelled: &'a dyn Fn() -> bool,
    ) -> Self {
        Self {
            generator,
            cancelled,
            layer,
            page: None,
        }
    }

    fn page(&mut self, x: i64, y: i64) -> Result<(&EnvironmentPage, usize), EnvironmentPageError> {
        let x = u16::try_from(x).map_err(|_| EnvironmentPageError::Invalid)?;
        let y = u16::try_from(y).map_err(|_| EnvironmentPageError::Invalid)?;
        let key = EnvironmentPageKey {
            layer: self.layer,
            level: 0,
            x: x / u16::from(ENVIRONMENT_PAGE_SAMPLES),
            y: y / u16::from(ENVIRONMENT_PAGE_SAMPLES),
        };
        if self.page.as_ref().is_none_or(|page| page.key() != key) {
            self.page = Some(load_page(self.generator, key, self.cancelled)?);
        }
        let page = self.page.as_deref().ok_or(EnvironmentPageError::Invalid)?;
        let (width, height) = match page {
            EnvironmentPage::Water(page) => (page.width, page.height),
            EnvironmentPage::HydrologyEvidence(page) => (page.width, page.height),
            _ => return Err(EnvironmentPageError::Corrupt),
        };
        Ok((page, page_index(width, height, x, y)?))
    }

    fn coverage(&mut self, x: i64, y: i64) -> Result<(u8, u8), EnvironmentPageError> {
        match self.page(x, y)? {
            (EnvironmentPage::Water(page), index) => Ok((
                page.ocean_coverage_percent[index],
                page.inland_coverage_percent[index],
            )),
            _ => Err(EnvironmentPageError::Corrupt),
        }
    }

    /// Modeled open water of one evidence cell; land and unmodeled kinds are
    /// not shaped.
    fn modeled(&mut self, x: i64, y: i64) -> Result<Option<ModeledWater>, EnvironmentPageError> {
        match self.page(x, y)? {
            (EnvironmentPage::HydrologyEvidence(page), index) => {
                Ok(water_model::modeled_at(page, index)?.filter(|model| open_water(model.0)))
            }
            _ => Err(EnvironmentPageError::Corrupt),
        }
    }
}

const fn open_water(kind: HydrologyKind) -> bool {
    matches!(
        kind,
        HydrologyKind::Ocean | HydrologyKind::Lake | HydrologyKind::River
    )
}

pub(in crate::terrain) struct WaterSample {
    pub water: WaterKind,
    pub provenance: Provenance,
    pub modeled: Option<ModeledWater>,
    pub observation: Option<HydrologyObservation>,
    pub modern_land_cover_class: Option<u8>,
}

/// Shaped categorical and modeled water at one tile, before the modeled
/// surface is applied. `with_cover` also reads the modern land-cover class.
pub(in crate::terrain) fn sample(
    generator: &MapChunkGenerator,
    environment: &PreparedEnvironment,
    tile: TileCoord,
    with_cover: bool,
    cancelled: &dyn Fn() -> bool,
) -> Result<WaterSample, EnvironmentPageError> {
    let key = generator.shoreline_key();
    let (water, provenance) = if let Some(axis) = environment.water_samples_per_axis() {
        let mut cells = Cells::new(generator, PageLayer::Water, cancelled);
        let grid = CellGrid::source(axis, generator.width_tiles);
        shoreline::sample(key, grid, tile, &mut |x, y| {
            let (ocean, inland) = cells.coverage(x, y)?;
            let water = water_from_coverage(ocean, inland);
            Ok((water.0 != WaterKind::None).then_some(water))
        })?
        .unwrap_or((WaterKind::None, Provenance::SourceDerived))
    } else {
        (fallback_water(generator, tile), Provenance::Fallback)
    };
    let Some(index) = &environment.hydrology_evidence else {
        return Ok(WaterSample {
            water,
            provenance,
            modeled: None,
            observation: None,
            modern_land_cover_class: None,
        });
    };
    let (observation, modern_land_cover_class, nearest) = water_model::sample_typed_evidence(
        generator,
        index.samples_per_axis,
        tile,
        with_cover,
        cancelled,
    )?;
    let grid = CellGrid::source(index.samples_per_axis, generator.width_tiles);
    let modeled = if nearest.is_some() && grid.shaped() {
        let mut cells = Cells::new(generator, PageLayer::HydrologyEvidence, cancelled);
        // A rounded-off water corner falls back to categorical coverage; a
        // land correction still applies where no shaped water reaches.
        shoreline::sample(key, grid, tile, &mut |x, y| cells.modeled(x, y))?
            .or(nearest.filter(|model| !open_water(model.0)))
    } else {
        nearest
    };
    Ok(WaterSample {
        water,
        provenance,
        modeled,
        observation,
        modern_land_cover_class,
    })
}

/// Applies the modeled surface and the river-corridor relabel to a sample.
pub(in crate::terrain) fn resolve(
    sample: &WaterSample,
    compression: Ratio,
    game_height_level: &mut i16,
    surface_kind: &mut TileSurface,
) -> (WaterKind, Provenance) {
    let (mut water, mut provenance) = (sample.water, sample.provenance);
    water_model::apply_to_tile(
        sample.modeled,
        compression,
        &mut water,
        game_height_level,
        surface_kind,
        &mut provenance,
    );
    if water == WaterKind::Lake
        && sample.modeled.is_none()
        && sample.observation.is_some_and(|observation| {
            observation.kind == HydrologyKind::River
                && observation.method == HydrologyEvidenceMethod::HydroRiversBufferedCorridor
        })
    {
        water = WaterKind::River;
    }
    (water, provenance)
}

/// Physical water kind only: no elevation, biome or land-cover reads.
pub(in crate::terrain) fn water_kind(
    generator: &MapChunkGenerator,
    tile: TileCoord,
    cancelled: &dyn Fn() -> bool,
) -> Result<WaterKind, EnvironmentPageError> {
    let environment = generator
        .provider_environment
        .as_ref()
        .ok_or(EnvironmentPageError::Invalid)?;
    let compression = generator
        .provider_compression
        .ok_or(EnvironmentPageError::Invalid)?;
    let sample = sample(generator, environment, tile, false, cancelled)?;
    let mut surface_kind = surface::from_heights([0; 4], compression);
    Ok(resolve(&sample, compression, &mut 0, &mut surface_kind).0)
}

/// Conservative: `false` proves no water tile within `radius` tiles.
pub(in crate::terrain) fn water_near(
    generator: &MapChunkGenerator,
    tile: TileCoord,
    radius: i32,
    cancelled: &dyn Fn() -> bool,
) -> Result<bool, EnvironmentPageError> {
    let environment = generator
        .provider_environment
        .as_ref()
        .ok_or(EnvironmentPageError::Invalid)?;
    let categorical = if let Some(axis) = environment.water_samples_per_axis() {
        let mut cells = Cells::new(generator, PageLayer::Water, cancelled);
        let grid = CellGrid::source(axis, generator.width_tiles);
        shoreline::any_wet_near(grid, tile, radius, &mut |x, y| {
            cells.coverage(x, y).map(|coverage| coverage != (0, 0))
        })?
    } else {
        fallback_water_near(generator, tile, radius)
    };
    if categorical {
        return Ok(true);
    }
    let Some(index) = &environment.hydrology_evidence else {
        return Ok(false);
    };
    let mut cells = Cells::new(generator, PageLayer::HydrologyEvidence, cancelled);
    let grid = CellGrid::source(index.samples_per_axis, generator.width_tiles);
    shoreline::any_wet_near(grid, tile, radius, &mut |x, y| {
        cells.modeled(x, y).map(|model| model.is_some())
    })
}
