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
