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
