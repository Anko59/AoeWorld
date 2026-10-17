//! Provider water: mapped lakes keep their sampled outline; ocean coverage
//! uses the shared shoreline shaping.
use super::helpers::{load_page, page_index};
use super::water_model::{self, ModeledWater};
use crate::terrain::fallback::{
    fallback_water, fallback_water_near, open_coverage, water_from_coverage,
};
use crate::terrain::shoreline::{self, CellGrid};
use crate::terrain::{MapChunkGenerator, Provenance, WaterKind, surface};
use crate::{
    ENVIRONMENT_PAGE_SAMPLES, EnvironmentPage, EnvironmentPageError, EnvironmentPageKey,
    HydrologyKind, HydrologyObservation, PageLayer, PreparedEnvironment, Ratio, TileSurface,
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
        // Mapped inland water owns its outline. Coarse HYDE coverage must
        // not fill mapped bays or islands, including nearby-water queries.
        let mapped = self
            .generator
            .provider_environment
            .as_ref()
            .is_some_and(|environment| environment.hydrology_evidence.is_some());
        match self.page(x, y)? {
            (EnvironmentPage::Water(page), index) => Ok((
                page.ocean_coverage_percent[index],
                if mapped {
                    0
                } else {
                    page.inland_coverage_percent[index]
                },
            )),
            _ => Err(EnvironmentPageError::Corrupt),
        }
    }

    /// Modeled open water of one evidence cell; land and unmodeled kinds are
    /// not shaped.
    fn modeled(&mut self, x: i64, y: i64) -> Result<Option<ModeledWater>, EnvironmentPageError> {
        match self.page(x, y)? {
            (EnvironmentPage::HydrologyEvidence(page), index) => Ok(open_evidence_at(page, index)?
                .filter(|model| open_water(model.0) && model.0 != HydrologyKind::River)),
            _ => Err(EnvironmentPageError::Corrupt),
        }
    }
}

/// Unmodeled lake/ocean observations remain open-water evidence. Use the
/// same reader for shaped neighbors and the nearest cell at a river mouth.
fn open_evidence_at(
    page: &crate::HydrologyEvidencePage,
    index: usize,
) -> Result<Option<ModeledWater>, EnvironmentPageError> {
    let modeled = water_model::modeled_at(page, index)?;
    if modeled.is_some() {
        return Ok(modeled);
    }
    let observation = page
        .observation(index)
        .map_err(|_| EnvironmentPageError::Corrupt)?;
    Ok(observed_open_water(observation))
}

fn observed_open_water(observation: HydrologyObservation) -> Option<ModeledWater> {
    matches!(observation.kind, HydrologyKind::Lake | HydrologyKind::Ocean).then_some((
        observation.kind,
        None,
        crate::WaterModelProvenance::EvidenceOnly,
    ))
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
    /// The tile is river water of a narrow stream: wadeable shallows.
    pub wadeable: bool,
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
            wadeable: false,
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
    let nearest = nearest
        .or_else(|| observation.and_then(observed_open_water))
        .filter(|model| model.0 != HydrologyKind::River);
    // Lake bays and islands belong to the mapped mask, not procedural warp.
    let mut modeled =
        if nearest.is_some_and(|model| model.0 == HydrologyKind::Ocean) && grid.shaped() {
            let mut cells = Cells::new(generator, PageLayer::HydrologyEvidence, cancelled);
            // A rounded-off water corner falls back to ocean coverage; a
            // land correction still applies where no shaped water reaches.
            shoreline::sample(key, grid, tile, &mut |x, y| cells.modeled(x, y))?
                .or(nearest.filter(|model| !open_water(model.0)))
        } else {
            nearest
        };
    // Lake/ocean evidence and categorical coverage also protect mouths when
    // no water model exists. Explicit land corrections retain their precedence.
    let protected = matches!(water, WaterKind::Lake | WaterKind::Ocean)
        || modeled.is_some_and(|m| {
            matches!(m.0, HydrologyKind::Lake | HydrologyKind::Ocean)
                || (m.0 == HydrologyKind::Land
                    && m.2 == crate::WaterModelProvenance::GeographicCorrection)
        });
    let channel = if protected {
        None
    } else {
        super::rivers::sample(generator, tile, 0, cancelled)?
    };
    let wadeable = channel.is_some_and(|channel| channel.wadeable);
    if channel.is_some() {
        // Source channels retain local terrain height. A sampled kilometre-cell
        // water level would create cliffs at every channel and cell boundary.
        modeled = Some((
            HydrologyKind::River,
            None,
            crate::WaterModelProvenance::ModelledRiverSurface,
        ));
    }
    Ok(WaterSample {
        water,
        provenance,
        modeled,
        observation,
        modern_land_cover_class,
        wadeable,
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
    if super::rivers::sample(generator, tile, radius, cancelled)?.is_some() {
        return Ok(true);
    }
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

/// Conservative: `true` proves only lake or ocean water within `radius`
/// tiles. Modeled hydrology decides wherever it is present, because a cited
/// correction can dry categorical water.
pub(in crate::terrain) fn open_water_near(
    generator: &MapChunkGenerator,
    tile: TileCoord,
    radius: i32,
    cancelled: &dyn Fn() -> bool,
) -> Result<bool, EnvironmentPageError> {
    let environment = generator
        .provider_environment
        .as_ref()
        .ok_or(EnvironmentPageError::Invalid)?;
    if let Some(index) = &environment.hydrology_evidence {
        let mut cells = Cells::new(generator, PageLayer::HydrologyEvidence, cancelled);
        let grid = CellGrid::source(index.samples_per_axis, generator.width_tiles);
        return shoreline::all_wet_near(grid, tile, radius, &mut |x, y| {
            cells.modeled(x, y).map(|model| {
                model.is_some_and(|model| {
                    matches!(model.0, HydrologyKind::Ocean | HydrologyKind::Lake)
                })
            })
        });
    }
    let Some(axis) = environment.water_samples_per_axis() else {
        return Ok(false);
    };
    let mut cells = Cells::new(generator, PageLayer::Water, cancelled);
    let grid = CellGrid::source(axis, generator.width_tiles);
    shoreline::all_wet_near(grid, tile, radius, &mut |x, y| {
        cells
            .coverage(x, y)
            .map(|(ocean, inland)| open_coverage(ocean, inland))
    })
}
