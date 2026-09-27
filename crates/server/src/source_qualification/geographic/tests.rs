use super::*;
use aoe_map::{
    ENVIRONMENT_PAGE_SAMPLES, ElevationPage, EnvironmentPage, EnvironmentPageError,
    EnvironmentPageKey, EnvironmentPageProvider, EnvironmentalProvenance, FieldPyramid,
    LayerProvenance, MapRequest, PreparedEnvironment, ProjectionMetadata, PyramidLevel, Ratio,
    WaterPage, ordered_page_root, ordered_water_page_root,
};
use aoe_simulation::Terrain;
use std::{fs, sync::Arc};

#[test]
fn geographic_route_contract_has_five_20km_orders_and_100km_total() {
    let start = TileCoord::new(24_999, 24_999);
    let waypoints = planned_waypoints(start).expect("in-bounds route");
    assert_eq!(waypoints.len(), 5);
    assert_eq!(waypoints[0], TileCoord::new(34_999, 24_999));
    assert_eq!(waypoints[1], start);
    assert_eq!(waypoints[2], waypoints[0]);
    assert_eq!(waypoints[3], start);
    assert_eq!(waypoints[4], waypoints[0]);
    let distance = std::iter::once(start)
        .chain(waypoints.iter().copied())
        .zip(waypoints.iter().copied())
        .map(|(from, to)| {
            f64::from(from.x.abs_diff(to.x) + from.y.abs_diff(to.y)) * METERS_PER_TILE
        })
        .sum::<f64>();
    assert_eq!(distance, REQUIRED_DISTANCE_METERS);
}

#[test]
fn geographic_route_contract_rejects_waypoints_outside_map() {
    assert!(matches!(
        planned_waypoints(TileCoord::new(49_999, 49_999)),
        Err(SourceQualificationError::GeographicWaypointOutsideMap { .. })
    ));
}

fn prepared_route_fixture(
    wall_x: Option<usize>,
) -> (MapPackage, Vec<ElevationPage>, Vec<WaterPage>) {
    let request = MapRequest {
        requested_side_meters: 256,
        compression: Ratio::new(2, 1).expect("fixture compression"),
        ..MapRequest::default()
    };
    let mut axis = 64_u16;
    let mut level = 0_u8;
    let mut elevations = Vec::new();
    let mut waters = Vec::new();
    let mut elevation_levels = Vec::new();
    let mut water_levels = Vec::new();
    loop {
        let side = u8::try_from(axis).expect("fixture page side");
        let count = usize::from(axis).pow(2);
        let elevation = ElevationPage {
            level,
            x: 0,
            y: 0,
            width: side,
            height: side,
            geographic_height_centimeters: vec![0; count],
        };
        let mut ocean = vec![0; count];
        if level == 0
            && let Some(wall_x) = wall_x
        {
            for y in 0..usize::from(axis) {
                ocean[y * usize::from(axis) + wall_x] = 100;
            }
        }
        let water = WaterPage {
            level,
            x: 0,
            y: 0,
            width: side,
            height: side,
            ocean_coverage_percent: ocean,
            inland_coverage_percent: vec![0; count],
        };
        elevation_levels.push(PyramidLevel {
            samples_per_axis: axis,
            ordered_page_root: ordered_page_root(std::slice::from_ref(&elevation))
                .expect("elevation page root"),
        });
        water_levels.push(PyramidLevel {
            samples_per_axis: axis,
            ordered_page_root: ordered_water_page_root(std::slice::from_ref(&water))
                .expect("water page root"),
        });
        elevations.push(elevation);
        waters.push(water);
        if axis == 1 {
            break;
        }
        axis = axis.div_ceil(2);
        level += 1;
    }
    let environment = PreparedEnvironment {
        samples_per_axis: 64,
        geographic_millimeters_per_sample: 1_000,
        page_samples: ENVIRONMENT_PAGE_SAMPLES,
        elevation: FieldPyramid {
            levels: elevation_levels,
        },
        water: Some(FieldPyramid {
            levels: water_levels,
        }),
        ..PreparedEnvironment::default()
    };
    let package = MapPackage::with_prepared_environment(
        aoe_map::MAP_SCHEMA_VERSION,
        request,
        Vec::new(),
        ProjectionMetadata::default(),
        EnvironmentalProvenance::default(),
        environment,
    )
    .expect("prepared route fixture package");
    (package, elevations, waters)
}

#[derive(Debug)]
struct FailingPageProvider(EnvironmentPageError);

impl EnvironmentPageProvider for FailingPageProvider {
    fn page(
        &self,
        _key: EnvironmentPageKey,
        _cancelled: &dyn Fn() -> bool,
    ) -> Result<Arc<EnvironmentPage>, EnvironmentPageError> {
        Err(self.0)
    }
}

#[test]
fn route_planner_diagnostics_report_completed_and_proven_unreachable_paths() {
    let (open_package, elevations, waters) = prepared_route_fixture(None);
    let open = Terrain::from_prepared_package(
        &open_package,
        elevations.clone(),
        waters.clone(),
        vec![],
        vec![],
    )
    .expect("open route fixture terrain");
    let completed = plan_order(
        &open,
        TileCoord::new(1, 1),
        TileCoord::new(8, 1),
        "synthetic-open-route",
        Some(1),
    )
    .expect("completed route diagnostic");
    assert_eq!(completed.outcome, RoutePlanningOutcome::Complete);
    assert_eq!(completed.origin, [1, 1]);
    assert_eq!(completed.destination, [8, 1]);
    assert!(completed.path_tile_count >= 8);
    assert!(completed.work > 0);

    let (wall_package, elevations, waters) = prepared_route_fixture(Some(32));
    let wall = Terrain::from_prepared_package(&wall_package, elevations, waters, vec![], vec![])
        .expect("water-wall route fixture terrain");
    let unreachable = plan_order(
        &wall,
        TileCoord::new(1, 32),
        TileCoord::new(62, 32),
        "synthetic-water-wall",
        None,
    )
    .expect("unreachable route diagnostic");
    assert_eq!(unreachable.outcome, RoutePlanningOutcome::Unreachable);
    assert!(unreachable.work > 0);
    assert!(unreachable.peak_retained_entries > 0);
}

#[test]
fn route_planner_diagnostics_keep_cancelled_and_provider_errors_distinct() {
    let (package, _, _) = prepared_route_fixture(None);
    for (provider_error, expected) in [
        (
            EnvironmentPageError::Cancelled,
            RoutePlanningOutcome::Cancelled,
        ),
        (
            EnvironmentPageError::Unavailable,
            RoutePlanningOutcome::ProviderError,
        ),
    ] {
        let terrain =
            Terrain::from_page_provider(&package, Arc::new(FailingPageProvider(provider_error)))
                .expect("provider-backed route fixture");
        let diagnostic = plan_order(
            &terrain,
            TileCoord::new(1, 1),
            TileCoord::new(8, 1),
            "synthetic-provider-failure",
            None,
        )
        .expect("provider failure diagnostic");
        assert_eq!(diagnostic.outcome, expected);
    }
}

fn source_lock() -> aoe_map::SourceLock {
    aoe_map::SourceLock {
        id: "synthetic-reference".to_owned(),
        provider: "test-only".to_owned(),
        release: "fixture".to_owned(),
        url: "https://example.invalid/synthetic-reference".to_owned(),
        sha256: [5; 32],
        acquired_at: "2026-09-26T00:00:00Z".to_owned(),
        native_resolution: "fixture".to_owned(),
        crs: "EPSG:4326".to_owned(),
        vertical_datum: "fixture".to_owned(),
        license: "test-only".to_owned(),
        preprocessing_version: "fixture-v1".to_owned(),
    }
}

fn synthetic_reference_package(center: (i32, i32)) -> (MapPackage, ElevationPage) {
    let request = MapRequest {
        center_latitude_e7: center.0,
        center_longitude_e7: center.1,
        requested_side_meters: 100_000,
        compression: Ratio::new(1, 1).expect("1:1 fixture compression"),
        ..MapRequest::default()
    };
    let elevation = ElevationPage {
        level: 0,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        geographic_height_centimeters: vec![0],
    };
    let environment = PreparedEnvironment {
        samples_per_axis: 1,
        geographic_millimeters_per_sample: 1_000,
        page_samples: ENVIRONMENT_PAGE_SAMPLES,
        elevation: FieldPyramid {
            levels: vec![PyramidLevel {
                samples_per_axis: 1,
                ordered_page_root: ordered_page_root(std::slice::from_ref(&elevation))
                    .expect("synthetic elevation root"),
            }],
        },
        ..PreparedEnvironment::default()
    };
    let package = MapPackage::with_prepared_environment(
        aoe_map::MAP_SCHEMA_VERSION,
        request,
        vec![source_lock()],
        ProjectionMetadata::default(),
        EnvironmentalProvenance {
            elevation: LayerProvenance::SourceDerived,
            ..EnvironmentalProvenance::default()
        },
        environment,
    )
    .expect("small synthetic reference package");
    (package, elevation)
}

fn persist_synthetic_reference(directory: &Path, package: &MapPackage, page: &ElevationPage) {
    fs::create_dir_all(directory).expect("synthetic package directory");
    let hash = package.content_hash_hex();
    let pages = directory.join("pages").join(&hash).join("elevation");
    fs::create_dir_all(&pages).expect("synthetic source page directory");
    fs::write(
        directory.join(format!("{hash}.json")),
        serde_json::to_vec(package).expect("synthetic manifest"),
    )
    .expect("write synthetic manifest");
    fs::write(
        pages.join("0-0-0.json"),
        serde_json::to_vec(page).expect("synthetic source page"),
    )
    .expect("write synthetic source page");
}

#[test]
fn reference_loader_checks_the_persisted_package_and_fixed_location_without_claiming_source_data() {
    let directory = tempfile::tempdir().expect("synthetic reference directory");
    let center = (250_000_000, -50_000_000);
    let (package, page) = synthetic_reference_package(center);
    let hash = package.content_hash_hex();
    persist_synthetic_reference(directory.path(), &package, &page);
    assert_eq!(
        load_reference_package(directory.path(), &hash)
            .expect("synthetic fixture meets the manifest contract")
            .content_hash_hex(),
        hash
    );
    assert!(matches!(
        load_reference_package(directory.path(), &"0".repeat(64)),
        Err(SourceQualificationError::UnknownPackage { .. })
    ));

    let wrong_directory = tempfile::tempdir().expect("wrong-center fixture directory");
    let (wrong_center, page) = synthetic_reference_package((250_000_000, -49_999_999));
    let wrong_hash = wrong_center.content_hash_hex();
    persist_synthetic_reference(wrong_directory.path(), &wrong_center, &page);
    assert!(matches!(
        load_reference_package(wrong_directory.path(), &wrong_hash),
        Err(SourceQualificationError::GeographicReferenceLocationMismatch { .. })
    ));
}

#[test]
fn replay_comparison_accepts_identical_hashes_and_reports_the_failed_tick() {
    assert!(ensure_replay([7; 32], [7; 32], 10).is_ok());
    assert!(matches!(
        ensure_replay([7; 32], [8; 32], 4_096),
        Err(SourceQualificationError::ReplayDiverged(4_096))
    ));
}
