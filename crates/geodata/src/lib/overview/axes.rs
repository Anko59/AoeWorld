use super::*;
use serde::{Deserialize, Serialize};

/// Native preparation options, not part of the map request or package wire schema.
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
        elevation: 1024,
        vegetation: 128,
        water: 128,
        historical: 1024,
    };

    pub const fn legacy(axis: u16, historical: u16) -> Self {
        Self {
            elevation: axis,
            vegetation: axis,
            water: axis,
            historical,
        }
    }

    pub(crate) fn validate(self, request: MapRequest, explicit: bool) -> Result<(), GeodataError> {
        if !request.detail_profile.uses_landscape_axes()
            && (self.water != self.elevation || self.vegetation != self.elevation)
        {
            return Err(GeodataError::Preparation(
                "independent overview axes require LandscapeV2",
            ));
        }
        let elevation_cap = if explicit && request.detail_profile.uses_landscape_axes() {
            elevation::MAX_LANDSCAPE_DIRECT_ELEVATION_SAMPLES_PER_AXIS
        } else {
            MAX_DIRECT_ELEVATION_SAMPLES_PER_AXIS
        };
        if !(2..=elevation_cap).contains(&self.elevation)
            || !(2..=MAX_DIRECT_ELEVATION_SAMPLES_PER_AXIS).contains(&self.water)
            || !(2..=MAX_DIRECT_ELEVATION_SAMPLES_PER_AXIS).contains(&self.vegetation)
            || !(2..=MAX_HISTORICAL_GRID_SAMPLES_PER_AXIS).contains(&self.historical)
        {
            return Err(GeodataError::Preparation(
                "overview field axes are outside direct bounds",
            ));
        }
        Ok(())
    }
}
