use super::*;
use aoe_map::MapRequest;
use gdal::{
    DriverManager,
    raster::Buffer,
    spatial_ref::SpatialRef,
    vector::{Geometry, LayerAccess, LayerOptions},
};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

const HYDE_WIDTH: usize = 4_320;
const HYDE_HEIGHT: usize = 2_160;
const HYDE_CELL_DEGREES: f64 = 0.083_333_3;
const HYDE_MEMBERS: [&str; 5] = [
    "baseline/asc/600AD_lu/cropland600AD.asc",
    "baseline/asc/600AD_lu/grazing600AD.asc",
    "baseline/asc/600AD_pop/popc_600AD.asc",
    "general_files/landlake.asc",
    "general_files/maxln_cr.asc",
];

#[test]
fn verified_source_paths_run_the_complete_offline_overview_pipeline() {
    let fixture = OverviewFixture::new();
    let request = MapRequest::default();
    let historical = GeographicHistoricalCorrectionDocument::empty(
        request,
        2,
        hyde::HYDE_AREA_PREPROCESSING_IDENTITY,
    )
    .expect("empty historical corrections");
    let vegetation = VegetationPatchDocument::empty(request, 2).expect("empty vegetation patches");

    let prepared = prepare_overview_from_verified_sources(
        request,
        2,
        2,
        &historical,
        &vegetation,
        fixture.sources.clone(),
    )
    .expect("offline overview sampling");

    prepared
        .environment
        .validate()
        .expect("complete environment");
    assert_eq!(prepared.environment.samples_per_axis, 2);
    assert_eq!(prepared.pages[0].geographic_height_centimeters, [4_200; 4]);
    assert_eq!(prepared.vegetation_pages[0].potential_biome_class, [2; 4]);
    let water = &prepared.water_pages[0];
    assert!(water.ocean_coverage_percent.contains(&0));
    assert!(water.ocean_coverage_percent.contains(&100));
    assert!(
        water
            .inland_coverage_percent
            .iter()
            .all(|coverage| *coverage == 0)
    );
    assert!(
        prepared.historical_land_use_pages[0]
            .coverage
            .iter()
            .all(|coverage| coverage.land_percent == 100)
    );
    assert_eq!(prepared.source_lock.id, "fixture-etopo");
    assert_eq!(prepared.water_source_lock.id, "fixture-natural-earth");
    assert_eq!(
        prepared.vegetation_source_lock.id,
        POTENTIAL_BIOME_RASTER_ID
    );
    assert_eq!(
        prepared.vegetation_classes_source_lock.id,
        POTENTIAL_BIOME_CLASSES_ID
    );
    assert_eq!(prepared.hyde_baseline_source_lock.id, HYDE_BASELINE_ID);
    assert_eq!(
        prepared.hyde_supplementary_source_lock.id,
        HYDE_SUPPLEMENTARY_ID
    );
    assert_eq!(prepared.hyde_readme_source_lock.id, HYDE_README_ID);
    assert_eq!(prepared.provenance.water, LayerProvenance::SourceDerived);
    assert_eq!(
        prepared.provenance.vegetation,
        LayerProvenance::SourceDerived
    );
    assert_eq!(
        prepared.provenance.historical_land_use,
        LayerProvenance::SourceDerived
    );

    let mut unavailable = fixture.sources.clone();
    unavailable.elevation.path = fixture.root.join("missing-etopo.tif");
    assert!(matches!(
        prepare_overview_from_verified_sources(
            request,
            2,
            2,
            &historical,
            &vegetation,
            unavailable,
        ),
        Err(GeodataError::Gdal(_))
    ));
}

struct OverviewFixture {
    root: PathBuf,
    sources: VerifiedOverviewSources,
}

impl OverviewFixture {
    fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let serial = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "aoe-overview-pipeline-{}-{serial}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("fixture directory");

        let elevation_path = root.join("etopo.tif");
        let vegetation_path = root.join("vegetation.tif");
        write_geotiff(&elevation_path, 42.0);
        write_geotiff(&vegetation_path, 2.0);

        let water_path = write_land_archive(&root);
        let vegetation_classes_path = root.join("potential-biome-classes.csv");
        fs::write(&vegetation_classes_path, compatible_legend()).expect("write legend");
        let hyde_baseline_path = root.join("hyde-baseline.zip");
        let hyde_supplementary_path = root.join("hyde-supplementary.zip");
        write_hyde_archive(&hyde_baseline_path, &HYDE_MEMBERS[..3], &["0", "0", "0"]);
        write_hyde_archive(&hyde_supplementary_path, &HYDE_MEMBERS[3..], &["1", "1"]);
        let hyde_readme_path = root.join("hyde-readme.txt");
        fs::write(&hyde_readme_path, "fixture source release notes").expect("write readme");

        let sources = VerifiedOverviewSources {
            elevation: VerifiedOverviewSource {
                lock: fixture_lock(
                    &elevation_path,
                    "fixture-etopo",
                    Provider::Noaa,
                    "www.ngdc.noaa.gov",
                ),
                path: elevation_path,
            },
            water: VerifiedOverviewSource {
                lock: fixture_lock(
                    &water_path,
                    "fixture-natural-earth",
                    Provider::NaturalEarth,
                    "naciscdn.org",
                ),
                path: water_path,
            },
            vegetation: VerifiedOverviewSource {
                lock: fixture_lock(
                    &vegetation_path,
                    POTENTIAL_BIOME_RASTER_ID,
                    Provider::Zenodo,
                    "zenodo.org",
                ),
                path: vegetation_path,
            },
            vegetation_classes: VerifiedOverviewSource {
                lock: fixture_lock(
                    &vegetation_classes_path,
                    POTENTIAL_BIOME_CLASSES_ID,
                    Provider::Zenodo,
                    "zenodo.org",
                ),
                path: vegetation_classes_path,
            },
            hyde_baseline: VerifiedOverviewSource {
                lock: fixture_lock(
                    &hyde_baseline_path,
                    HYDE_BASELINE_ID,
                    Provider::Dans,
                    "archaeology.datastations.nl",
                ),
                path: hyde_baseline_path,
            },
            hyde_supplementary: VerifiedOverviewSource {
                lock: fixture_lock(
                    &hyde_supplementary_path,
                    HYDE_SUPPLEMENTARY_ID,
                    Provider::Dans,
                    "archaeology.datastations.nl",
                ),
                path: hyde_supplementary_path,
            },
            hyde_readme: fixture_lock(
                &hyde_readme_path,
                HYDE_README_ID,
                Provider::Dans,
                "archaeology.datastations.nl",
            ),
        };
        Self { root, sources }
    }
}

impl Drop for OverviewFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn fixture_lock(path: &Path, id: &str, provider: Provider, host: &str) -> SourceLock {
    let bytes = fs::read(path).expect("fixture source bytes");
    SourceLock {
        id: id.to_owned(),
        provider,
        release: "offline pipeline fixture".to_owned(),
        url: format!("https://{host}/fixture"),
        sha256: digest_hex(&Sha256::digest(&bytes)),
        bytes: bytes.len() as u64,
        native_resolution: "fixture".to_owned(),
        crs: "EPSG:4326".to_owned(),
        vertical_datum: "fixture".to_owned(),
        license_reference: "test fixture".to_owned(),
    }
}

fn write_geotiff(path: &Path, value: f64) {
    let driver = DriverManager::get_driver_by_name("GTiff").expect("GTiff driver");
    let mut dataset = driver
        .create_with_band_type::<f64, _>(path, 256, 256, 1)
        .expect("fixture raster");
    dataset
        .set_geo_transform(&[1.0, 0.01, 0.0, 50.0, 0.0, -0.01])
        .expect("geotransform");
    dataset
        .set_spatial_ref(&SpatialRef::from_epsg(4326).expect("WGS84"))
        .expect("spatial reference");
    let mut values = Buffer::new((256, 256), vec![value; 256 * 256]);
    dataset
        .rasterband(1)
        .expect("band")
        .write((0, 0), (256, 256), &mut values)
        .expect("raster values");
    dataset.flush_cache().expect("flush raster");
}

fn write_land_archive(root: &Path) -> PathBuf {
    let geojson_path = root.join("land.geojson");
    let driver = DriverManager::get_driver_by_name("GeoJSON").expect("GeoJSON driver");
    let mut dataset = driver
        .create_vector_only(&geojson_path)
        .expect("fixture vector");
    let wgs84 = SpatialRef::from_epsg(4326).expect("WGS84");
    {
        let mut layer = dataset
            .create_layer(LayerOptions {
                name: "land",
                srs: Some(&wgs84),
                ty: gdal::vector::OGRwkbGeometryType::wkbPolygon,
                ..Default::default()
            })
            .expect("land layer");
        layer
            .create_feature(
                Geometry::from_wkt(
                    "POLYGON ((1.0 48.0, 2.35 48.0, 2.35 49.5, 1.0 49.5, 1.0 48.0))",
                )
                .expect("land polygon"),
            )
            .expect("land feature");
    }
    dataset.flush_cache().expect("flush vector");
    drop(dataset);

    let archive = root.join("natural-earth.zip");
    let mut zip = ZipWriter::new(File::create(&archive).expect("vector archive"));
    zip.start_file(
        "land.geojson",
        SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
    )
    .expect("vector member");
    zip.write_all(&fs::read(&geojson_path).expect("GeoJSON bytes"))
        .expect("vector bytes");
    zip.finish().expect("finish vector archive");
    archive
}

fn write_hyde_archive(path: &Path, members: &[&str], values: &[&str]) {
    let mut zip = ZipWriter::new(File::create(path).expect("HYDE archive"));
    for (member, value) in members.iter().zip(values) {
        zip.start_file(
            *member,
            SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
        )
        .expect("HYDE member");
        writeln!(
            zip,
            "ncols {HYDE_WIDTH}\nnrows {HYDE_HEIGHT}\nxllcorner -180\nyllcorner -90\ncellsize {HYDE_CELL_DEGREES}\nNODATA_value -9999"
        )
        .expect("HYDE header");
        let mut row = String::with_capacity(HYDE_WIDTH * 2);
        for _ in 0..HYDE_WIDTH {
            row.push_str(value);
            row.push(' ');
        }
        row.push('\n');
        for _ in 0..HYDE_HEIGHT {
            zip.write_all(row.as_bytes()).expect("HYDE grid row");
        }
    }
    zip.finish().expect("finish HYDE archive");
}

fn compatible_legend() -> &'static str {
    "\"\",\"Number\",\"New.global.consolidated.biome.scheme\"\n\
     tropical evergreen broadleaf forest\n\
     cool evergreen needleleaf forest\n\
     temperate deciduous broadleaf forest\n\
     tropical savanna\nsteppe\ndesert\ngraminoid and forb tundra\n"
}

#[path = "tests/field_axes.rs"]
mod field_axes_tests;
