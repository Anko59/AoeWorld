//! Shared landscape eligibility from undecorated terrain and historical parcels.
//! No published recipe invokes this adapter yet. Region/support are explicit
//! modeling policy, not modern land-cover or an inferred historical observation.
use crate::historical_parcels::LandUse;
use crate::landscape_patches::{Fitness, Input, Mode, Region};
use crate::{Biome, GroundMaterial, Tile, WaterKind};

/// Reservations must be realized before descriptor singleton filtering, so a
/// later tree-only removal cannot disagree with forest floor/canopy occupancy.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Reservations {
    pub route: bool,
    pub resource_approach: bool,
    pub start: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EcologyError {
    SupportOutOfRange,
}

/// Observation status survives the modeling adapter. Unobserved is not a
/// measured zero; it leaves procedural potential vegetation eligible, subject
/// to physical terrain and reservations, without manufacturing source evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Assessment {
    pub input: Input,
    pub historical_land_use: LandUse,
}

pub fn assess(
    base: Tile,
    region: Region,
    support_per_thousand: u16,
    historical_land_use: LandUse,
    reservations: Reservations,
) -> Result<Assessment, EcologyError> {
    if support_per_thousand > 1000 {
        return Err(EcologyError::SupportOutOfRange);
    }
    let mode = match base.biome {
        Biome::Temperate | Biome::Boreal | Biome::Tropical | Biome::Woodland => {
            Mode::Forest(region)
        }
        Biome::Savanna => Mode::SparseSavanna,
        Biome::Steppe | Biome::Desert | Biome::Tundra | Biome::Alpine | Biome::Polar => {
            Mode::Treeless
        }
    };
    // Walkable bare rock, sand or ice is not forest-suitable just because the
    // coarse biome cell names a forest. Snow supports boreal trees; source slope
    // restrictions are already represented by the undecorated surface.
    let suitable = base.water == WaterKind::None
        && base.passable
        && base.surface.walkable()
        && !matches!(
            base.material,
            GroundMaterial::Rock | GroundMaterial::Sand | GroundMaterial::Ice
        )
        && mode != Mode::Treeless;
    let excluded = matches!(
        historical_land_use,
        LandUse::Crop | LandUse::Grazing | LandUse::Nonland
    ) || reservations.route
        || reservations.resource_approach
        || reservations.start;
    Ok(Assessment {
        input: Input {
            mode,
            fitness: Fitness {
                suitable,
                support_per_thousand,
            },
            eligible: !excluded,
        },
        historical_land_use,
    })
}

#[path = "ecology/tests.rs"]
#[cfg(test)]
mod tests;
