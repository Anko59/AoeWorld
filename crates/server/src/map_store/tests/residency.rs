use super::super::{PageResidency, elevation_page_root, persist_prepared};
use super::prepared;
use crate::page_residency::Registry;
use aoe_map::{
    ENVIRONMENT_PAGE_SAMPLES, ElevationPage, EnvironmentPageError, EnvironmentPageKey,
    EnvironmentPageProvider, EnvironmentalProvenance, FieldPyramid, HydrologyEvidenceIndex,
    HydrologyEvidenceMethod, HydrologyEvidencePage, HydrologyKind, HydrologyWaterPolicy,
    MapPackage, MapRequest, ModernLandCoverPage, PageLayer, PreparedEnvironment,
    ProjectionMetadata, PyramidLevel, WORLD_COVER_OBSERVATION_YEAR, WaterKind,
    ordered_hydrology_page_root, ordered_modern_land_cover_page_root, ordered_page_root,
    ordered_water_page_root,
};
use std::fs;

#[test]
fn lazy_residency_matches_dense_chunks_and_fails_closed_for_page_errors() {
    let directory = tempfile::tempdir().expect("package directory");
    let (package, elevation, water, vegetation, land_use) = prepared();
    persist_prepared(
        Some(directory.path()),
        &package,
        &elevation,
        &water,
        &vegetation,
        &land_use,
    )
    .expect("persist");
    let residency = PageResidency::open(directory.path(), &package, &|| false).expect("residency");
    assert_eq!(residency.indexed_pages(), 8);
    assert_eq!(residency.resident_pages(), 0);

    let dense = package
        .generator_with_environment(
            elevation.clone(),
            water.clone(),
            vegetation.clone(),
            land_use.clone(),
        )
        .expect("dense generator")
        .chunk(0, 0)
        .expect("dense chunk");
    let lazy = package
        .generator_with_page_provider(residency.clone())
        .expect("lazy generator")
        .chunk(0, 0)
        .expect("lazy chunk");
    assert_eq!(lazy, dense);
    assert!(lazy.tiles.iter().all(|tile| {
        tile.hydrology_observation.is_none() && tile.modern_land_cover_class.is_none()
    }));
    assert!(residency.resident_pages() <= super::super::residency::MAX_RESIDENT_ENVIRONMENT_PAGES);

    let elevation_key = EnvironmentPageKey {
        layer: PageLayer::Elevation,
        level: 1,
        x: 0,
        y: 0,
    };
    let page_root = elevation_page_root(directory.path(), &package);
    fs::remove_file(page_root.join("1-0-0.json")).expect("remove page");
    // The startup index was valid before the source disappeared. The missing
    // page must fail when its uncached payload is requested.
    let error = residency
        .page(elevation_key, &|| false)
        .expect_err("missing page must fail");
    assert_eq!(error, EnvironmentPageError::Missing);
    assert_eq!(
        residency
            .page(elevation_key, &|| true)
            .expect_err("cancelled page must fail"),
        EnvironmentPageError::Cancelled
    );
}

#[test]
fn lazy_residency_rejects_corruption_after_startup_without_poisoning_other_pages() {
    let directory = tempfile::tempdir().expect("package directory");
    let (package, elevation, water, vegetation, land_use) = prepared();
    persist_prepared(
        Some(directory.path()),
        &package,
        &elevation,
        &water,
        &vegetation,
        &land_use,
    )
    .expect("persist");
    let residency = PageResidency::open(directory.path(), &package, &|| false).expect("residency");
    let elevation_key = EnvironmentPageKey {
        layer: PageLayer::Elevation,
        level: 0,
        x: 0,
        y: 0,
    };
    let water_key = EnvironmentPageKey {
        layer: PageLayer::Water,
        level: 0,
        x: 0,
        y: 0,
    };
    fs::write(
        elevation_page_root(directory.path(), &package).join("0-0-0.json"),
        b"{}",
    )
    .expect("corrupt page");
    assert_eq!(
        residency
            .page(elevation_key, &|| false)
            .expect_err("corrupt page must fail"),
        EnvironmentPageError::Corrupt
    );
    let generator = package
        .generator_with_page_provider(residency.clone())
        .expect("generator");
    assert_eq!(
        generator.chunk(0, 0).expect_err("corrupt corner must fail"),
        EnvironmentPageError::Corrupt
    );
    assert_eq!(
        generator
            .chunk_with_cancel(0, 0, &|| true)
            .expect_err("cancelled chunk must fail"),
        EnvironmentPageError::Cancelled
    );
    assert!(residency.page(water_key, &|| false).is_ok());
}

#[test]
fn provider_registry_bounds_multi_package_residency() {
    let directory = tempfile::tempdir().expect("package directory");
    let (package, elevation, water, vegetation, land_use) = prepared();
    persist_prepared(
        Some(directory.path()),
        &package,
        &elevation,
        &water,
        &vegetation,
        &land_use,
    )
    .expect("persist");
    let provider = PageResidency::open(directory.path(), &package, &|| false).expect("residency");
    let mut registry = Registry::default();
    registry.insert("a".to_owned(), provider.clone());
    registry.insert("b".to_owned(), provider.clone());
    assert_eq!(registry.len(), 2);
    assert!(registry.get("a").is_some());
    registry.insert("c".to_owned(), provider);
    assert!(registry.get("a").is_some());
    assert!(registry.get("b").is_none());
    assert!(registry.get("c").is_some());
}

#[test]
fn elevation_residency_reloads_after_eviction_without_changing_chunks() {
    let directory = tempfile::tempdir().expect("package directory");
    let (package, pages) = large_elevation_package();
    persist_prepared(Some(directory.path()), &package, &pages, &[], &[], &[]).expect("persist");
    let residency = PageResidency::open(directory.path(), &package, &|| false).expect("residency");
    let generator = package
        .generator_with_page_provider(residency.clone())
        .expect("generator");
    let first = generator.chunk(0, 0).expect("first chunk");
    let distant = generator.chunk(15, 15).expect("distant chunk");
    for page in &pages {
        residency
            .page(
                EnvironmentPageKey {
                    layer: PageLayer::Elevation,
                    level: page.level,
                    x: page.x,
                    y: page.y,
                },
                &|| false,
            )
            .expect("page reload");
    }
    assert!(residency.resident_pages() <= super::super::residency::MAX_RESIDENT_ENVIRONMENT_PAGES);
    assert_eq!(generator.chunk(0, 0).expect("reloaded first chunk"), first);
    assert_eq!(
        generator.chunk(15, 15).expect("reloaded distant chunk"),
        distant
    );
}

fn large_elevation_package() -> (MapPackage, Vec<ElevationPage>) {
    let mut samples = 1_024_u16;
    let mut levels = Vec::new();
    let mut pages = Vec::new();
    let mut level = 0_u8;
    loop {
        let count = samples.div_ceil(u16::from(ENVIRONMENT_PAGE_SAMPLES));
        let mut level_pages = Vec::new();
        for y in 0..count {
            for x in 0..count {
                let width = (samples - x * u16::from(ENVIRONMENT_PAGE_SAMPLES))
                    .min(u16::from(ENVIRONMENT_PAGE_SAMPLES)) as u8;
                let height = (samples - y * u16::from(ENVIRONMENT_PAGE_SAMPLES))
                    .min(u16::from(ENVIRONMENT_PAGE_SAMPLES)) as u8;
                level_pages.push(ElevationPage {
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
            ordered_page_root: ordered_page_root(&level_pages).expect("level root"),
        });
        pages.extend(level_pages);
        if samples == 1 {
            break;
        }
        samples = samples.div_ceil(2);
        level = level.saturating_add(1);
    }
    let environment = PreparedEnvironment {
        samples_per_axis: 1_024,
        geographic_millimeters_per_sample: 1_000,
        page_samples: ENVIRONMENT_PAGE_SAMPLES,
        elevation: FieldPyramid { levels },
        ..PreparedEnvironment::default()
    };
    (
        MapPackage::with_prepared_environment(
            1,
            MapRequest::default(),
            Vec::new(),
            ProjectionMetadata::default(),
            EnvironmentalProvenance::default(),
            environment,
        )
        .expect("package"),
        pages,
    )
}

#[test]
fn typed_modern_evidence_is_sampled_without_turning_modern_water_into_history() {
    let directory = tempfile::tempdir().expect("package directory");
    let (base, elevation, mut water, vegetation, land_use) = prepared();
    water[0].ocean_coverage_percent.fill(0);
    let hydrology = vec![HydrologyEvidencePage {
        level: 0,
        x: 0,
        y: 0,
        width: 3,
        height: 3,
        kind: vec![
            HydrologyKind::River as u8,
            HydrologyKind::Land as u8,
            HydrologyKind::RegulatedLake as u8,
            HydrologyKind::Land as u8,
            HydrologyKind::Land as u8,
            HydrologyKind::Land as u8,
            HydrologyKind::Land as u8,
            HydrologyKind::NoEvidence as u8,
            HydrologyKind::River as u8,
        ],
        method: vec![
            HydrologyEvidenceMethod::HydroRiversBufferedCorridor as u8,
            HydrologyEvidenceMethod::WorldCoverClass as u8,
            HydrologyEvidenceMethod::HydroLakesExtent as u8,
            HydrologyEvidenceMethod::WorldCoverClass as u8,
            HydrologyEvidenceMethod::WorldCoverClass as u8,
            HydrologyEvidenceMethod::WorldCoverClass as u8,
            HydrologyEvidenceMethod::WorldCoverClass as u8,
            HydrologyEvidenceMethod::None as u8,
            HydrologyEvidenceMethod::HydroRiversBufferedCorridor as u8,
        ],
        water_model: None,
    }];
    let cover = vec![ModernLandCoverPage {
        level: 0,
        x: 0,
        y: 0,
        width: 3,
        height: 3,
        worldcover_class: vec![10, 40, 40, 40, 40, 40, 40, 0, 0],
    }];
    let mut environment = base.environment.clone();
    environment.water.as_mut().expect("water field").levels[0].ordered_page_root =
        ordered_water_page_root(&water[..1]).expect("water root");
    environment.hydrology_evidence = Some(HydrologyEvidenceIndex {
        samples_per_axis: 3,
        page_samples: ENVIRONMENT_PAGE_SAMPLES,
        world_cover_year: WORLD_COVER_OBSERVATION_YEAR,
        policy: HydrologyWaterPolicy::HistoricalOverviewWithMappedNaturalWaterV1,
        hydrology_page_root: ordered_hydrology_page_root(&hydrology).expect("hydrology root"),
        modern_land_cover_page_root: ordered_modern_land_cover_page_root(&cover)
            .expect("cover root"),
        water_model: None,
    });
    let package = MapPackage::with_prepared_environment(
        base.generator_version,
        base.request,
        base.source_locks.clone(),
        base.projection.clone(),
        base.provenance.clone(),
        environment,
    )
    .expect("typed package");
    let root = directory
        .path()
        .join("pages")
        .join(package.content_hash_hex());
    fs::create_dir_all(root.join("hydrology-evidence")).expect("hydrology directory");
    fs::create_dir_all(root.join("modern-land-cover")).expect("cover directory");
    fs::write(
        root.join("hydrology-evidence/0-0-0.json"),
        serde_json::to_vec(&hydrology[0]).expect("hydrology json"),
    )
    .expect("write hydrology");
    fs::write(
        root.join("modern-land-cover/0-0-0.json"),
        serde_json::to_vec(&cover[0]).expect("cover json"),
    )
    .expect("write cover");
    persist_prepared(
        Some(directory.path()),
        &package,
        &elevation,
        &water,
        &vegetation,
        &land_use,
    )
    .expect("publish package");

    let residency = PageResidency::open(directory.path(), &package, &|| false)
        .expect("verified typed residency");
    let generator = package
        .generator_with_page_provider(residency)
        .expect("typed provider generator");
    let river = generator
        .tile_at_with_cancel(aoe_core::TileCoord::new(0, 0), &|| false)
        .expect("river query")
        .expect("in bounds");
    assert_eq!(river.water, WaterKind::River);
    assert_eq!(
        river.hydrology_observation.expect("observation").kind,
        HydrologyKind::River
    );
    assert_eq!(river.modern_land_cover_class, Some(10));

    let regulated_dry = generator
        .tile_at_with_cancel(aoe_core::TileCoord::new(499, 0), &|| false)
        .expect("regulated extent query")
        .expect("in bounds");
    assert_eq!(regulated_dry.water, WaterKind::None);
    assert_eq!(regulated_dry.modern_land_cover_class, Some(40));

    let river_on_dry_overview = generator
        .tile_at_with_cancel(aoe_core::TileCoord::new(499, 499), &|| false)
        .expect("dry river corridor query")
        .expect("in bounds");
    assert_eq!(river_on_dry_overview.water, WaterKind::None);
    assert_eq!(river_on_dry_overview.modern_land_cover_class, Some(0));
}

mod evidence;
