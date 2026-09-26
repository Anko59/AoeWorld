use super::helpers::{load_page, page_index, source_coordinate};
use crate::{
    ENVIRONMENT_PAGE_SAMPLES, EnvironmentPage, EnvironmentPageError, EnvironmentPageKey,
    HydrologyKind, HydrologyObservation, PageLayer, Provenance, Ratio, TileSurface, WaterKind,
    WaterModelProvenance,
};
use super::surface;
use aoe_core::TileCoord;

pub(super) fn sample_typed_evidence(
    generator: &super::MapChunkGenerator,
    samples: u16,
    tile: TileCoord,
    cancelled: &dyn Fn() -> bool,
) -> Result<
    (
        Option<HydrologyObservation>,
        Option<u8>,
        Option<(HydrologyKind, Option<i32>, WaterModelProvenance)>,
    ),
    EnvironmentPageError,
> {
    let (source_x, source_y) =
        source_coordinate(tile.x, tile.y, samples, generator.width_tiles)?;
    let x = source_x / u16::from(ENVIRONMENT_PAGE_SAMPLES);
    let y = source_y / u16::from(ENVIRONMENT_PAGE_SAMPLES);
    let observation_page = load_page(
        generator,
        EnvironmentPageKey {
            layer: PageLayer::HydrologyEvidence,
            level: 0,
            x,
            y,
        },
        cancelled,
    )?;
    let observation_page = match observation_page.as_ref() {
        EnvironmentPage::HydrologyEvidence(page) => page,
        _ => return Err(EnvironmentPageError::Corrupt),
    };
    let index = page_index(
        observation_page.width,
        observation_page.height,
        source_x,
        source_y,
    )?;
    let observation = observation_page
        .observation(index)
        .map_err(|_| EnvironmentPageError::Corrupt)?;
    let cover_page = load_page(
        generator,
        EnvironmentPageKey {
            layer: PageLayer::ModernLandCover,
            level: 0,
            x,
            y,
        },
        cancelled,
    )?;
    let cover_page = match cover_page.as_ref() {
        EnvironmentPage::ModernLandCover(page) => page,
        _ => return Err(EnvironmentPageError::Corrupt),
    };
    if (cover_page.width, cover_page.height) != (observation_page.width, observation_page.height) {
        return Err(EnvironmentPageError::Corrupt);
    }
    let class = cover_page
        .class_at(index)
        .map_err(|_| EnvironmentPageError::Corrupt)?;
    let modeled = observation_page
        .water_model
        .as_ref()
        .map(|model| {
            let kind = model
                .kind_at(index)
                .map_err(|_| EnvironmentPageError::Corrupt)?;
            let level = *model
                .surface_level_centimeters
                .get(index)
                .ok_or(EnvironmentPageError::Corrupt)?;
            let provenance = model
                .provenance
                .get(index)
                .copied()
                .ok_or(EnvironmentPageError::Corrupt)?
                .try_into()
                .map_err(|_| EnvironmentPageError::Corrupt)?;
            Ok::<_, EnvironmentPageError>((kind, level, provenance))
        })
        .transpose()?;
    Ok((Some(observation), Some(class), modeled))
}

pub(super) fn apply_to_tile(
    modeled: Option<(HydrologyKind, Option<i32>, WaterModelProvenance)>,
    compression: Ratio,
    water: &mut WaterKind,
    game_height_level: &mut i16,
    surface_kind: &mut TileSurface,
    water_provenance: &mut Provenance,
) {
    let Some((kind, surface_level, provenance)) = modeled else {
        return;
    };
    match kind {
        HydrologyKind::Ocean => {
            *water = WaterKind::Ocean;
            if let Some(level) = surface_level {
                *game_height_level = super::super::quantize_game_height(level, compression);
                *surface_kind = surface::from_heights([level; 4], compression);
            }
            *water_provenance = match provenance {
                WaterModelProvenance::EvidenceOnly => Provenance::SourceDerived,
                WaterModelProvenance::GeographicCorrection => Provenance::HistoricallyCorrected,
                WaterModelProvenance::ModelledOceanSurface => Provenance::ModelDerived,
                WaterModelProvenance::ModelledLakeSurface
                | WaterModelProvenance::ModelledRiverSurface => Provenance::SourceDerived,
                WaterModelProvenance::ModelledJunctionSurface => Provenance::ModelDerived,
            };
        }
        HydrologyKind::Lake => {
            *water = WaterKind::Lake;
            if let Some(level) = surface_level {
                *game_height_level = super::super::quantize_game_height(level, compression);
                *surface_kind = surface::from_heights([level; 4], compression);
            }
            *water_provenance = match provenance {
                WaterModelProvenance::EvidenceOnly => Provenance::SourceDerived,
                WaterModelProvenance::ModelledLakeSurface => Provenance::ModelDerived,
                WaterModelProvenance::GeographicCorrection => Provenance::HistoricallyCorrected,
                WaterModelProvenance::ModelledOceanSurface
                | WaterModelProvenance::ModelledRiverSurface
                | WaterModelProvenance::ModelledJunctionSurface => Provenance::SourceDerived,
            };
        }
        HydrologyKind::River => {
            *water = WaterKind::River;
            if let Some(level) = surface_level {
                *game_height_level = super::super::quantize_game_height(level, compression);
                *surface_kind = surface::from_heights([level; 4], compression);
            }
            *water_provenance = match provenance {
                WaterModelProvenance::EvidenceOnly => Provenance::SourceDerived,
                WaterModelProvenance::ModelledLakeSurface => Provenance::ModelDerived,
                WaterModelProvenance::GeographicCorrection => Provenance::HistoricallyCorrected,
                WaterModelProvenance::ModelledOceanSurface => Provenance::SourceDerived,
                WaterModelProvenance::ModelledJunctionSurface
                | WaterModelProvenance::ModelledRiverSurface => Provenance::ModelDerived,
            };
        }
        HydrologyKind::Land if provenance == WaterModelProvenance::GeographicCorrection => {
            *water = WaterKind::None;
            *water_provenance = Provenance::HistoricallyCorrected;
        }
        HydrologyKind::Land
        | HydrologyKind::Shallow
        | HydrologyKind::Reservoir
        | HydrologyKind::UnknownWater
        | HydrologyKind::RegulatedLake
        | HydrologyKind::NoEvidence => {}
    }
}
