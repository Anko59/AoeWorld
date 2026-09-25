use super::*;
use crate::PreparedOverview;
use aoe_map::Ratio;
use aoe_map::{
    ElevationPage, EnvironmentalProvenance, HistoricalLandUsePage, HydrologyEvidenceIndex,
    HydrologyEvidenceMethod, HydrologyEvidencePage, HydrologyKind, HydrologyWaterPolicy,
    ModernLandCoverPage, PageLayer, PotentialBiomePage, PreparedEnvironment, ProjectionMetadata,
    WaterPage, ordered_hydrology_page_root, ordered_modern_land_cover_page_root,
};
use gdal::{DriverManager, raster::Buffer, spatial_ref::SpatialRef};
use std::{
    collections::BTreeSet,
    fs,
    sync::atomic::{AtomicU64, Ordering},
};

#[test]
fn public_tile_names_match_the_aws_one_degree_catalog() {
    assert_eq!(
        tile_prefix(48, 2, "30"),
        "Copernicus_DSM_COG_30_N48_00_E002_00_DEM"
    );
    assert_eq!(
        tile_prefix(-1, -7, "10"),
        "Copernicus_DSM_COG_10_S01_00_W007_00_DEM"
    );
}

#[test]
fn detailed_sample_cap_is_rejected_before_any_source_access() {
    let result = prepare_detailed_directory(
        std::env::temp_dir().join("aoe-detailed-test-cache"),
        std::env::temp_dir().join("aoe-detailed-test-output"),
        MapRequest::default(),
        MAX_DETAILED_SAMPLES_PER_AXIS + 1,
        DemResolution::Glo90,
    );
    assert!(matches!(result, Err(GeodataError::Preparation(_))));
}

#[test]
fn oversized_tile_extent_is_rejected_before_overview_acquisition() {
    let request = MapRequest {
        requested_side_meters: 2_000_000,
        compression: Ratio::new(10_000, 1).expect("valid ratio"),
        ..MapRequest::default()
    };
    let cache = std::env::temp_dir().join("aoe-detailed-overlarge-cache");
    let result = prepare_detailed_directory(
        cache,
        std::env::temp_dir().join("aoe-detailed-overlarge-output"),
        request,
        16,
        DemResolution::Glo90,
    );
    assert!(matches!(result, Err(GeodataError::Preparation(reason)) if reason.contains("64 tile")));
}

#[test]
fn footprint_containing_the_pole_is_rejected_before_source_access() {
    let request = MapRequest {
        center_latitude_e7: 875_000_000,
        requested_side_meters: 500_000,
        compression: Ratio::new(1, 1).expect("valid ratio"),
        ..MapRequest::default()
    };
    let result = prepare_detailed_directory(
        std::env::temp_dir().join("aoe-detailed-polar-cache"),
        std::env::temp_dir().join("aoe-detailed-polar-output"),
        request,
        16,
        DemResolution::Glo90,
    );
    assert!(
        matches!(result, Err(GeodataError::Preparation(reason)) if reason.contains("geographic boundary"))
    );
}

#[test]
fn antimeridian_footprint_is_rejected_without_coordinate_overflow() {
    let request = MapRequest {
        center_longitude_e7: 1_799_900_000,
        ..MapRequest::default()
    };
    let result = prepare_detailed_directory(
        std::env::temp_dir().join("aoe-detailed-antimeridian-cache"),
        std::env::temp_dir().join("aoe-detailed-antimeridian-output"),
        request,
        16,
        DemResolution::Glo90,
    );
    assert!(
        matches!(result, Err(GeodataError::Preparation(reason)) if reason.contains("geographic boundary"))
    );
}

#[test]
fn nearest_resampling_uses_the_center_of_each_coarse_cell() {
    assert_eq!(super::pyramid::coarse_coordinate(256, 0), 0);
    assert_eq!(super::pyramid::coarse_coordinate(256, 1), 0);
    assert_eq!(super::pyramid::coarse_coordinate(256, 127), 63);
    assert_eq!(super::pyramid::coarse_coordinate(256, 128), 64);
    assert_eq!(super::pyramid::coarse_coordinate(256, 255), 127);
    assert_eq!(super::pyramid::coarse_coordinate(64, 0), 1);
    assert_eq!(super::pyramid::coarse_coordinate(64, 63), 127);
}

#[test]
fn tile_candidates_include_both_sides_of_exact_geocell_boundaries() {
    let below = super::sampler::tile_candidate_keys(48.999_999_999, 1.999_999_999);
    let above = super::sampler::tile_candidate_keys(49.0, 2.0);
    assert!(below.contains(&(48, 1)) && below.contains(&(49, 2)));
    assert!(above.contains(&(48, 1)) && above.contains(&(49, 2)));
}

#[test]
fn only_authoritative_missing_tiles_can_supply_ocean_zero() {
    let bounds = super::Bounds {
        min_latitude: 47,
        max_latitude: 49,
        min_longitude: 1,
        max_longitude: 3,
    };
    let absent = BTreeSet::from([(48, 2)]);
    assert_eq!(
        super::sampler::missing_tile_value(bounds, &absent, (48, 2), None, Some(100))
            .expect("404 ocean"),
        0
    );
    assert!(super::sampler::missing_tile_value(bounds, &absent, (48, 2), None, Some(0)).is_err());
    assert!(super::sampler::missing_tile_value(bounds, &absent, (48, 2), None, None).is_err());
    assert!(super::sampler::missing_tile_value(bounds, &absent, (50, 2), None, Some(100)).is_err());
}

#[test]
fn sampler_reads_native_geotiff_and_rejects_invalid_geotransforms() {
    let root = temporary_directory();
    fs::create_dir_all(&root).expect("temporary directory");
    let valid = root.join("valid.tif");
    write_tile(&valid, [1.0, 0.01, 0.0, 50.5, 0.0, -0.01], 12.34, None);
    let tile = test_tile(valid.clone(), 48, 2);
    let bounds = Bounds {
        min_latitude: 47,
        max_latitude: 49,
        min_longitude: 1,
        max_longitude: 3,
    };
    let mut sampler = Sampler::new(
        MapRequest::default(),
        30_000,
        bounds,
        BTreeSet::new(),
        vec![tile],
        vec![0; 128 * 128],
    )
    .expect("sampler");
    let page = sampler.page(65, 0, 1, 1).expect("native tile page");
    assert_eq!(page.width, 1);
    assert_eq!(page.height, 1);
    assert_eq!(page.geographic_height_centimeters, vec![1234]);

    let rotated = root.join("rotated.tif");
    write_tile(&rotated, [1.0, 0.01, 0.1, 50.5, 0.0, -0.01], 12.34, None);
    let mut sampler = Sampler::new(
        MapRequest::default(),
        30_000,
        bounds,
        BTreeSet::new(),
        vec![test_tile(rotated, 48, 2)],
        vec![0; 128 * 128],
    )
    .expect("rotated sampler");
    assert!(matches!(
        sampler.page(2, 0, 0, 0),
        Err(GeodataError::Preparation(reason)) if reason.contains("geotransform")
    ));
    fs::remove_dir_all(root).expect("remove tile fixtures");
}

#[test]
fn detailed_pyramid_streams_native_pages_and_retains_overview_layers() {
    let root = temporary_directory();
    fs::create_dir_all(&root).expect("temporary directory");
    let tile_path = root.join("tile.tif");
    write_tile(&tile_path, [1.0, 0.01, 0.0, 50.5, 0.0, -0.01], 5.0, None);
    let bounds = Bounds {
        min_latitude: 47,
        max_latitude: 49,
        min_longitude: 1,
        max_longitude: 3,
    };
    let mut sampler = Sampler::new(
        MapRequest::default(),
        30_000,
        bounds,
        BTreeSet::new(),
        vec![test_tile(tile_path, 48, 2)],
        vec![0; 128 * 128],
    )
    .expect("sampler");
    let stage = Stage::new(&root).expect("staging");
    let overview = overview_fixture();
    let hydrology_pages = vec![HydrologyEvidencePage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        kind: vec![HydrologyKind::NoEvidence as u8; 4],
        method: vec![HydrologyEvidenceMethod::None as u8; 4],
    }];
    let modern_land_cover_pages = vec![ModernLandCoverPage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        worldcover_class: vec![40; 4],
    }];
    let hydrology = crate::PreparedHydrology {
        samples_per_axis: 2,
        evidence_index: HydrologyEvidenceIndex {
            samples_per_axis: 2,
            page_samples: aoe_map::ENVIRONMENT_PAGE_SAMPLES,
            world_cover_year: aoe_map::WORLD_COVER_OBSERVATION_YEAR,
            policy: HydrologyWaterPolicy::HistoricalOverviewWithMappedNaturalWaterV1,
            hydrology_page_root: ordered_hydrology_page_root(&hydrology_pages)
                .expect("hydrology root"),
            modern_land_cover_page_root: ordered_modern_land_cover_page_root(
                &modern_land_cover_pages,
            )
            .expect("cover root"),
        },
        source_locks: vec![],
        hydrology_pages,
        modern_land_cover_pages,
    };
    let progress_path = root.join("progress.json");
    let _progress = crate::preparation_progress::Scope::new(Some(progress_path.clone()));
    let fields = super::pyramid::build_pyramids(&mut sampler, &stage, 65, &overview, &hydrology)
        .expect("detailed pyramids");
    let progress: serde_json::Value =
        serde_json::from_slice(&fs::read(progress_path).unwrap()).unwrap();
    assert_eq!(progress["phase"], "building_pyramids");
    assert_eq!(progress["completed"], 44);
    assert_eq!(progress["total"], 44);
    assert_eq!(fields.elevation.levels.len(), 8);
    assert_eq!(fields.water.levels[0].samples_per_axis, 65);
    assert_eq!(fields.vegetation.levels[0].samples_per_axis, 65);
    assert_eq!(fields.historical_land_use.levels[0].samples_per_axis, 65);
    assert_ne!(fields.elevation.levels[0].ordered_page_root, [0; 32]);
    for (level, metadata) in fields.elevation.levels.iter().enumerate() {
        let level = level as u8;
        let edge = metadata.samples_per_axis.div_ceil(PAGE) - 1;
        let elevation: ElevationPage = serde_json::from_slice(
            &stage
                .read(PageLayer::Elevation, level, edge, edge)
                .expect("staged elevation edge page"),
        )
        .expect("elevation page JSON");
        assert_eq!(
            elevation.geographic_height_centimeters,
            vec![500; usize::from(elevation.width) * usize::from(elevation.height)]
        );

        let water: WaterPage = serde_json::from_slice(
            &stage
                .read(PageLayer::Water, level, edge, edge)
                .expect("staged water edge page"),
        )
        .expect("water page JSON");
        assert_eq!(
            water.ocean_coverage_percent,
            vec![100; usize::from(water.width) * usize::from(water.height)]
        );
        assert_eq!(
            water.inland_coverage_percent,
            vec![0; usize::from(water.width) * usize::from(water.height)]
        );

        let vegetation: PotentialBiomePage = serde_json::from_slice(
            &stage
                .read(PageLayer::Vegetation, level, edge, edge)
                .expect("staged vegetation edge page"),
        )
        .expect("vegetation page JSON");
        assert_eq!(
            vegetation.potential_biome_class,
            vec![7; usize::from(vegetation.width) * usize::from(vegetation.height)]
        );

        let historical: HistoricalLandUsePage = serde_json::from_slice(
            &stage
                .read(PageLayer::HistoricalLandUse, level, edge, edge)
                .expect("staged historical edge page"),
        )
        .expect("historical page JSON");
        let size = usize::from(historical.width) * usize::from(historical.height);
        assert_eq!(historical.crop_percent, vec![1; size]);
        assert_eq!(historical.grazing_percent, vec![2; size]);
        assert_eq!(
            historical.population_pressure_per_square_kilometer,
            vec![3; size]
        );
    }
    assert!(
        stage
            .write(
                PageLayer::Elevation,
                0,
                99,
                99,
                &vec![0; crate::MAX_DIRECTORY_PAGE_BYTES as usize + 1],
            )
            .is_err()
    );
    fs::remove_dir_all(root).expect("remove pyramid fixtures");
}

fn test_tile(path: std::path::PathBuf, latitude: i32, longitude: i32) -> Tile {
    Tile {
        latitude,
        longitude,
        path,
        lock: SourceLock {
            id: "fixture-tile".to_owned(),
            provider: Provider::Copernicus,
            release: "fixture".to_owned(),
            url: "https://copernicus-dem-90m.s3.amazonaws.com/fixture.tif".to_owned(),
            sha256: "a".repeat(64),
            bytes: 1,
            native_resolution: "3 arc-seconds".to_owned(),
            crs: "EPSG:4326".to_owned(),
            vertical_datum: "EGM2008 orthometric".to_owned(),
            license_reference: "fixture".to_owned(),
        },
    }
}

fn write_tile(path: &std::path::Path, transform: [f64; 6], value: f64, nodata: Option<f64>) {
    let driver = DriverManager::get_driver_by_name("GTiff").expect("GTiff driver");
    let mut dataset = driver
        .create_with_band_type::<f64, _>(path, 200, 320, 1)
        .expect("tile raster");
    dataset
        .set_geo_transform(&transform)
        .expect("tile transform");
    dataset
        .set_spatial_ref(&SpatialRef::from_epsg(4326).expect("WGS84"))
        .expect("tile SRS");
    let mut band = dataset.rasterband(1).expect("tile band");
    if let Some(nodata) = nodata {
        band.set_no_data_value(Some(nodata)).expect("tile nodata");
    }
    let mut values = Buffer::new((200, 320), vec![value; 200 * 320]);
    band.write((0, 0), (200, 320), &mut values)
        .expect("tile values");
    dataset.flush_cache().expect("flush tile");
}

fn overview_fixture() -> PreparedOverview {
    let water_pages = [(0, 0), (1, 0), (0, 1), (1, 1)]
        .into_iter()
        .map(|(x, y)| WaterPage {
            level: 0,
            x,
            y,
            width: 64,
            height: 64,
            ocean_coverage_percent: vec![100; 64 * 64],
            inland_coverage_percent: vec![0; 64 * 64],
        })
        .collect();
    let vegetation_pages = [(0, 0), (1, 0), (0, 1), (1, 1)]
        .into_iter()
        .map(|(x, y)| PotentialBiomePage {
            level: 0,
            x,
            y,
            width: 64,
            height: 64,
            potential_biome_class: vec![7; 64 * 64],
        })
        .collect();
    let historical_land_use_pages = [(0, 0), (1, 0), (0, 1), (1, 1)]
        .into_iter()
        .map(|(x, y)| HistoricalLandUsePage {
            level: 0,
            x,
            y,
            width: 64,
            height: 64,
            crop_percent: vec![1; 64 * 64],
            grazing_percent: vec![2; 64 * 64],
            population_pressure_per_square_kilometer: vec![3; 64 * 64],
        })
        .collect();
    PreparedOverview {
        source_lock: fixture_source_lock("overview"),
        water_source_lock: fixture_source_lock("water"),
        vegetation_source_lock: fixture_source_lock("vegetation"),
        vegetation_classes_source_lock: fixture_source_lock("classes"),
        hyde_baseline_source_lock: fixture_source_lock("baseline"),
        hyde_supplementary_source_lock: fixture_source_lock("supplementary"),
        hyde_readme_source_lock: fixture_source_lock("readme"),
        projection: ProjectionMetadata::default(),
        provenance: EnvironmentalProvenance::default(),
        environment: PreparedEnvironment::default(),
        pages: Vec::new(),
        water_pages,
        vegetation_pages,
        historical_land_use_pages,
    }
}

#[test]
fn source_backed_overview_ocean_rejects_provenance_and_grid_corruption() {
    let mut fixture = overview_fixture();
    fixture.provenance.water = LayerProvenance::SourceDerived;
    let values = super::source_backed_overview_ocean(&fixture).expect("complete grid");
    assert_eq!(values.len(), 128 * 128);
    assert!(values.iter().all(|value| *value == 100));

    fixture.provenance.water = LayerProvenance::Fallback;
    assert!(super::source_backed_overview_ocean(&fixture).is_err());

    let mut incomplete = overview_fixture();
    incomplete.provenance.water = LayerProvenance::SourceDerived;
    incomplete.water_pages.pop();
    assert!(super::source_backed_overview_ocean(&incomplete).is_err());

    let mut duplicate = overview_fixture();
    duplicate.provenance.water = LayerProvenance::SourceDerived;
    duplicate.water_pages.push(duplicate.water_pages[0].clone());
    assert!(super::source_backed_overview_ocean(&duplicate).is_err());

    let mut outside = overview_fixture();
    outside.provenance.water = LayerProvenance::SourceDerived;
    outside.water_pages[0].x = 2;
    assert!(super::source_backed_overview_ocean(&outside).is_err());
}

fn fixture_source_lock(id: &str) -> aoe_map::SourceLock {
    aoe_map::SourceLock {
        id: id.to_owned(),
        provider: "fixture".to_owned(),
        release: "fixture".to_owned(),
        url: "https://example.invalid/fixture".to_owned(),
        sha256: [1; 32],
        acquired_at: "fixture".to_owned(),
        native_resolution: "fixture".to_owned(),
        crs: "EPSG:4326".to_owned(),
        vertical_datum: "fixture".to_owned(),
        license: "fixture".to_owned(),
        preprocessing_version: "fixture".to_owned(),
    }
}

fn temporary_directory() -> std::path::PathBuf {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "aoe-copernicus-test-{}-{serial}",
        std::process::id()
    ))
}

#[test]
fn worker_staging_lease_rejects_missing_or_recovering_scope() {
    let root = temporary_directory();
    fs::create_dir_all(&root).expect("scope");
    assert!(Stage::lease(&root).is_err());
    let path = root.join("lease");
    let owner = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)
        .expect("lease");
    owner.lock().expect("exclusive recovery");
    assert!(Stage::lease(&root).is_err());
    owner.unlock().expect("release recovery");
    owner.lock_shared().expect("parent lease");
    let worker = Stage::lease(&root).expect("worker shares parent lease");
    let contender = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .expect("contender");
    drop(owner);
    assert!(matches!(
        contender.try_lock(),
        Err(fs::TryLockError::WouldBlock)
    ));
    drop(worker);
    contender.try_lock().expect("released worker lease");
    fs::remove_dir_all(root).expect("cleanup");
}
