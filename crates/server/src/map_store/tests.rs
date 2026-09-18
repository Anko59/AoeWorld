use super::{
    MapStoreError, elevation_page_root, load, load_elevation_pages, load_water_pages, persist,
    persist_prepared,
};
use aoe_map::{
    ENVIRONMENT_PAGE_SAMPLES, ElevationPage, EnvironmentalProvenance, FieldPyramid, MapPackage,
    MapRequest, PreparedEnvironment, ProjectionMetadata, PyramidLevel, WaterPage,
    ordered_page_root, ordered_water_page_root,
};
use std::fs;

fn prepared() -> (MapPackage, Vec<ElevationPage>, Vec<WaterPage>) {
    let elevation = [
        ElevationPage {
            level: 0,
            x: 0,
            y: 0,
            width: 2,
            height: 2,
            geographic_height_centimeters: vec![1, 2, 3, 4],
        },
        ElevationPage {
            level: 1,
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            geographic_height_centimeters: vec![3],
        },
    ];
    let water = [
        WaterPage {
            level: 0,
            x: 0,
            y: 0,
            width: 2,
            height: 2,
            ocean_coverage_percent: vec![0, 25, 75, 100],
        },
        WaterPage {
            level: 1,
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            ocean_coverage_percent: vec![50],
        },
    ];
    let field = |roots: [[u8; 32]; 2]| FieldPyramid {
        levels: vec![
            PyramidLevel {
                samples_per_axis: 2,
                ordered_page_root: roots[0],
            },
            PyramidLevel {
                samples_per_axis: 1,
                ordered_page_root: roots[1],
            },
        ],
    };
    let environment = PreparedEnvironment {
        samples_per_axis: 2,
        geographic_millimeters_per_sample: 1_000,
        page_samples: ENVIRONMENT_PAGE_SAMPLES,
        elevation: field([
            ordered_page_root(&[elevation[0].clone()]).expect("elevation root"),
            ordered_page_root(&[elevation[1].clone()]).expect("elevation root"),
        ]),
        water: Some(field([
            ordered_water_page_root(&[water[0].clone()]).expect("water root"),
            ordered_water_page_root(&[water[1].clone()]).expect("water root"),
        ])),
    };
    let package = MapPackage::with_prepared_environment(
        1,
        MapRequest::default(),
        Vec::new(),
        ProjectionMetadata::default(),
        EnvironmentalProvenance::default(),
        environment,
    )
    .expect("package");
    (package, elevation.into(), water.into())
}

#[test]
fn persisted_packages_reload_with_their_canonical_identity() {
    let directory = tempfile::tempdir().expect("package directory");
    let package = MapPackage::new(1, MapRequest::default(), Vec::new()).expect("package");
    persist(Some(directory.path()), &package).expect("persist");
    assert_eq!(
        load(Some(directory.path()))
            .expect("load")
            .get(&package.content_hash_hex()),
        Some(&package)
    );
}

#[test]
fn noncanonical_file_names_are_rejected_on_load() {
    let directory = tempfile::tempdir().expect("package directory");
    let package = MapPackage::new(1, MapRequest::default(), Vec::new()).expect("package");
    fs::write(
        directory.path().join("wrong.json"),
        serde_json::to_vec(&package).expect("JSON"),
    )
    .expect("fixture");
    assert!(matches!(
        load(Some(directory.path())),
        Err(MapStoreError::InvalidPackage { .. })
    ));
}

#[test]
fn prepared_elevation_and_water_pages_must_persist_with_their_package() {
    let directory = tempfile::tempdir().expect("package directory");
    let (package, elevation, water) = prepared();
    persist_prepared(Some(directory.path()), &package, &elevation, &water).expect("persist");
    assert_eq!(load(Some(directory.path())).expect("load").len(), 1);
    assert_eq!(
        load_elevation_pages(Some(directory.path()), &package).expect("elevation pages"),
        elevation
    );
    assert_eq!(
        load_water_pages(Some(directory.path()), &package).expect("water pages"),
        water
    );
    fs::remove_file(elevation_page_root(directory.path(), &package).join("0-0-0.json"))
        .expect("remove page");
    assert!(matches!(
        load(Some(directory.path())),
        Err(MapStoreError::Io(_))
    ));
}
