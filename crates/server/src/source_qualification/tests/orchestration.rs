//! End-to-end contract regression using persisted, explicitly synthetic pages.
//! This test writes no qualification report and does not claim real-source evidence.

use super::super::{
    MAX_RESIDENT_PAGES, MAX_ROUTE_TICKS, SourceScalePackageReference,
    run_source_qualification_with_geographic_reference,
};
use aoe_core::TileCoord;
use aoe_map::{
    ENVIRONMENT_PAGE_SAMPLES, ElevationPage, EnvironmentPageProvider, EnvironmentalProvenance,
    FieldPyramid, LayerProvenance, MapPackage, MapRequest, PotentialBiomePage, PreparedEnvironment,
    PyramidLevel, Ratio, ResourceNode, SourceLock, WaterPage, ordered_biome_page_root,
    ordered_page_root, ordered_water_page_root,
};
use aoe_simulation::{GameWorld, StartSearchResult};
use std::{path::Path, sync::Arc};

const SAHARA_CENTER_E7: (i32, i32) = (250_000_000, -50_000_000);
const SOURCE_LOCK_ID: &str = "synthetic-flat-qualification-fixture";
const PRIMARY_SAMPLE_AXIS: u16 = 1_024;
const FIXED_FIXTURE_SEED: u64 = 2;

struct PersistedFixture {
    package: MapPackage,
    elevation: Vec<ElevationPage>,
    water: Vec<WaterPage>,
    vegetation: Vec<PotentialBiomePage>,
    centered_resource: Option<ResourceNode>,
}

fn fixture(tiles_per_side: u64, samples_per_axis: u16) -> PersistedFixture {
    let axes = pyramid_axes(samples_per_axis);
    let (elevation, elevation_levels) = elevation_pyramid(&axes);
    let (water, water_levels) = water_pyramid(&axes);
    // Potential-biome class 27 is desert, a zero-tree-density biome.
    let (vegetation, vegetation_levels) = desert_pyramid(&axes);
    let physical_side_meters = tiles_per_side * u64::from(aoe_map::GAME_TILE_METERS);
    let environment = PreparedEnvironment {
        samples_per_axis,
        geographic_millimeters_per_sample: physical_side_meters
            .saturating_mul(1_000)
            .div_ceil(u64::from(samples_per_axis - 1)),
        page_samples: ENVIRONMENT_PAGE_SAMPLES,
        elevation: FieldPyramid {
            levels: elevation_levels,
        },
        water: Some(FieldPyramid {
            levels: water_levels,
        }),
        vegetation: Some(FieldPyramid {
            levels: vegetation_levels,
        }),
        ..PreparedEnvironment::default()
    };
    let request = MapRequest {
        center_latitude_e7: SAHARA_CENTER_E7.0,
        center_longitude_e7: SAHARA_CENTER_E7.1,
        requested_side_meters: physical_side_meters,
        compression: Ratio::new(1, 1).expect("identity compression"),
        seed: FIXED_FIXTURE_SEED,
        ..MapRequest::default()
    };
    let package = MapPackage::with_prepared_environment(
        aoe_map::MAP_SCHEMA_VERSION,
        request,
        vec![synthetic_source_lock()],
        Default::default(),
        EnvironmentalProvenance {
            elevation: LayerProvenance::SourceDerived,
            water: LayerProvenance::SourceDerived,
            vegetation: LayerProvenance::SourceDerived,
            ..EnvironmentalProvenance::default()
        },
        environment,
    )
    .expect("synthetic prepared source package");
    let centered_resource = if tiles_per_side == 50_000 {
        let generator = package
            .generator_with_environment(
                elevation.clone(),
                water.clone(),
                vegetation.clone(),
                vec![],
            )
            .expect("in-memory synthetic source pages");
        Some(
            super::super::find_center_resource(&generator, tiles_per_side as i32)
                .expect("fixed synthetic source resource"),
        )
    } else {
        None
    };
    PersistedFixture {
        package,
        elevation,
        water,
        vegetation,
        centered_resource,
    }
}

fn synthetic_source_lock() -> SourceLock {
    SourceLock {
        id: SOURCE_LOCK_ID.to_owned(),
        provider: "synthetic-test-only".to_owned(),
        release: "offline-fixture-v1".to_owned(),
        url: "https://example.invalid/synthetic-flat-fixture".to_owned(),
        sha256: [0x5a; 32],
        acquired_at: "2026-09-27T00:00:00Z".to_owned(),
        native_resolution: "synthetic constant fields".to_owned(),
        crs: "EPSG:4326".to_owned(),
        vertical_datum: "synthetic zero elevation".to_owned(),
        license: "test-only".to_owned(),
        preprocessing_version: "synthetic-v1".to_owned(),
    }
}

fn pyramid_axes(samples_per_axis: u16) -> Vec<(u8, u16)> {
    let mut axes = Vec::new();
    let mut samples = samples_per_axis;
    while samples > 0 {
        axes.push((axes.len() as u8, samples));
        if samples == 1 {
            break;
        }
        samples = samples.div_ceil(2);
    }
    axes
}

fn page_shape(samples: u16, x: u16, y: u16) -> (u8, u8) {
    let page_side = u16::from(ENVIRONMENT_PAGE_SAMPLES);
    (
        (samples - x * page_side).min(page_side) as u8,
        (samples - y * page_side).min(page_side) as u8,
    )
}

fn elevation_pyramid(axes: &[(u8, u16)]) -> (Vec<ElevationPage>, Vec<PyramidLevel>) {
    let mut pages = Vec::new();
    let mut levels = Vec::new();
    for &(level, samples) in axes {
        let page_count = samples.div_ceil(u16::from(ENVIRONMENT_PAGE_SAMPLES));
        let mut current = Vec::new();
        for y in 0..page_count {
            for x in 0..page_count {
                let (width, height) = page_shape(samples, x, y);
                current.push(ElevationPage {
                    level,
                    x,
                    y,
                    width,
                    height,
                    geographic_height_centimeters: vec![
                        0;
                        usize::from(width) * usize::from(height)
                    ],
                });
            }
        }
        levels.push(PyramidLevel {
            samples_per_axis: samples,
            ordered_page_root: ordered_page_root(&current).expect("synthetic elevation root"),
        });
        pages.extend(current);
    }
    (pages, levels)
}

fn water_pyramid(axes: &[(u8, u16)]) -> (Vec<WaterPage>, Vec<PyramidLevel>) {
    let mut pages = Vec::new();
    let mut levels = Vec::new();
    for &(level, samples) in axes {
        let page_count = samples.div_ceil(u16::from(ENVIRONMENT_PAGE_SAMPLES));
        let mut current = Vec::new();
        for y in 0..page_count {
            for x in 0..page_count {
                let (width, height) = page_shape(samples, x, y);
                let count = usize::from(width) * usize::from(height);
                current.push(WaterPage {
                    level,
                    x,
                    y,
                    width,
                    height,
                    ocean_coverage_percent: vec![0; count],
                    inland_coverage_percent: vec![0; count],
                });
            }
        }
        levels.push(PyramidLevel {
            samples_per_axis: samples,
            ordered_page_root: ordered_water_page_root(&current).expect("synthetic water root"),
        });
        pages.extend(current);
    }
    (pages, levels)
}

fn desert_pyramid(axes: &[(u8, u16)]) -> (Vec<PotentialBiomePage>, Vec<PyramidLevel>) {
    let mut pages = Vec::new();
    let mut levels = Vec::new();
    for &(level, samples) in axes {
        let page_count = samples.div_ceil(u16::from(ENVIRONMENT_PAGE_SAMPLES));
        let mut current = Vec::new();
        for y in 0..page_count {
            for x in 0..page_count {
                let (width, height) = page_shape(samples, x, y);
                current.push(PotentialBiomePage {
                    level,
                    x,
                    y,
                    width,
                    height,
                    potential_biome_class: vec![27; usize::from(width) * usize::from(height)],
                });
            }
        }
        levels.push(PyramidLevel {
            samples_per_axis: samples,
            ordered_page_root: ordered_biome_page_root(&current)
                .expect("synthetic desert-biome root"),
        });
        pages.extend(current);
    }
    (pages, levels)
}

fn persist(directory: &Path, fixture: &PersistedFixture) {
    crate::map_store::persist_prepared(
        Some(directory),
        &fixture.package,
        &fixture.elevation,
        &fixture.water,
        &fixture.vegetation,
        &[],
    )
    .expect("persist and verify synthetic source fixture");
}

#[tokio::test]
async fn synthetic_flat_sahara_package_exercises_full_offline_qualification_contract() {
    let directory = tempfile::tempdir().expect("synthetic qualification storage");
    // The 1,024-sample three-field pyramid indexes 1,041 persisted pages,
    // exercising real startup verification and eviction beyond the 128-page cap.
    let primary = fixture(50_000, PRIMARY_SAMPLE_AXIS);
    let primary_hash = primary.package.content_hash_hex();
    persist(directory.path(), &primary);

    let mut scale_references = Vec::new();
    for tiles_per_side in [512, 16_384, 262_144] {
        let scale_fixture = fixture(tiles_per_side, 64);
        let content_hash = scale_fixture.package.content_hash_hex();
        persist(directory.path(), &scale_fixture);
        scale_references.push(SourceScalePackageReference {
            tiles_per_side,
            directory: directory.path().to_owned(),
            content_hash,
        });
    }

    // All source locks/pages above are deterministic test fixtures. The call
    // exercises actual page verification, residency, resource lifecycle,
    // fixed-distance movement/replay, route planning, scale sampling and report
    // agreement, but intentionally writes no qualification evidence artifact.
    let report = run_source_qualification_with_geographic_reference(
        directory.path(),
        &primary_hash,
        None,
        None,
        &scale_references,
        MAX_ROUTE_TICKS,
        |progress| {
            eprintln!(
                "synthetic qualification progress: tick={} leg={} moved_meters={:.1}",
                progress.tick, progress.leg, progress.moved_meters
            );
        },
    )
    .await
    .expect("complete synthetic offline qualification");

    assert_eq!(report.package_hash, primary_hash);
    assert_eq!(report.start_tile, [24_999, 24_999]);
    assert_eq!(report.source_lock_ids, [SOURCE_LOCK_ID.to_owned()]);
    assert_eq!(report.indexed_page_count, 1_041);
    assert_eq!(report.page_churn_unique_count, report.indexed_page_count);
    assert!(report.indexed_page_count > MAX_RESIDENT_PAGES);
    assert!(report.peak_resident_page_count_per_provider <= MAX_RESIDENT_PAGES);
    assert!(report.evicted_page_reloaded_same_hash);
    assert!(report.resource_overlay_reloaded_equal);
    assert!(report.resource_overlay_revision > 0);
    assert!((24_967..=25_031).contains(&report.resource_tile[0]));
    assert!((24_967..=25_031).contains(&report.resource_tile[1]));
    let centered_resource = primary
        .centered_resource
        .expect("synthetic source fixture includes a centered resource");
    assert_eq!(
        centered_resource,
        ResourceNode {
            id: 13_089_948_530,
            tile: TileCoord::new(25_017, 24_967),
            kind: aoe_map::ResourceKind::Stone,
            object: aoe_map::ObjectKind::StoneDeposit,
            initial_amount: 350,
            visual_variant: 57,
        }
    );
    assert_eq!(report.resource_id, centered_resource.id);
    assert_eq!(
        report.resource_tile,
        [centered_resource.tile.x, centered_resource.tile.y]
    );
    assert_eq!(report.route_evidence.moved_meters, 100_000.0);
    assert_eq!(report.route_evidence.repetition_count, 25_000);
    assert!(report.route_evidence.replay_matches);
    assert_eq!(report.route_evidence.replay_hash, report.replay_hash);
    assert_eq!(report.movement_ticks, report.simulation_work.movement_ticks);
    assert!(report.movement_ticks <= MAX_ROUTE_TICKS);
    assert!(report.route_evidence.replay_hash.len() == 64);

    assert!(report.geographic_navigation.is_none());

    let axes = report
        .source_workload_contracts
        .iter()
        .map(|contract| contract.tiles_per_side)
        .collect::<Vec<_>>();
    assert_eq!(axes, [512, 16_384, 50_000, 262_144]);
    assert!(
        report
            .source_workload_contracts
            .iter()
            .all(|contract| contract.source_evidence.is_some())
    );
    let serialized = serde_json::to_value(&report).expect("serialize in-memory report contract");
    assert_eq!(
        serialized["qualification_sections"]["geographic_long_distance_navigation"],
        "not_run"
    );
    assert_eq!(
        serialized["page_churn_unique_count"],
        report.indexed_page_count
    );
}

#[test]
fn synthetic_flat_sahara_geographic_movement_exercises_full_routes_and_replay() {
    let directory = tempfile::tempdir().expect("synthetic geographic qualification storage");
    let geographic_fixture = fixture(50_000, PRIMARY_SAMPLE_AXIS);
    let content_hash = geographic_fixture.package.content_hash_hex();
    persist(directory.path(), &geographic_fixture);

    let mut progress = |progress: super::super::SourceQualificationProgress| {
        eprintln!(
            "synthetic geographic qualification progress: tick={} leg={} moved_meters={:.1}",
            progress.tick, progress.leg, progress.moved_meters
        );
    };
    let mut rss = super::super::metrics::ProcessRssSampler::start();
    let mut packages = crate::load_map_packages(Some(directory.path()))
        .expect("load persisted synthetic reference package");
    let package = packages
        .remove(&content_hash)
        .expect("persisted package matches its content hash");
    let planner_provider = crate::PageResidency::open(directory.path(), &package, &|| false)
        .expect("open verified synthetic planner pages");
    let planner_world = GameWorld::from_page_provider(
        package.clone(),
        planner_provider.clone() as Arc<dyn EnvironmentPageProvider>,
    )
    .expect("construct synthetic planner terrain");
    let config = planner_world.config();
    let start = match planner_world
        .terrain()
        .search_start_checked(config, 64, || false)
        .expect("synthetic activation search")
    {
        StartSearchResult::Found(tile) => tile,
        other => panic!("synthetic package should find an ordinary start: {other:?}"),
    };
    assert_eq!(start, TileCoord::new(24_999, 24_999));
    let waypoints = super::super::geographic::planned_waypoints(start)
        .expect("fixed 100 km route stays inside map");
    assert_eq!(waypoints.len(), 5);

    let planner_smoke = super::super::geographic::plan_order(
        planner_world.terrain(),
        start,
        TileCoord::new(start.x + 10, start.y),
        "synthetic-short-route-planner-smoke",
        Some(1),
    )
    .expect("short fixed route planner smoke");
    assert_eq!(
        planner_smoke.outcome,
        super::super::navigation_report::RoutePlanningOutcome::Complete
    );
    assert!(planner_smoke.expansions > 0);
    drop(planner_world);
    drop(planner_provider);

    let movement = super::super::geographic::execute_movement(
        directory.path(),
        &package,
        start,
        &waypoints,
        MAX_ROUTE_TICKS,
        &mut progress,
        &mut rss,
    )
    .expect("complete synthetic five-order movement and replay");

    assert_eq!(movement.completed_orders, 5);
    assert!(movement.measured_distance_meters >= 100_000.0);
    assert!(movement.speed_within_one_percent);
    assert!(movement.movement_ticks <= MAX_ROUTE_TICKS);
    assert!(movement.replay_matches);
    assert_eq!(movement.replay_hash.len(), 64);
    assert!(movement.route_checkpoint_count > 0);
    assert!(movement.route_page_loads > 0);
    assert!(movement.replay_page_loads > 0);
    assert!(movement.peak_resident_pages_per_provider <= MAX_RESIDENT_PAGES);
    assert!(movement.spatial_extent_tiles[0] >= 10_000);
    assert_eq!(movement.spatial_extent_tiles[1], 0);
    assert_eq!(movement.unique_corridor_count, 1);
    assert!(movement.peak_combined_navigation_cache_logical_retained_bytes <= 128 * 1024 * 1024);
}
