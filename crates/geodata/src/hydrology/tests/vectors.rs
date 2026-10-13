use super::*;

#[test]
fn coverage_comes_from_the_dataset_extent_and_includes_southern_finland() {
    let cancelled = AtomicBool::new(false);
    // Retain every request and valid axis in the former pilot-window test.
    // The expanded pinned extent deliberately covers its former rejections.
    let france = MapRequest {
        requested_side_meters: 1_200_000,
        ..MapRequest::default()
    };
    for request in [
        france,
        MapRequest {
            center_latitude_e7: 360_000_000,
            ..france
        },
        MapRequest {
            center_latitude_e7: 600_000_000,
            ..france
        },
        MapRequest {
            center_longitude_e7: -120_000_000,
            ..france
        },
        MapRequest {
            center_longitude_e7: 250_000_000,
            ..france
        },
        MapRequest {
            center_latitude_e7: 615_000_000,
            center_longitude_e7: 255_000_000,
            ..MapRequest::default()
        },
        // A former coverage rejection of the worker-option test: the
        // expanded extent covers the default footprint at latitude 60.
        MapRequest {
            center_latitude_e7: 600_000_000,
            ..MapRequest::default()
        },
    ] {
        for axis in [2, 15, 16, 65] {
            assert!(
                validate_vector_footprint(request, axis, &cancelled).is_ok(),
                "{request:?}, axis {axis}"
            );
        }
    }
    // Still rejected: a footprint west of the extent, and the default-centred
    // 3,000 km square whose corners leave it.
    for request in [
        MapRequest {
            center_longitude_e7: -750_000_000,
            ..MapRequest::default()
        },
        MapRequest {
            requested_side_meters: 3_000_000,
            ..MapRequest::default()
        },
    ] {
        assert!(
            validate_vector_footprint(request, 16, &cancelled).is_err(),
            "{request:?}"
        );
    }
}

#[test]
fn pinned_vector_selection_has_no_worldcover_or_catalog_inputs() {
    let sources = hydrology_vector_sources();
    assert_eq!(sources.len(), 2);
    assert_eq!(sources[0].id, "hydrolakes-v1.0-global-gdb");
    assert_eq!(sources[1].id, "hydrorivers-v1.0-eu-shp");
    assert!(
        sources
            .iter()
            .all(|source| !source.id.contains("worldcover"))
    );
    assert!(
        sources
            .iter()
            .all(|source| matches!(source.expected_checksum, crate::ExpectedChecksum::Sha256(_)))
    );
    assert!(
        sources.iter().map(|source| source.bytes).sum::<u64>()
            <= crate::MAX_HYDROLOGY_DOWNLOAD_BYTES
    );
}

#[test]
fn overview_dem_context_accepts_full_128_and_1024_pyramids_without_relaxing_grid_validation() {
    for axis in [128_u16, 1024] {
        let mut pages = Vec::new();
        for y in 0..axis / PAGE {
            for x in 0..axis / PAGE {
                pages.push(aoe_map::ElevationPage {
                    level: 0,
                    x,
                    y,
                    width: PAGE as u8,
                    height: PAGE as u8,
                    geographic_height_centimeters: vec![4200; usize::from(PAGE).pow(2)],
                });
            }
        }
        let level_zero = pages.clone();
        pages.push(aoe_map::ElevationPage {
            level: 1,
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            geographic_height_centimeters: vec![4200],
        });
        assert!(water_model::ElevationGrid::new(&pages).is_err());
        assert_eq!(
            super::super::overview_elevation_context(&pages).unwrap(),
            level_zero
        );
        pages.push(pages[0].clone());
        assert!(super::super::overview_elevation_context(&pages).is_err());
    }
    assert!(super::super::overview_elevation_context(&[]).is_err());
}

#[test]
fn vector_api_cancel_and_invalid_axis_fail_before_context_cache_or_network() {
    let root = std::env::temp_dir().join(format!("aoe-vector-no-cache-{}", std::process::id()));
    assert!(!root.exists(), "test root must not already exist");
    let request = MapRequest::default();
    let corrections = WaterCorrectionDocument::empty(request, 2).expect("corrections");
    assert!(matches!(
        PreparedHydrology::prepare_vectors(
            root.clone(),
            request,
            2,
            &[],
            &[],
            corrections.clone(),
            &AtomicBool::new(true),
        ),
        Err(GeodataError::Cache(crate::CacheError::Cancelled))
    ));
    assert!(!root.exists());
    assert!(matches!(
        prepare_hydrology_vectors(
            root.clone(),
            request,
            1,
            &[],
            &[],
            corrections.clone(),
            &AtomicBool::new(false),
        ),
        Err(GeodataError::Preparation(
            "hydrology preparation supports 2 through 1024 samples per axis"
        ))
    ));
    assert!(
        prepare_hydrology_vectors(
            root.clone(),
            MapRequest {
                schema_version: 2,
                ..request
            },
            2,
            &[],
            &[],
            corrections,
            &AtomicBool::new(false),
        )
        .is_err()
    );
    assert!(!root.exists());
}

#[test]
fn uncovered_vectors_fail_preflight_without_acquiring_inputs() {
    let request = MapRequest {
        center_latitude_e7: 0,
        center_longitude_e7: 0,
        requested_side_meters: 30_000,
        ..MapRequest::default()
    };
    let cancelled = AtomicBool::new(false);
    let corrections = WaterCorrectionDocument::empty(request, 2).expect("corrections");
    let error = PreparedHydrology::validate_vectors_request(request, 2, &corrections, &cancelled)
        .expect_err("uncovered preflight");
    assert!(
        error
            .to_string()
            .contains("outside the pinned river dataset extent")
    );
}

#[test]
fn uncovered_vector_preparation_does_not_create_a_cache() {
    let root = std::env::temp_dir().join(format!("aoe-uncovered-vectors-{}", std::process::id()));
    assert!(!root.exists());
    // A sentinel denies cache creation and network access even before the fix.
    std::fs::write(&root, b"untouched").unwrap();
    let request = MapRequest {
        center_latitude_e7: 0,
        center_longitude_e7: 0,
        requested_side_meters: 30_000,
        ..MapRequest::default()
    };
    let elevation = [aoe_map::ElevationPage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        geographic_height_centimeters: vec![0; 4],
    }];
    let water = (0..2)
        .flat_map(|y| {
            (0..2).map(move |x| aoe_map::WaterPage {
                level: 0,
                x,
                y,
                width: 64,
                height: 64,
                ocean_coverage_percent: vec![0; 4096],
                inland_coverage_percent: vec![0; 4096],
            })
        })
        .collect::<Vec<_>>();
    let error = PreparedHydrology::prepare_vectors(
        root.clone(),
        request,
        2,
        &elevation,
        &water,
        WaterCorrectionDocument::empty(request, 2).expect("corrections"),
        &AtomicBool::new(false),
    )
    .expect_err("uncovered preparation");
    assert_eq!(std::fs::read(&root).unwrap(), b"untouched");
    std::fs::remove_file(&root).unwrap();
    assert!(
        error
            .to_string()
            .contains("outside the pinned river dataset extent")
    );
}

#[test]
fn every_hydrology_preparation_path_rejects_uncovered_before_cache_or_inputs() {
    let request = MapRequest {
        center_latitude_e7: 0,
        center_longitude_e7: 0,
        requested_side_meters: 30_000,
        ..MapRequest::default()
    };
    let cancelled = AtomicBool::new(false);
    let corrections = WaterCorrectionDocument::empty(request, 2).unwrap();
    // A file where a cache directory would be makes any cache/input access
    // fail distinctly, without allowing acquisition even on unfixed code.
    let mut failures = Vec::new();
    for route in [
        "vector",
        "vector-method",
        "full",
        "detailed",
        "detailed-corrections",
        "overview-worker",
        "detailed-worker",
    ] {
        let root =
            std::env::temp_dir().join(format!("aoe-uncovered-{route}-{}", std::process::id()));
        let output = root.with_extension("output");
        assert!(!root.exists() && !output.exists());
        std::fs::write(&root, b"untouched").unwrap();
        let result = match route {
            "vector" => prepare_hydrology_vectors(
                root.clone(),
                request,
                2,
                &[],
                &[],
                corrections.clone(),
                &cancelled,
            )
            .map(|_| ()),
            "vector-method" => PreparedHydrology::prepare_vectors(
                root.clone(),
                request,
                2,
                &[],
                &[],
                corrections.clone(),
                &cancelled,
            )
            .map(|_| ()),
            "full" => crate::prepare_hydrology(root.clone(), request, 2).map(|_| ()),
            "detailed" => crate::prepare_detailed_directory(
                root.clone(),
                output.clone(),
                request,
                2,
                crate::DemResolution::Glo90,
            )
            .map(|_| ()),
            "detailed-corrections" => crate::prepare_detailed_directory_with_water_corrections(
                root.clone(),
                output.clone(),
                request,
                2,
                crate::DemResolution::Glo90,
                Some(corrections.clone()),
            )
            .map(|_| ()),
            "overview-worker" => crate::execute_with_cancellation(
                crate::WorkerRequest::PrepareOverviewDirectory {
                    cache_root: root.clone(),
                    output_directory: output.clone(),
                    request,
                    samples_per_axis: aoe_map::LANDSCAPE_OVERVIEW_SAMPLES_PER_AXIS,
                    field_axes: aoe_map::OverviewFieldAxes::LANDSCAPE,
                    hydrology_mode: aoe_map::OverviewHydrologyMode::Vectors,
                    water_corrections: None,
                    historical_corrections: None,
                    vegetation_corrections: None,
                },
                &cancelled,
            )
            .map(|_| ()),
            "detailed-worker" => crate::execute_with_cancellation(
                crate::WorkerRequest::PrepareDetailedDirectory {
                    cache_root: root.clone(),
                    output_directory: output.clone(),
                    request,
                    samples_per_axis: 2,
                    resolution: crate::DemResolution::Glo90,
                    staging_root: None,
                    historical_corrections: None,
                    water_corrections: None,
                    vegetation_corrections: None,
                },
                &cancelled,
            )
            .map(|_| ()),
            _ => unreachable!(),
        };
        let error = result.expect_err(route);
        assert_eq!(std::fs::read(&root).unwrap(), b"untouched", "{route}");
        assert!(!output.exists(), "{route}: no publication");
        std::fs::remove_file(root).unwrap();
        if !error
            .to_string()
            .contains("outside the pinned river dataset extent")
        {
            failures.push(format!("{route}: {error}"));
        }
    }
    assert!(
        failures.is_empty(),
        "coverage preflight failures: {failures:#?}"
    );
}

#[test]
fn validated_footprint_is_reused_and_cannot_be_rebound_to_another_grid() {
    let request = MapRequest::default();
    let footprint = project_river_footprint(request, 2, &AtomicBool::new(false)).unwrap();
    assert!(footprint.covered());
    assert!(footprint.check_binding(request, 2).is_ok());
    for (request, axis) in [(request, 15), (MapRequest { seed: 2, ..request }, 2)] {
        assert!(footprint.check_binding(request, axis).is_err());
    }
    let root = std::env::temp_dir().join(format!("aoe-plan-grid-{}", std::process::id()));
    std::fs::write(&root, b"untouched").unwrap();
    let plan = HydrologySourcePlan {
        worldcover: Vec::new(),
        sources: Vec::new(),
        footprint,
    };
    let result = prepare_with_plan(
        root.clone(),
        request,
        15,
        plan,
        &[],
        HydrologySelection::WorldCover,
    );
    assert!(
        matches!(result,Err(GeodataError::Preparation(message)) if message.contains("preflight grid"))
    );
    assert_eq!(std::fs::read(&root).unwrap(), b"untouched");
    std::fs::remove_file(root).unwrap();
}
