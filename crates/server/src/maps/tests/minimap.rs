use super::super::minimap::{
    FLAT_SHADE, MAX_BACKGROUND_MINIMAPS, MAX_CACHED_MINIMAPS, MAX_MINIMAP_BYTES,
    MAX_MINIMAP_SAMPLES_PER_AXIS, MINIMAP_HEADER_BYTES, MINIMAP_MAGIC, MinimapCache, MinimapClass,
    MinimapError, classify, finish, minimap, minimap_raster, minimap_samples, minimap_tile,
    package_minimap,
};
use super::MemoryPageProvider;
use aoe_core::TileCoord;
use aoe_map::{
    ElevationPage, EnvironmentPage, EnvironmentPageError, EnvironmentalProvenance, FieldPyramid,
    MAP_SCHEMA_VERSION, MapChunkGenerator, MapPackage, MapRequest, PotentialBiomePage,
    PreparedEnvironment, ProjectionMetadata, PyramidLevel, Ratio, WaterPage,
};
use axum::{
    body::to_bytes,
    extract::{Path, State},
    http::{StatusCode, header},
};
use std::sync::Arc;

fn small_fallback_package() -> MapPackage {
    let request = MapRequest {
        requested_side_meters: 328,
        compression: Ratio::new(2, 1).expect("ratio"),
        ..MapRequest::default()
    };
    MapPackage::new(MAP_SCHEMA_VERSION, request, Vec::new()).expect("fallback package")
}

fn pyramid() -> FieldPyramid {
    FieldPyramid {
        levels: [4, 2, 1]
            .into_iter()
            .map(|samples_per_axis| PyramidLevel {
                samples_per_axis,
                ordered_page_root: [7; 32],
            })
            .collect(),
    }
}

/// A 4×4 source grid: an ocean column in the west, level forest country with
/// one lake in the middle, and a mountain column in the east.
fn coast_forest_mountain() -> (MapPackage, MapChunkGenerator) {
    let request = MapRequest {
        requested_side_meters: 15_360,
        ..MapRequest::default()
    };
    let environment = PreparedEnvironment {
        samples_per_axis: 4,
        geographic_millimeters_per_sample: 3_840_000,
        page_samples: 64,
        elevation: pyramid(),
        water: Some(pyramid()),
        vegetation: Some(pyramid()),
        historical_land_use: None,
        hydrology_evidence: None,
    };
    let package = MapPackage::with_prepared_environment(
        MAP_SCHEMA_VERSION,
        request,
        Vec::new(),
        ProjectionMetadata::default(),
        EnvironmentalProvenance::default(),
        environment,
    )
    .expect("synthetic source package");
    let row = |values: [i32; 4]| (0..4).flat_map(move |_| values).collect::<Vec<_>>();
    let mut inland = vec![0_u8; 16];
    inland[2 * 4 + 1] = 100;
    let pages = [
        EnvironmentPage::Elevation(ElevationPage {
            level: 0,
            x: 0,
            y: 0,
            width: 4,
            height: 4,
            geographic_height_centimeters: row([-5_000, 20_000, 20_000, 300_000]),
        }),
        EnvironmentPage::Water(WaterPage {
            level: 0,
            x: 0,
            y: 0,
            width: 4,
            height: 4,
            ocean_coverage_percent: row([100, 0, 0, 0]).into_iter().map(|v| v as u8).collect(),
            inland_coverage_percent: inland,
        }),
        EnvironmentPage::Vegetation(PotentialBiomePage {
            level: 0,
            x: 0,
            y: 0,
            width: 4,
            height: 4,
            potential_biome_class: vec![14; 16],
        }),
    ];
    let mut provider = MemoryPageProvider::default();
    for page in pages {
        provider.pages.insert(page.key(), Arc::new(page));
    }
    let generator = package
        .generator_with_page_provider(Arc::new(provider))
        .expect("provider generator");
    (package, generator)
}

fn raster_of(package: &MapPackage, generator: &MapChunkGenerator) -> Vec<u8> {
    minimap_raster(
        generator,
        package.estimate.tiles_per_side,
        package.estimate.geographic_millimeters_per_tile,
        package.environment.samples_per_axis,
        &|| false,
    )
    .expect("raster")
}

fn count(raster: &[u8], class: MinimapClass) -> usize {
    raster[MINIMAP_HEADER_BYTES..]
        .iter()
        .filter(|cell| *cell & 0x0f == class as u8)
        .count()
}

#[path = "minimap_rivers.rs"]
mod rivers;

#[path = "minimap_relief.rs"]
mod relief;

#[path = "minimap_dump.rs"]
mod dump;

#[test]
fn raster_axis_and_bytes_are_capped_for_every_map_size() {
    assert_eq!(MAX_MINIMAP_SAMPLES_PER_AXIS, 512);
    assert_eq!(MAX_MINIMAP_BYTES, 8 + 512 * 512);
    assert_eq!(minimap_samples(0), 0);
    assert_eq!(minimap_samples(1), 1);
    assert_eq!(minimap_samples(500), 500);
    assert_eq!(minimap_samples(513), 512);
    assert_eq!(minimap_samples(20_000), 512);
    assert_eq!(minimap_samples(u64::MAX), 512);
}

#[test]
fn sample_centres_are_monotonic_and_stay_inside_the_map() {
    assert_eq!(minimap_tile(1, 1, 0), Some(0));
    assert_eq!(minimap_tile(500, 500, 0), Some(0));
    assert_eq!(minimap_tile(500, 500, 499), Some(499));
    assert_eq!(minimap_tile(20_000, 512, 0), Some(19));
    assert_eq!(minimap_tile(20_000, 512, 511), Some(19_980));
    assert_eq!(minimap_tile(20_000, 512, 512), None);
    assert_eq!(minimap_tile(0, 0, 0), None);
    assert_eq!(minimap_tile(u64::MAX, 512, 511), None);
    assert_eq!(minimap_tile(u64::from(u32::MAX), 512, 511), None);
    let mut previous = -1;
    for index in 0..512 {
        let tile = minimap_tile(1_000, 512, index).expect("tile");
        assert!(tile > previous && tile < 1_000);
        previous = tile;
    }
}

#[test]
fn raster_has_the_documented_header_and_one_byte_per_sample() {
    let package = small_fallback_package();
    let samples = minimap_samples(package.estimate.tiles_per_side);
    assert_eq!(u64::from(samples), package.estimate.tiles_per_side);
    let raster = raster_of(&package, &package.generator());
    assert_eq!(raster[..4], MINIMAP_MAGIC);
    assert_eq!(raster[4..6], samples.to_le_bytes());
    assert_eq!(raster[6..8], [0, 0]);
    assert_eq!(
        raster.len(),
        MINIMAP_HEADER_BYTES + usize::from(samples).pow(2)
    );
    assert!(raster.len() <= MAX_MINIMAP_BYTES);
    assert!(
        raster[MINIMAP_HEADER_BYTES..]
            .iter()
            .all(|cell| cell & 0x0f <= MinimapClass::Ocean as u8)
    );
}

#[test]
fn raster_is_deterministic_and_agrees_with_the_composed_world() {
    let (package, generator) = coast_forest_mountain();
    let raster = raster_of(&package, &generator);
    assert_eq!(raster, raster_of(&package, &generator));
    let tiles = package.estimate.tiles_per_side;
    let samples = minimap_samples(tiles);
    for (x, y) in [(0, 0), (3, 200), (100, 17), (140, 140), (samples - 1, 9)] {
        let tile = TileCoord::new(
            minimap_tile(tiles, samples, x).expect("x"),
            minimap_tile(tiles, samples, y).expect("y"),
        );
        let point = generator
            .landscape_point_with_cancel(tile, &|| false)
            .expect("point")
            .expect("inside");
        let cell =
            raster[MINIMAP_HEADER_BYTES + usize::from(y) * usize::from(samples) + usize::from(x)];
        assert_eq!(cell & 0x0f, classify(&point) as u8, "sample {x},{y}");
    }
}

#[test]
fn synthetic_coast_forest_and_mountain_are_distinct_classes() {
    let (package, generator) = coast_forest_mountain();
    let raster = raster_of(&package, &generator);
    let cells = raster.len() - MINIMAP_HEADER_BYTES;
    for class in [
        MinimapClass::Ocean,
        MinimapClass::Lake,
        MinimapClass::Forest,
        MinimapClass::Grass,
        MinimapClass::Rock,
    ] {
        assert!(count(&raster, class) > cells / 200, "{class:?} is missing");
    }
    // Water is drawn flat; the mountain flank is visibly lit or shaded.
    assert!(
        raster[MINIMAP_HEADER_BYTES..]
            .iter()
            .all(|cell| { cell & 0x0f < MinimapClass::Shore as u8 || cell >> 4 == FLAT_SHADE })
    );
    assert!(
        raster[MINIMAP_HEADER_BYTES..]
            .iter()
            .any(|cell| (cell >> 4).abs_diff(FLAT_SHADE) >= 4)
    );
}

#[test]
fn cancellation_and_missing_pages_fail_without_a_partial_raster() {
    let (package, generator) = coast_forest_mountain();
    assert_eq!(
        minimap_raster(
            &generator,
            package.estimate.tiles_per_side,
            60_000,
            4,
            &|| { true }
        ),
        Err(MinimapError::Page(EnvironmentPageError::Cancelled))
    );
    assert_eq!(
        minimap_raster(&generator, 0, 60_000, 4, &|| false),
        Err(MinimapError::EmptyMap)
    );
    let empty = package
        .generator_with_page_provider(Arc::new(MemoryPageProvider::default()))
        .expect("generator");
    assert_eq!(
        minimap_raster(&empty, package.estimate.tiles_per_side, 60_000, 4, &|| {
            false
        }),
        Err(MinimapError::Page(EnvironmentPageError::Missing))
    );
    assert_eq!(
        package_minimap(&package, None, &|| false),
        Err(MinimapError::Page(EnvironmentPageError::Missing))
    );
}

#[test]
fn cache_keeps_a_fixed_number_of_most_recent_rasters() {
    let mut cache = MinimapCache::default();
    for index in 0..MAX_CACHED_MINIMAPS + 3 {
        cache.insert(index.to_string(), Arc::from(vec![index as u8]));
        assert!(cache.get("0").is_some() || index >= MAX_CACHED_MINIMAPS);
    }
    assert_eq!(cache.len(), MAX_CACHED_MINIMAPS);
    assert!(cache.get("1").is_none());
    cache.insert("6".to_owned(), Arc::from(vec![9]));
    assert_eq!(cache.len(), MAX_CACHED_MINIMAPS);
    assert_eq!(cache.get("6").as_deref(), Some(&[9_u8][..]));
}

#[tokio::test]
async fn endpoint_serves_one_cached_binary_raster_per_package() {
    let config = crate::Config {
        bind: "127.0.0.1:0".parse().expect("bind"),
        scenario: aoe_scenario::SMOKE,
        tick_hz: 20,
        asset_pack: None,
        map_package_directory: None,
        open_map_package: None,
        map_worker: None,
        geodata_cache_directory: ".cache/geodata".into(),
    };
    let state = crate::AppState::new(&config, "minimap-test").expect("state");
    let package = small_fallback_package();
    let hash = package.content_hash_hex();
    state
        .map_packages
        .write()
        .await
        .insert(hash.clone(), package.clone());
    let expected = raster_of(&package, &package.generator());
    for _ in 0..2 {
        let response = minimap(Path(hash.clone()), State(state.clone()))
            .await
            .expect("minimap");
        assert_eq!(
            response.headers()[header::CONTENT_TYPE],
            "application/octet-stream"
        );
        let body = to_bytes(response.into_body(), MAX_MINIMAP_BYTES)
            .await
            .expect("bounded body");
        assert_eq!(body.as_ref(), expected.as_slice());
        assert_eq!(state.minimaps.lock().await.len(), 1);
    }
    assert_eq!(
        minimap(Path("0".repeat(64)), State(state))
            .await
            .expect_err("unknown package"),
        StatusCode::NOT_FOUND
    );
}

fn slow_task(
    release: &Arc<std::sync::atomic::AtomicBool>,
    raster: u8,
) -> tokio::task::JoinHandle<Result<Vec<u8>, MinimapError>> {
    let release = release.clone();
    tokio::task::spawn_blocking(move || {
        while !release.load(std::sync::atomic::Ordering::Acquire) {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        Ok(vec![raster])
    })
}

#[tokio::test]
async fn a_raster_past_its_deadline_is_cached_for_the_retry_with_bounded_background_work() {
    let minimaps = Arc::new(tokio::sync::Mutex::new(MinimapCache::default()));
    let release = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let budget = std::time::Duration::from_millis(20);
    for index in 0..=MAX_BACKGROUND_MINIMAPS {
        let mut cache = minimaps.lock().await;
        let result = finish(
            &mut cache,
            &minimaps,
            index.to_string(),
            budget,
            slow_task(&release, index as u8),
            (),
        )
        .await;
        assert_eq!(
            result.expect_err("past deadline"),
            StatusCode::SERVICE_UNAVAILABLE
        );
        // The same hash is never computed twice at once.
        assert_eq!(
            cache.pending.contains(&index.to_string()),
            index < MAX_BACKGROUND_MINIMAPS
        );
    }
    assert_eq!(minimaps.lock().await.pending.len(), MAX_BACKGROUND_MINIMAPS);
    release.store(true, std::sync::atomic::Ordering::Release);
    for _ in 0..200 {
        if minimaps.lock().await.pending.is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let mut cache = minimaps.lock().await;
    assert!(cache.pending.is_empty());
    assert_eq!(cache.get("0").as_deref(), Some(&[0_u8][..]));
    assert_eq!(cache.get("1").as_deref(), Some(&[1_u8][..]));
    assert!(
        cache.get("2").is_none(),
        "the over-limit raster was not kept"
    );
}
