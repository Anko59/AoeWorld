use super::*;
use crate::copernicus::{OVERVIEW_DEM_IDENTITY, Tile};

const REDUCED: OverviewFieldAxes = OverviewFieldAxes {
    elevation: 128,
    vegetation: 32,
    water: 64,
    historical: 16,
};

/// A 1 degree GLO-90 stand-in holding one height, covering 48N 2E.
fn write_flat_tile(fixture: &OverviewFixture, meters: f32) -> Tile {
    let path = fixture.root.join("glo90-n48-e002.tif");
    let driver = DriverManager::get_driver_by_name("GTiff").expect("GTiff driver");
    let mut dataset = driver
        .create_with_band_type::<f32, _>(&path, 120, 120, 1)
        .expect("tile raster");
    dataset
        .set_geo_transform(&[2.0, 1.0 / 120.0, 0.0, 49.0, 0.0, -1.0 / 120.0])
        .expect("geotransform");
    dataset
        .set_spatial_ref(&SpatialRef::from_epsg(4326).expect("WGS84"))
        .expect("spatial reference");
    dataset
        .rasterband(1)
        .expect("band")
        .write(
            (0, 0),
            (120, 120),
            &mut Buffer::new((120, 120), vec![meters; 120 * 120]),
        )
        .expect("values");
    dataset.flush_cache().expect("flush tile");
    // Closing finishes the file, so the lock below describes its final bytes.
    drop(dataset);
    let mut lock = fixture_lock(
        &path,
        "copernicus-dem-3 arc-seconds-fixture-n48-e002",
        Provider::Copernicus,
        "copernicus-dem-90m.s3.amazonaws.com",
    );
    lock.native_resolution = "3 arc-seconds".to_owned();
    Tile {
        latitude: 48,
        longitude: 2,
        path,
        lock,
    }
}

fn prepare(sources: VerifiedOverviewSources) -> PreparedOverview {
    let request = MapRequest::default();
    let historical = GeographicHistoricalCorrectionDocument::empty(
        request,
        REDUCED.historical,
        hyde::HYDE_AREA_PREPROCESSING_IDENTITY,
    )
    .expect("history binding");
    let vegetation =
        VegetationPatchDocument::empty(request, REDUCED.vegetation).expect("PNV binding");
    prepare_overview_fields_from_verified_sources(
        request,
        REDUCED,
        true,
        &historical,
        &vegetation,
        sources,
    )
    .expect("offline overview")
}

#[test]
fn overview_elevation_for_a_landscape_request_uses_the_finer_source() {
    let fixture = OverviewFixture::new();
    let mut sources = fixture.sources.clone();
    sources.elevation_tiles = Some(vec![write_flat_tile(&fixture, 100.0)]);
    let prepared = prepare(sources);
    // The ETOPO stand-in reads 42 m everywhere; the tile reads 100 m.
    assert!(
        prepared.pages[0]
            .geographic_height_centimeters
            .iter()
            .all(|height| *height == 10_000)
    );
    assert_eq!(prepared.elevation_tile_source_locks.len(), 1);
    assert!(
        prepared.elevation_tile_source_locks[0]
            .preprocessing_version
            .contains(OVERVIEW_DEM_IDENTITY)
    );
    assert!(
        prepared
            .source_lock
            .preprocessing_version
            .contains("glo90-fill")
    );
    let generated = GeneratedMap::from_prepared(MapRequest::default(), prepared).expect("package");
    generated.validate().expect("package with tile locks");
    assert_eq!(generated.package.source_locks.len(), 8);
}

#[test]
fn overview_elevation_without_tiles_keeps_the_coarse_source() {
    let fixture = OverviewFixture::new();
    let prepared = prepare(fixture.sources.clone());
    assert!(
        prepared.pages[0]
            .geographic_height_centimeters
            .iter()
            .all(|height| *height == 4_200)
    );
    assert!(prepared.elevation_tile_source_locks.is_empty());
    assert!(!prepared.source_lock.preprocessing_version.contains("glo90"));
}

#[test]
fn production_wiring_selects_and_acquires_tiles_by_request_size() {
    let fixture = OverviewFixture::new();
    let cache_root = fixture.root.join("cache");
    let cache = SourceCache::new(
        cache_root.clone(),
        DownloadPolicy {
            cache_quota_bytes: DEFAULT_CACHE_QUOTA_BYTES,
            job_acquisition_budget_bytes: MAX_OVERVIEW_INPUT_BYTES,
        },
    )
    .expect("cache");
    // A verified cached N48 E002 tile: the Tiny footprint needs no network.
    let tile = write_flat_tile(&fixture, 100.0);
    let id = "copernicus-dem-3 arc-seconds-Copernicus_DSM_COG_30_N48_00_E002_00_DEM";
    let mut lock = tile.lock.clone();
    lock.id = id.to_owned();
    let object = cache.object_path(&lock).expect("object path");
    fs::create_dir_all(object.parent().expect("objects directory")).expect("objects");
    fs::copy(&tile.path, &object).expect("cached tile");
    fs::create_dir_all(cache_root.join("known")).expect("known directory");
    fs::write(
        cache_root.join("known").join(format!(
            "{}.json",
            digest_hex(&Sha256::digest(id.as_bytes()))
        )),
        serde_json::to_vec(&lock).expect("lock json"),
    )
    .expect("known lock");
    assert!(
        cache.known_lock(id).expect("known lock").is_some(),
        "seeded lock is not found"
    );
    assert!(
        cache.is_verified(&lock).expect("verify"),
        "object not verified"
    );
    let cancelled = std::sync::atomic::AtomicBool::new(false);
    // Centered inside the degree, so the footprint needs this tile alone.
    let tiny = MapRequest {
        center_latitude_e7: 485_000_000,
        center_longitude_e7: 25_000_000,
        ..MapRequest::default()
    };

    let tiles = acquire_elevation_tiles(&cache, tiny, true, &cancelled)
        .expect("tiny acquisition")
        .expect("a landscape overview selects GLO-90");
    assert_eq!(tiles.len(), 1);
    assert_eq!((tiles[0].latitude, tiles[0].longitude), (48, 2));
    assert_eq!(tiles[0].lock.id, id);
    // The coupled 128-sample context of detailed preparation keeps ETOPO.
    assert!(
        acquire_elevation_tiles(&cache, tiny, false, &cancelled)
            .expect("coupled")
            .is_none()
    );
    let paris = |side: u64| MapRequest {
        requested_side_meters: side,
        compression: aoe_map::Ratio::new(30, 1).expect("ratio"),
        ..MapRequest::default()
    };
    // Giant stays on ETOPO without any lookup.
    assert!(
        acquire_elevation_tiles(&cache, paris(1_920_000), true, &cancelled)
            .expect("giant")
            .is_none()
    );
    // Large selects the 48 tiles measured for Paris.
    let large = crate::copernicus::plan_overview_tiles(paris(480_000))
        .expect("large plan")
        .expect("Large selects GLO-90");
    assert_eq!(large.tile_count(), 48);
}
