use super::{MapStoreError, elevation_page_root, load, persist, persist_prepared};
use aoe_map::{
    ENVIRONMENT_PAGE_SAMPLES, ElevationPage, EnvironmentalProvenance, FieldPyramid,
    HistoricalLandUsePage, MapPackage, MapRequest, PotentialBiomePage, PreparedEnvironment,
    ProjectionMetadata, PyramidLevel, WaterPage, ordered_biome_page_root,
    ordered_land_use_page_root, ordered_page_root, ordered_water_page_root,
};
use std::fs;

#[cfg(test)]
mod legacy;
#[cfg(test)]
mod residency;

use legacy::{load_elevation_pages, load_land_use_pages, load_vegetation_pages, load_water_pages};

pub(super) fn prepared() -> (
    MapPackage,
    Vec<ElevationPage>,
    Vec<WaterPage>,
    Vec<PotentialBiomePage>,
    Vec<HistoricalLandUsePage>,
) {
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
            inland_coverage_percent: vec![100, 0, 0, 0],
        },
        WaterPage {
            level: 1,
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            ocean_coverage_percent: vec![50],
            inland_coverage_percent: vec![0],
        },
    ];
    let vegetation = [
        PotentialBiomePage {
            level: 0,
            x: 0,
            y: 0,
            width: 2,
            height: 2,
            potential_biome_class: vec![13, 15, 18, 27],
        },
        PotentialBiomePage {
            level: 1,
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            potential_biome_class: vec![13],
        },
    ];
    let land_use = [
        HistoricalLandUsePage {
            level: 0,
            x: 0,
            y: 0,
            width: 2,
            height: 2,
            crop_percent: vec![20, 0, 10, 0],
            grazing_percent: vec![30, 15, 0, 0],
            population_pressure_per_square_kilometer: vec![5, 2, 1, 0],
            coverage: Vec::new(),
        },
        HistoricalLandUsePage {
            level: 1,
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            crop_percent: vec![8],
            grazing_percent: vec![11],
            population_pressure_per_square_kilometer: vec![2],
            coverage: Vec::new(),
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
        vegetation: Some(field([
            ordered_biome_page_root(&[vegetation[0].clone()]).expect("vegetation root"),
            ordered_biome_page_root(&[vegetation[1].clone()]).expect("vegetation root"),
        ])),
        historical_land_use: Some(field([
            ordered_land_use_page_root(&[land_use[0].clone()]).expect("land-use root"),
            ordered_land_use_page_root(&[land_use[1].clone()]).expect("land-use root"),
        ])),

        hydrology_evidence: None,
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
    (
        package,
        elevation.into(),
        water.into(),
        vegetation.into(),
        land_use.into(),
    )
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
    assert_eq!(load(Some(directory.path())).expect("load").len(), 1);
    assert_eq!(
        load_elevation_pages(Some(directory.path()), &package).expect("elevation pages"),
        elevation
    );
    assert_eq!(
        load_water_pages(Some(directory.path()), &package).expect("water pages"),
        water
    );
    assert_eq!(
        load_vegetation_pages(Some(directory.path()), &package).expect("vegetation pages"),
        vegetation
    );
    assert_eq!(
        load_land_use_pages(Some(directory.path()), &package).expect("land-use pages"),
        land_use
    );
    fs::remove_file(elevation_page_root(directory.path(), &package).join("0-0-0.json"))
        .expect("remove page");
    assert!(matches!(
        load(Some(directory.path())),
        Err(MapStoreError::Io(_))
    ));
}

#[test]
fn prepared_publication_rejects_wrong_coordinates_and_dimensions() {
    for wrong_coordinates in [true, false] {
        let directory = tempfile::tempdir().expect("directory");
        let (original, mut elevation, water, vegetation, land_use) = prepared();
        if wrong_coordinates {
            elevation[0].x = 1;
        } else {
            elevation[0].width = 1;
            elevation[0].height = 4;
        }
        let mut environment = original.environment;
        environment.elevation.levels[0].ordered_page_root =
            ordered_page_root(&elevation[..1]).expect("changed root");
        let package = MapPackage::with_prepared_environment(
            original.generator_version,
            original.request,
            original.source_locks,
            original.projection,
            original.provenance,
            environment,
        )
        .expect("changed package");
        let error = persist_prepared(
            Some(directory.path()),
            &package,
            &elevation,
            &water,
            &vegetation,
            &land_use,
        )
        .expect_err("reject invalid geometry before publishing");
        assert!(
            error.to_string().contains("coordinates or dimensions")
                || error.to_string().contains("No such file")
        );
        assert!(
            !directory
                .path()
                .join(format!("{}.json", package.content_hash_hex()))
                .exists()
        );
    }
}

#[cfg(unix)]
#[test]
fn streaming_verifier_rejects_redirected_layer_directories() {
    let directory = tempfile::tempdir().expect("directory");
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
    super::verify_stored(directory.path(), &package).expect("valid package");
    let original = elevation_page_root(directory.path(), &package);
    let moved = directory.path().join("redirected-elevation");
    fs::rename(&original, &moved).expect("move layer");
    std::os::unix::fs::symlink(&moved, &original).expect("symlink");
    assert!(super::verify_stored(directory.path(), &package).is_err());
}

#[test]
fn legacy_numbered_non_elevation_pages_remain_readable() {
    let directory = tempfile::tempdir().expect("directory");
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
    for layer in ["water", "vegetation", "historical-land-use"] {
        let root = directory
            .path()
            .join("pages")
            .join(package.content_hash_hex())
            .join(layer);
        for level in 0..2 {
            fs::rename(
                root.join(format!("{level}-0-0.json")),
                root.join(format!("{level}.json")),
            )
            .expect("legacy filename");
        }
    }
    assert_eq!(load(Some(directory.path())).expect("reload").len(), 1);
    assert_eq!(
        load_water_pages(Some(directory.path()), &package).expect("water"),
        water
    );
}

#[test]
fn existing_page_contents_are_never_rewritten() {
    let directory = tempfile::tempdir().expect("directory");
    let path = directory.path().join("page.json");
    super::write_json(&path, &vec![1], 128).expect("write");
    super::write_json(&path, &vec![1], 128).expect("idempotent");
    assert!(super::write_json(&path, &vec![2], 128).is_err());
    assert_eq!(fs::read(path).expect("read"), b"[1]");
}

#[test]
fn repeated_source_acquisition_keeps_the_first_manifest_for_the_same_identity() {
    let directory = tempfile::tempdir().expect("directory");
    let source = aoe_map::SourceLock {
        id: "fixture".to_owned(),
        provider: "fixture".to_owned(),
        release: "1".to_owned(),
        url: "https://example.invalid/fixture".to_owned(),
        sha256: [7; 32],
        acquired_at: "first".to_owned(),
        native_resolution: "30m".to_owned(),
        crs: "EPSG:4326".to_owned(),
        vertical_datum: "EGM2008".to_owned(),
        license: "fixture".to_owned(),
        preprocessing_version: "1".to_owned(),
    };
    let first = MapPackage::new(1, MapRequest::default(), vec![source]).expect("package");
    persist(Some(directory.path()), &first).expect("persist");
    let mut later = first.clone();
    later.source_locks[0].acquired_at = "later".to_owned();
    later.validate().expect("same canonical identity");
    persist(Some(directory.path()), &later).expect("idempotent persist");
    assert_eq!(
        super::verify_stored(directory.path(), &later).expect("verified"),
        first
    );
}

#[test]
fn concurrent_publishers_use_distinct_temporary_files_and_one_complete_manifest() {
    let directory = tempfile::tempdir().expect("directory");
    let (package, elevation, water, vegetation, land_use) = prepared();
    let gate = std::sync::Barrier::new(8);
    std::thread::scope(|scope| {
        let threads = (0..8)
            .map(|_| {
                scope.spawn(|| {
                    gate.wait();
                    persist_prepared(
                        Some(directory.path()),
                        &package,
                        &elevation,
                        &water,
                        &vegetation,
                        &land_use,
                    )
                })
            })
            .collect::<Vec<_>>();
        for thread in threads {
            thread.join().expect("publisher").expect("complete");
        }
    });
    assert_eq!(
        super::verify_stored(directory.path(), &package).expect("verified"),
        package
    );
}

#[test]
fn bounded_file_reader_rejects_bytes_beyond_the_limit() {
    let directory = tempfile::tempdir().expect("directory");
    let path = directory.path().join("oversized.json");
    fs::write(&path, b"1234").expect("fixture");
    let error = super::read_bounded_file(&path, 3, "fixture").expect_err("oversized file");
    assert!(error.to_string().contains("bounded"));
}

#[cfg(unix)]
#[test]
fn storage_root_symlinks_are_rejected() {
    let parent = tempfile::tempdir().expect("parent");
    let target = tempfile::tempdir().expect("target");
    let link = parent.path().join("maps");
    std::os::unix::fs::symlink(target.path(), &link).expect("symlink");
    let error = load(Some(&link)).expect_err("symlink root");
    assert!(error.to_string().contains("symlink"));
}

#[cfg(unix)]
#[test]
fn prepared_publication_rejects_redirected_page_root_before_writing() {
    let directory = tempfile::tempdir().expect("directory");
    let external = tempfile::tempdir().expect("external");
    let (package, elevation, water, vegetation, land_use) = prepared();
    let pages = directory.path().join("pages");
    fs::create_dir(&pages).expect("pages");
    std::os::unix::fs::symlink(external.path(), pages.join(package.content_hash_hex()))
        .expect("page-root symlink");
    let error = persist_prepared(
        Some(directory.path()),
        &package,
        &elevation,
        &water,
        &vegetation,
        &land_use,
    )
    .expect_err("redirected page root");
    assert!(error.to_string().contains("symlink"));
    assert!(
        external
            .path()
            .read_dir()
            .expect("external entries")
            .next()
            .is_none()
    );
    assert!(
        !directory
            .path()
            .join(format!("{}.json", package.content_hash_hex()))
            .exists()
    );
}
