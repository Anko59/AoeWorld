//! Shared composed-surface material and appearance rules.
use super::*;

pub(super) fn tree_family(biome: Biome) -> ResourceVisualFamily {
    match biome {
        Biome::Boreal => ResourceVisualFamily::Conifer,
        Biome::Tropical => ResourceVisualFamily::Tropical,
        Biome::Woodland | Biome::Savanna => ResourceVisualFamily::DryScrub,
        _ => ResourceVisualFamily::Broadleaf,
    }
}

pub(super) fn appearance(base: Tile, density: DensitySample) -> LandscapeAppearance {
    let height_band = match base.geographic_height_centimeters {
        ..100_000 => NativeHeightBand::Lowland,
        100_000..180_000 => NativeHeightBand::Montane,
        180_000..250_000 => NativeHeightBand::Subalpine,
        250_000..350_000 => NativeHeightBand::Alpine,
        _ => NativeHeightBand::Nival,
    };
    let palette = match base.biome {
        Biome::Temperate => EcologicalPalette::Temperate,
        Biome::Boreal => EcologicalPalette::Boreal,
        Biome::Tropical => EcologicalPalette::Tropical,
        Biome::Woodland => EcologicalPalette::DryScrub,
        Biome::Savanna => EcologicalPalette::Savanna,
        _ => EcologicalPalette::Treeless,
    };
    let exposure = if base.surface.is_cliff()
        || base.material == GroundMaterial::Rock
        || matches!(
            height_band,
            NativeHeightBand::Alpine | NativeHeightBand::Nival
        ) {
        NativeExposure::Exposed
    } else if density.canopy_per_thousand > 0 {
        NativeExposure::Sheltered
    } else {
        NativeExposure::Open
    };
    LandscapeAppearance {
        canopy_strength: density.canopy_per_thousand,
        floor_strength: density.forest_floor_per_thousand,
        palette,
        exposure,
        height_band,
    }
}

/// Rock, snow and ice shores stay as they are; everything else takes the band.
pub(super) const fn soft_ground(material: GroundMaterial) -> bool {
    !matches!(
        material,
        GroundMaterial::Rock | GroundMaterial::Snow | GroundMaterial::Ice
    )
}

/// Snow lies on everything walkable above the snow line, forest floor under
/// canopy, bare rock in the band below the snow. Cold biomes below the snow
/// line are summer grass and heath, not snow.
pub(super) fn material(
    base: Tile,
    appearance: LandscapeAppearance,
    cover: Option<crate::terrain::relief::Cover>,
) -> GroundMaterial {
    use crate::terrain::relief::Cover;
    if base.water != crate::WaterKind::None || base.material == GroundMaterial::Ice {
        return base.material;
    }
    if cover == Some(Cover::Snow) {
        return GroundMaterial::Snow;
    }
    if appearance.floor_strength > 0 {
        return GroundMaterial::ForestFloor;
    }
    if cover == Some(Cover::Rock) {
        return GroundMaterial::Rock;
    }
    match base.material {
        GroundMaterial::ForestFloor => GroundMaterial::DryGrass,
        GroundMaterial::Snow if base.biome == Biome::Boreal => GroundMaterial::TemperateGrass,
        GroundMaterial::Snow => GroundMaterial::DryGrass,
        material => material,
    }
}
