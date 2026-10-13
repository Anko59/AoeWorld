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
        PreparationMode::Overview
    );
    value["preparation"] = serde_json::json!("invented");
    assert!(serde_json::from_value::<CreationRequest>(value).is_err());
}

#[test]
fn regional_grid_bounds_use_effective_square_and_normalized_longitude() {
    let mut input = input();
    input.preparation = PreparationPreference::Detailed;
    input.hydrology_mode = HydrologyMode::Vectors;
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
    input.preparation = PreparationPreference::Detailed;
    input.hydrology_mode = HydrologyMode::Vectors;
    input.request.requested_side_meters = 30_001;
    assert_eq!(
        PreparationPlan::resolve(input, true)
            .expect("rounded extent")
            .geographic_millimeters_per_sample,
        Some(29_356)
    );
}

#[test]
fn automatic_overview_and_explicit_regional_detail_are_distinct() {
    let mut input = input();
    assert_eq!(
        PreparationPlan::resolve(input, true).unwrap().mode,
        PreparationMode::Overview
    );
    input.preparation = PreparationPreference::Detailed;
    input.hydrology_mode = HydrologyMode::Vectors;
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
            .unwrap()
            .field_axes
            .is_some()
    );
    input.preparation = PreparationPreference::Detailed;
    input.hydrology_mode = HydrologyMode::Vectors;
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

#[tokio::test]
async fn explicit_source_choices_never_become_procedural_fallback() {
    use crate::map_jobs::{JobState, Manager, StartError};

    let (state, _, _) = crate::map_jobs::tests::state_with_job();
    for (latitude, longitude, regional) in [
        (
            MapRequest::default().center_latitude_e7,
            MapRequest::default().center_longitude_e7,
            true,
        ),
        (-178_000_000, 1_798_000_000, false),
    ] {
        for worker in [false, true] {
            for preparation in [
                PreparationPreference::Automatic,
                PreparationPreference::Overview,
                PreparationPreference::Detailed,
            ] {
                for hydrology_mode in [
                    HydrologyMode::Vectors,
                    HydrologyMode::Automatic,
                    HydrologyMode::None,
                ] {
                    let input = CreationRequest {
                        request: MapRequest {
                            center_latitude_e7: latitude,
                            center_longitude_e7: longitude,
                            ..MapRequest::default()
                        },
                        preparation,
                        hydrology_mode,
                    };
                    let case = format!("{input:?}, worker={worker}");
                    let expected_error = if !worker && hydrology_mode == HydrologyMode::Vectors {
                        Some(
                            "With rivers requires a configured geographic worker; procedural fallback cannot supply source river data.",
                        )
                    } else if !worker && preparation != PreparationPreference::Automatic {
                        Some("Source preparation requires a configured geographic worker.")
                    } else if worker
                        && preparation == PreparationPreference::Detailed
                        && hydrology_mode == HydrologyMode::None
                    {
                        Some(
                            "Detailed preparation includes rivers; choose overview for a map without rivers.",
                        )
                    } else if worker && preparation == PreparationPreference::Detailed && !regional
                    {
                        Some(
                            "Creator detailed mode supports squares up to 120 km, centers within 75° latitude and 170° longitude; use overview for this selection.",
                        )
                    } else {
                        None
                    };
                    let result = PreparationPlan::resolve(input, worker);
                    if let Some(error) = expected_error {
                        assert_eq!(result.unwrap_err(), error, "{case}");
                    } else {
                        let plan = result.unwrap_or_else(|error| panic!("{case}: {error}"));
                        if worker {
                            let detailed = preparation == PreparationPreference::Detailed
                                || (preparation == PreparationPreference::Automatic
                                    && regional
                                    && hydrology_mode == HydrologyMode::Vectors);
                            assert_eq!(
                                plan.mode,
                                if detailed {
                                    PreparationMode::Detailed
                                } else {
                                    PreparationMode::Overview
                                },
                                "{case}"
                            );
                            assert_eq!(
                                plan.hydrology_mode,
                                if detailed {
                                    HydrologyMode::Vectors
                                } else {
                                    hydrology_mode
                                },
                                "{case}"
                            );
                        } else {
                            assert_eq!(plan.mode, PreparationMode::ProceduralFallback, "{case}");
                            assert!(
                                plan.explanation.contains("No geographic worker configured"),
                                "{case}"
                            );
                        }
                    }
                    if worker {
                        continue;
                    }
                    *state.map_jobs.lock().await = Manager::default();
                    let submitted = crate::map_jobs::start(&state, input, None).await;
                    if let Some(error) = expected_error {
                        assert!(
                            matches!(submitted, Err(StartError::Invalid(ref actual)) if actual == error),
                            "{case}: {submitted:?}"
                        );
                        assert!(state.map_jobs.lock().await.jobs.is_empty(), "{case}");
                        continue;
                    }
                    let job = submitted.unwrap();
                    let completed =
                        tokio::time::timeout(std::time::Duration::from_secs(5), async {
                            loop {
                                let job = crate::map_jobs::status(&state, job.id).await.unwrap();
                                if job.state == JobState::Completed {
                                    break job;
                                }
                                assert_ne!(job.state, JobState::Failed, "{case}: {job:?}");
                                tokio::task::yield_now().await;
                            }
                        })
                        .await
                        .expect("procedural job completes");
                    assert!(
                        completed
                            .preparation
                            .explanation
                            .contains("No geographic worker configured")
                    );
                    assert_eq!(
                        serde_json::to_value(&completed.preparation_notes).unwrap(),
                        serde_json::json!(["geographic_worker_unavailable"]),
                        "{case}"
                    );
                    let packages = state.map_packages.read().await;
                    let package = &packages[completed.content_hash.as_ref().unwrap()];
                    assert_eq!(
                        package.preparation_notes, completed.preparation_notes,
                        "{case}"
                    );
                    assert!(package.source_locks.is_empty(), "{case}");
                    assert!(package.environment.hydrology_evidence.is_none(), "{case}");
                    package.validate().unwrap();
                }
            }
        }
    }
}
