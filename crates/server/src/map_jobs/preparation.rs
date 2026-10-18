use aoe_map::{MapEstimate, MapRequest};
use serde::{Deserialize, Serialize};

const MAX_CREATOR_DETAILED_SIDE_METERS: u64 = 120_000;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PreparationPreference {
    #[default]
    Automatic,
    Overview,
    Detailed,
}

/// Worker options are shared with the geographic worker through `aoe_map`.
pub(crate) use aoe_map::{OverviewFieldAxes, OverviewHydrologyMode as HydrologyMode};

#[derive(Clone, Copy, Debug, Deserialize)]
pub(crate) struct CreationRequest {
    #[serde(flatten)]
    pub request: MapRequest,
    #[serde(default)]
    pub preparation: PreparationPreference,
    #[serde(default = "creator_hydrology")]
    pub hydrology_mode: HydrologyMode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PreparationMode {
    ProceduralFallback,
    Overview,
    Detailed,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub(crate) struct PreparationPlan {
    pub mode: PreparationMode,
    pub samples_per_axis: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field_axes: Option<OverviewFieldAxes>,
    #[serde(skip_serializing_if = "HydrologyMode::is_none")]
    pub hydrology_mode: HydrologyMode,
    pub geographic_millimeters_per_sample: Option<u64>,
    pub explanation: &'static str,
}

fn creator_hydrology() -> HydrologyMode {
    HydrologyMode::Automatic
}

impl PreparationPlan {
    pub fn fallback() -> Self {
        Self {
            mode: PreparationMode::ProceduralFallback,
            samples_per_axis: 0,
            field_axes: None,
            hydrology_mode: HydrologyMode::None,
            geographic_millimeters_per_sample: None,
            explanation: "No geographic worker configured; terrain is procedural fallback.",
        }
    }

    pub fn resolve(input: CreationRequest, worker_available: bool) -> Result<Self, String> {
        let request = input
            .request
            .normalized()
            .map_err(|error| error.to_string())?;
        let estimate = request.estimate().map_err(|error| error.to_string())?;
        if !worker_available {
            if input.hydrology_mode == HydrologyMode::Vectors {
                return Err("With rivers requires a configured geographic worker; procedural fallback cannot supply source river data.".to_owned());
            }
            return match input.preparation {
                // The plan keeps the requested rivers choice so a retry or a
                // recovered job repeats it once a worker is configured.
                PreparationPreference::Automatic => Ok(Self {
                    hydrology_mode: input.hydrology_mode,
                    ..Self::fallback()
                }),
                _ => Err("Source preparation requires a configured geographic worker.".to_owned()),
            };
        }
        if input.preparation == PreparationPreference::Detailed
            && input.hydrology_mode == HydrologyMode::None
        {
            return Err(
                "Detailed preparation includes rivers; choose overview for a map without rivers."
                    .to_owned(),
            );
        }
        // A conservative creator window stays within the regional worker's
        // unwrapped, nonpolar footprint. The worker still verifies exact
        // projection bounds, tile counts and source/staging quotas.
        let regional = estimate.effective_side_meters <= MAX_CREATOR_DETAILED_SIDE_METERS
            && request.center_latitude_e7.unsigned_abs() <= 750_000_000
            && request.center_longitude_e7.unsigned_abs() <= 1_700_000_000;
        let detailed = match input.preparation {
            PreparationPreference::Detailed if !regional => {
                return Err("Creator detailed mode supports squares up to 120 km, centers within 75° latitude and 170° longitude; use overview for this selection.".to_owned());
            }
            PreparationPreference::Detailed => true,
            PreparationPreference::Overview => false,
            PreparationPreference::Automatic => {
                regional && input.hydrology_mode == HydrologyMode::Vectors
            }
        };
        // Overview packages always sample the independent landscape axes.
        let field_axes = (!detailed).then_some(OverviewFieldAxes::LANDSCAPE);
        let samples = if detailed {
            estimate
                .effective_side_meters
                .div_ceil(30)
                .next_power_of_two()
                .clamp(128, 4096) as u16
        } else {
            OverviewFieldAxes::LANDSCAPE.elevation
        };
        Ok(Self {
            mode: if detailed {
                PreparationMode::Detailed
            } else {
                PreparationMode::Overview
            },
            samples_per_axis: samples,
            field_axes,
            hydrology_mode: if detailed {
                HydrologyMode::Vectors
            } else {
                input.hydrology_mode
            },
            geographic_millimeters_per_sample: Some(sample_spacing(estimate, samples)),
            explanation: if detailed {
                "Regional elevation: modern Copernicus GLO30, with GLO90 only where GLO30 is absent. Modern WorldCover and European/Middle Eastern hydrography are stored as observations; mapped river corridors and lake extents only refine existing inland overview water. Vegetation and year-600 land use retain overview grids. Reservoirs and uncertain water remain evidence, not historical water. Source errors fail the job."
            } else if input.hydrology_mode == HydrologyMode::Automatic {
                "Automatic landscape overview: include rivers where the whole footprint is covered. Else use global overview water and record that the area is outside river coverage. Coverage is decided before acquiring inputs; source errors fail the job. Elevation is Copernicus GLO-90 averaged into each sample where the footprint needs at most 96 one-degree tiles, and the 1.85 km ETOPO overview elsewhere."
            } else if input.hydrology_mode == HydrologyMode::Vectors {
                "Landscape overview plus pinned HydroLAKES/HydroRIVERS evidence and modeled water on a 1024-sample axis. Elevation is Copernicus GLO-90 averaged into each sample where the footprint needs at most 96 one-degree tiles, and the 1.85 km ETOPO overview elsewhere. Ocean and vegetation context use the same 1024 axis; modern land cover is unobserved class-0 nodata, not WorldCover acquisition or year-600 observations. The footprint is verified against the pinned source coverage before acquisition; source errors fail the job."
            } else {
                "Landscape overview: elevation, modeled year-600 land use, potential vegetation and water each on independent 1024-sample grids. Elevation is Copernicus GLO-90 averaged into each sample where the footprint needs at most 96 one-degree tiles, and the 1.85 km ETOPO overview elsewhere. Fine vectors and real-source qualification are separate; source errors fail the job."
            },
        })
    }
}

fn sample_spacing(estimate: MapEstimate, samples: u16) -> u64 {
    estimate
        .effective_side_meters
        .saturating_mul(1_000)
        .div_ceil(u64::from(samples))
}

#[cfg(test)]
#[path = "preparation/tests/hydrology.rs"]
mod hydrology_tests;

#[cfg(test)]
#[path = "preparation/tests/options.rs"]
mod tests;
