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
    #[serde(default)]
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
        if input.hydrology_mode == HydrologyMode::Vectors
            && (!worker_available
                || input.preparation != PreparationPreference::Overview
                || !aoe_map::VECTOR_HYDROLOGY_FOOTPRINT_WINDOW
                    .contains_e7(request.center_longitude_e7, request.center_latitude_e7))
        {
            return Err("Vector hydrology requires a configured worker, explicit overview, and a center inside the pinned vector source coverage; the worker verifies the entire projected footprint before acquisition.".to_owned());
        }
        if !worker_available {
            return match input.preparation {
                PreparationPreference::Automatic => Ok(Self::fallback()),
                _ => Err("Source preparation requires a configured geographic worker.".to_owned()),
            };
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
            PreparationPreference::Automatic => regional,
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
            hydrology_mode: input.hydrology_mode,
            geographic_millimeters_per_sample: Some(sample_spacing(estimate, samples)),
            explanation: if detailed {
                "Regional elevation: modern Copernicus GLO30, with GLO90 only where GLO30 is absent. Modern WorldCover and European/Middle Eastern hydrography are stored as observations; mapped river corridors and lake extents only refine existing inland overview water. Vegetation and year-600 land use retain overview grids. Reservoirs and uncertain water remain evidence, not historical water. Source errors fail the job."
            } else if input.hydrology_mode == HydrologyMode::Vectors {
                "Landscape overview plus pinned HydroLAKES/HydroRIVERS evidence and modeled water on a 1024-sample axis. Categorical ocean/water context remains 128; modern land cover is unobserved class-0 nodata, not WorldCover acquisition or year-600 observations. The footprint is verified against the pinned source coverage before acquisition; source errors fail the job."
            } else {
                "Landscape overview: elevation and modeled year-600 land use on 1024-sample grids, potential vegetation and water on independent 128-sample grids. Fine vectors and real-source qualification are separate; source errors fail the job."
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
mod tests {
    use super::*;

    fn input() -> CreationRequest {
        CreationRequest {
            request: MapRequest::default(),
            preparation: PreparationPreference::Automatic,
            hydrology_mode: HydrologyMode::None,
        }
    }

    #[test]
    fn requests_without_a_mode_are_automatic_and_unknown_modes_fail() {
        let mut value = serde_json::to_value(MapRequest::default()).expect("request");
        let automatic: CreationRequest = serde_json::from_value(value.clone()).expect("automatic");
        assert_eq!(
            PreparationPlan::resolve(automatic, true)
                .expect("plan")
                .mode,
            PreparationMode::Detailed
        );
        value["preparation"] = serde_json::json!("invented");
        assert!(serde_json::from_value::<CreationRequest>(value).is_err());
    }

    #[test]
    fn regional_grid_bounds_use_effective_square_and_normalized_longitude() {
        let mut input = input();
        input.request.requested_side_meters = 120_000;
        assert_eq!(
            PreparationPlan::resolve(input, true)
                .expect("largest")
                .samples_per_axis,
            4096
        );
        input.request.requested_side_meters = 250;
        input.request.compression = aoe_map::Ratio::new(1, 1).expect("ratio");
        assert_eq!(
            PreparationPlan::resolve(input, true)
                .expect("small")
                .samples_per_axis,
            128
        );
        input.request.center_longitude_e7 = 1_900_000_000;
        assert_eq!(
            PreparationPlan::resolve(input, true)
                .expect("wrapped coordinate")
                .mode,
            PreparationMode::Detailed
        );
        input.request.compression = aoe_map::Ratio::new(10_000, 1).expect("ratio");
        assert!(PreparationPlan::resolve(input, true).is_err());
        input = super::tests::input();
        input.request.requested_side_meters = 30_001;
        assert_eq!(
            PreparationPlan::resolve(input, true)
                .expect("rounded extent")
                .geographic_millimeters_per_sample,
            Some(29_356)
        );
    }

    #[test]
    fn automatic_regional_detail_and_explicit_overview_are_distinct() {
        let mut input = input();
        let plan = PreparationPlan::resolve(input, true).expect("regional");
        assert_eq!(plan.mode, PreparationMode::Detailed);
        assert_eq!(plan.samples_per_axis, 1024);
        assert_eq!(plan.geographic_millimeters_per_sample, Some(29_297));
        input.preparation = PreparationPreference::Overview;
        assert_eq!(
            PreparationPlan::resolve(input, true)
                .expect("overview")
                .samples_per_axis,
            1024
        );
    }

    #[test]
    fn overview_always_uses_the_independent_landscape_axes() {
        let mut input = input();
        input.request.requested_side_meters = 1_200_000;
        input.request.compression = aoe_map::Ratio::new(30, 1).expect("ratio");
        let landscape = PreparationPlan::resolve(input, true).expect("landscape overview");
        assert_eq!(landscape.mode, PreparationMode::Overview);
        assert_eq!(landscape.samples_per_axis, 1024);
        assert_eq!(landscape.geographic_millimeters_per_sample, Some(1_171_875));
        assert_eq!(
            landscape.field_axes,
            Some(OverviewFieldAxes {
                elevation: 1024,
                vegetation: 128,
                water: 128,
                historical: 1024,
            })
        );
        assert!(landscape.explanation.contains("independent"));
        let json = serde_json::to_value(landscape).expect("landscape plan");
        assert_eq!(
            json["field_axes"],
            serde_json::json!({
                "elevation": 1024, "vegetation": 128, "water": 128, "historical": 1024,
            })
        );
        assert!(
            PreparationPlan::resolve(input, false)
                .expect("fallback")
                .field_axes
                .is_none()
        );
        input.request.requested_side_meters = 30_000;
        assert!(
            PreparationPlan::resolve(input, true)
                .expect("detailed")
                .field_axes
                .is_none()
        );
    }

    #[test]
    fn unavailable_sources_are_explicit_and_large_or_wrapped_selections_use_overview() {
        let mut input = input();
        assert_eq!(
            PreparationPlan::resolve(input, false)
                .expect("fallback")
                .mode,
            PreparationMode::ProceduralFallback
        );
        input.preparation = PreparationPreference::Detailed;
        assert!(PreparationPlan::resolve(input, false).is_err());
        for (side, latitude, longitude) in [
            (121_000, 0, 0),
            (30_000, 760_000_000, 0),
            (30_000, 0, 1_790_000_000),
        ] {
            input.request.requested_side_meters = side;
            input.request.center_latitude_e7 = latitude;
            input.request.center_longitude_e7 = longitude;
            input.preparation = PreparationPreference::Automatic;
            assert_eq!(
                PreparationPlan::resolve(input, true)
                    .expect("overview")
                    .mode,
                PreparationMode::Overview
            );
            input.preparation = PreparationPreference::Detailed;
            assert!(PreparationPlan::resolve(input, true).is_err());
        }
    }
}
