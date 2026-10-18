//! Native source-worker preparation options shared by the server and the
//! geographic worker. They are not part of `MapRequest` or the game wire.

use serde::{Deserialize, Serialize};

/// Canonical overview axis for every landscape field: elevation, water,
/// potential vegetation, history and vector hydrology.
pub const LANDSCAPE_OVERVIEW_SAMPLES_PER_AXIS: u16 = 1024;

/// Independent per-field overview axes requested from the source worker.
/// Overview packages always use [`OverviewFieldAxes::LANDSCAPE`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OverviewFieldAxes {
    pub elevation: u16,
    pub vegetation: u16,
    pub water: u16,
    pub historical: u16,
}

impl OverviewFieldAxes {
    pub const LANDSCAPE: Self = Self {
        elevation: LANDSCAPE_OVERVIEW_SAMPLES_PER_AXIS,
        vegetation: LANDSCAPE_OVERVIEW_SAMPLES_PER_AXIS,
        water: LANDSCAPE_OVERVIEW_SAMPLES_PER_AXIS,
        historical: LANDSCAPE_OVERVIEW_SAMPLES_PER_AXIS,
    };

    /// One elevation/vegetation/water axis, as sampled by the overview
    /// context of detailed preparation.
    pub const fn coupled(axis: u16, historical: u16) -> Self {
        Self {
            elevation: axis,
            vegetation: axis,
            water: axis,
            historical,
        }
    }
}

/// Native overview-only hydrology selection; never part of `MapRequest`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverviewHydrologyMode {
    #[default]
    None,
    Automatic,
    Vectors,
}

impl OverviewHydrologyMode {
    pub fn is_none(&self) -> bool {
        *self == Self::None
    }
}

/// Locks in an overview package: ETOPO, Natural Earth, potential
/// biome and HYDE sources, as listed by the worker's overview source catalog.
pub const LANDSCAPE_OVERVIEW_SOURCE_LOCKS: usize = 7;
/// Locks added by vector hydrology: HydroLAKES and HydroRIVERS.
pub const VECTOR_HYDROLOGY_SOURCE_LOCKS: usize = 2;

/// Locks of one-degree Copernicus DEM tiles start with this identifier prefix.
const ELEVATION_TILE_LOCK_PREFIX: &str = "copernicus-dem-";

/// Source locks of an overview package without the Copernicus GLO-90 tiles
/// averaged into its elevation, whose number follows the footprint.
pub fn overview_base_lock_count(locks: &[crate::SourceLock]) -> usize {
    locks
        .iter()
        .filter(|lock| !lock.id.starts_with(ELEVATION_TILE_LOCK_PREFIX))
        .count()
}

#[cfg(test)]
#[path = "tests/preparation.rs"]
mod tests;
