use super::*;
use aoe_map::{
    EnvironmentalProvenance, FieldPyramid, GeographicWaterPatch, HistoricalLandUsePage,
    HydrologyEvidenceIndex, HydrologyEvidenceMethod, HydrologyEvidencePage, HydrologyKind,
    HydrologyWaterPolicy, LayerProvenance, ModernLandCoverPage, PotentialBiomePage,
    PreparedEnvironment, ProjectionMetadata, PyramidLevel, WaterCorrectionDocument,
    WaterCorrectionOperation, WaterCorrectionVertex, WaterPage, ordered_hydrology_page_root,
    ordered_land_use_page_root, ordered_modern_land_cover_page_root,
};
use gdal::{DriverManager, raster::Buffer, spatial_ref::SpatialRef};
use std::{
    collections::BTreeSet,
    fs,
    sync::atomic::{AtomicU64, Ordering},
};

#[test]
fn acquired_inputs_build_and_verify_a_corrected_package_offline() {
    let root = temporary_directory();
    let output = root.join("published");
    fs::create_dir_all(&root).expect("fixture root");
    let dem_path = root.join("dem.tif");
    write_dem(&dem_path);

    let request = MapRequest::default();
    let estimate = request.estimate().expect("request estimate");
    let bounds = geographic_bounds(request, estimate.effective_side_meters).expect("bounds");
    let overview = overview_fixture();
    let hydrology = hydrology_fixture();
    let coverage = TileCoverage {
        tiles: vec![Tile {
            latitude: 48,
            longitude: 2,
            path: dem_path,
            lock: dem_source_lock(),
        }],
        absent_tiles: BTreeSet::new(),
    };
    let correction = lake_correction(request);
    let package = entry::assemble_acquired_package(
        request,
        estimate.effective_side_meters,
        2,
        &output,
        &root,
        correction.clone(),
        entry::AcquiredDetailedInputs {
            bounds,
            overview,
            hydrology,
            coverage,
        },
    )
    .expect("offline detailed assembly");

    assert_eq!(
        package.generation_recipe_version,
        aoe_map::GENERATION_RECIPE_VERSION
    );
    let model = package
        .environment
        .hydrology_evidence
        .as_ref()
        .and_then(|index| index.water_model.as_ref())
        .expect("modeled water index");
    assert_eq!(model.model_version, aoe_map::HYDROLOGY_WATER_MODEL_VERSION);
    assert_eq!(model.correction_document, correction);
    assert!(
        package
            .source_locks
            .iter()
            .any(|source| { source.id == "fixture-dem" && source.provider == "Copernicus" })
    );
    assert!(
        package
            .source_locks
            .iter()
            .any(|source| source.id == "fixture-hydrology")
    );

    let hash = package.content_hash_hex();
    crate::GeneratedMap::verify_directory(&output, &hash).expect("published pages verify");
    let generated = crate::GeneratedMap::read_directory(&output, &hash).expect("read package");
    assert_eq!(generated.package, package);
    assert_eq!(
        generated.elevation_pages[0].geographic_height_centimeters,
        vec![12_345; 4]
    );
    assert_eq!(generated.water_pages[0].ocean_coverage_percent, vec![0; 4]);
    assert_eq!(
        generated.water_pages[0].inland_coverage_percent,
        vec![100; 4]
    );
    let modeled = generated.hydrology_evidence_pages[0]
        .water_model
        .as_ref()
        .expect("published hydrology model");
    assert_eq!(modeled.kind, vec![HydrologyKind::Lake as u8; 4]);
    assert_eq!(modeled.surface_level_centimeters, vec![Some(12_345); 4]);
    assert!(
        modeled
            .provenance
            .iter()
            .all(|value| { *value == aoe_map::WaterModelProvenance::GeographicCorrection as u8 })
    );

    let page_path = output.join(format!("pages/{hash}/elevation/0-0-0.json"));
    fs::write(page_path, b"{}").expect("corrupt page fixture");
    assert!(crate::GeneratedMap::verify_directory(&output, &hash).is_err());
    fs::remove_dir_all(root).expect("remove fixture root");
}

fn overview_fixture() -> crate::PreparedOverview {
    let water_pages = four_pages(|x, y| WaterPage {
        level: 0,
        x,
        y,
        width: 64,
        height: 64,
        ocean_coverage_percent: vec![100; 64 * 64],
        inland_coverage_percent: vec![0; 64 * 64],
    });
    let vegetation_pages = four_pages(|x, y| PotentialBiomePage {
        level: 0,
        x,
        y,
        width: 64,
        height: 64,
        potential_biome_class: vec![7; 64 * 64],
    });
    let (historical_land_use_pages, historical_levels) = historical_overview_fixture();
    crate::PreparedOverview {
        source_lock: map_source_lock("overview"),
        water_source_lock: map_source_lock("overview-water"),
        vegetation_source_lock: map_source_lock("overview-vegetation"),
        vegetation_classes_source_lock: map_source_lock("overview-classes"),
        hyde_baseline_source_lock: map_source_lock("hyde-baseline"),
        hyde_supplementary_source_lock: map_source_lock("hyde-supplementary"),
        hyde_readme_source_lock: map_source_lock("hyde-readme"),
        projection: ProjectionMetadata::default(),
        provenance: EnvironmentalProvenance {
            elevation: LayerProvenance::SourceDerived,
            water: LayerProvenance::SourceDerived,
            vegetation: LayerProvenance::SourceDerived,
            historical_land_use: LayerProvenance::SourceDerived,
        },
        environment: PreparedEnvironment {
            samples_per_axis: 128,
            geographic_millimeters_per_sample: 1_000,
            page_samples: aoe_map::ENVIRONMENT_PAGE_SAMPLES,
            elevation: FieldPyramid {
                levels: historical_levels
                    .iter()
                    .map(|level| PyramidLevel {
                        samples_per_axis: level.samples_per_axis,
                        ordered_page_root: [1; 32],
                    })
                    .collect(),
            },
            historical_land_use: Some(FieldPyramid {
                levels: historical_levels,
            }),
            ..PreparedEnvironment::default()
        },
        pages: Vec::new(),
        water_pages,
        vegetation_pages,
        historical_land_use_pages,
    }
}

fn historical_overview_fixture() -> (Vec<HistoricalLandUsePage>, Vec<PyramidLevel>) {
    let mut pages = Vec::new();
    let mut levels = Vec::new();
    let mut axis = 128_u16;
    let mut level = 0_u8;
    loop {
        let level_pages = (0..axis.div_ceil(64))
            .flat_map(|y| (0..axis.div_ceil(64)).map(move |x| (x, y)))
            .map(|(x, y)| {
                let page_x = x * 64;
                let page_y = y * 64;
                let width = (axis - page_x).min(64) as u8;
                let height = (axis - page_y).min(64) as u8;
                let count = usize::from(width) * usize::from(height);
                HistoricalLandUsePage {
                    level,
                    x,
                    y,
                    width,
                    height,
                    crop_percent: vec![1; count],
                    grazing_percent: vec![2; count],
                    population_pressure_per_square_kilometer: vec![3; count],
                    coverage: Vec::new(),
                }
            })
            .collect::<Vec<_>>();
        levels.push(PyramidLevel {
            samples_per_axis: axis,
            ordered_page_root: ordered_land_use_page_root(&level_pages).expect("historical root"),
        });
        pages.extend(level_pages);
        if axis == 1 {
            break;
        }
        axis = axis.div_ceil(2);
        level += 1;
    }
    (pages, levels)
}

fn four_pages<T>(mut build: impl FnMut(u16, u16) -> T) -> Vec<T> {
    [(0, 0), (1, 0), (0, 1), (1, 1)]
        .into_iter()
        .map(|(x, y)| build(x, y))
        .collect()
}

fn hydrology_fixture() -> crate::PreparedHydrology {
    let evidence = vec![HydrologyEvidencePage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        kind: vec![HydrologyKind::NoEvidence as u8; 4],
        method: vec![HydrologyEvidenceMethod::None as u8; 4],
        water_model: None,
    }];
    let land_cover = vec![ModernLandCoverPage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        worldcover_class: vec![40; 4],
    }];
    crate::PreparedHydrology {
        samples_per_axis: 2,
        evidence_index: HydrologyEvidenceIndex {
            samples_per_axis: 2,
            page_samples: aoe_map::ENVIRONMENT_PAGE_SAMPLES,
            world_cover_year: aoe_map::WORLD_COVER_OBSERVATION_YEAR,
            policy: HydrologyWaterPolicy::HistoricalOverviewWithMappedNaturalWaterV1,
            hydrology_page_root: ordered_hydrology_page_root(&evidence).expect("evidence root"),
            modern_land_cover_page_root: ordered_modern_land_cover_page_root(&land_cover)
                .expect("land-cover root"),
            water_model: None,
        },
        source_locks: vec![map_source_lock("fixture-hydrology")],
        hydrology_pages: evidence,
        modern_land_cover_pages: land_cover,
        river_topology: None,
    }
}

fn lake_correction(request: MapRequest) -> WaterCorrectionDocument {
    WaterCorrectionDocument::new(
        request,
        2,
        vec![GeographicWaterPatch {
            id: "fixture-lake".to_owned(),
            precedence: 1,
            applies_from_year_ce: 1,
            applies_through_year_ce: 600,
            source_citation: "offline fixture correction".to_owned(),
            operation: WaterCorrectionOperation::SetNaturalLake,
            polygon: vec![
                vertex(2.2, 48.6),
                vertex(2.6, 48.6),
                vertex(2.6, 49.1),
                vertex(2.2, 49.1),
            ],
        }],
    )
    .expect("correction document")
}

fn vertex(longitude: f64, latitude: f64) -> WaterCorrectionVertex {
    WaterCorrectionVertex {
        longitude_e7: (longitude * 10_000_000.0) as i32,
        latitude_e7: (latitude * 10_000_000.0) as i32,
    }
}

fn dem_source_lock() -> crate::SourceLock {
    crate::SourceLock {
        id: "fixture-dem".to_owned(),
        provider: crate::Provider::Copernicus,
        release: "fixture".to_owned(),
        url: "https://copernicus-dem-90m.s3.amazonaws.com/fixture.tif".to_owned(),
        sha256: "a".repeat(64),
        bytes: 1,
        native_resolution: "3 arc-seconds".to_owned(),
        crs: "EPSG:4326".to_owned(),
        vertical_datum: "EGM2008 orthometric".to_owned(),
        license_reference: "fixture".to_owned(),
    }
}

fn map_source_lock(id: &str) -> aoe_map::SourceLock {
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

fn write_dem(path: &std::path::Path) {
    let driver = DriverManager::get_driver_by_name("GTiff").expect("GTiff driver");
    let mut dataset = driver
        .create_with_band_type::<f64, _>(path, 32, 32, 1)
        .expect("tiny DEM");
    dataset
        .set_geo_transform(&[2.2, 0.01, 0.0, 49.1, 0.0, -0.01])
        .expect("geotransform");
    dataset
        .set_spatial_ref(&SpatialRef::from_epsg(4326).expect("WGS84"))
        .expect("DEM CRS");
    let mut band = dataset.rasterband(1).expect("DEM band");
    let mut pixels = Buffer::new((32, 32), vec![123.45; 32 * 32]);
    band.write((0, 0), (32, 32), &mut pixels)
        .expect("DEM values");
    dataset.flush_cache().expect("flush DEM");
}

fn temporary_directory() -> std::path::PathBuf {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join(format!(
        "aoe-copernicus-offline-pipeline-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ))
}
