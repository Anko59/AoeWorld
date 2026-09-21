use super::super::{PageResidency, elevation_page_root, persist_prepared};
use super::prepared;
use crate::page_residency::Registry;
use aoe_map::{
    ENVIRONMENT_PAGE_SAMPLES, ElevationPage, EnvironmentPageError, EnvironmentPageKey,
    EnvironmentPageProvider, EnvironmentalProvenance, FieldPyramid, MapPackage, MapRequest,
    PageLayer, PreparedEnvironment, ProjectionMetadata, PyramidLevel, ordered_page_root,
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
