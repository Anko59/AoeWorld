//! An actual all-water landscape, independent of optional start preferences.
use aoe_map::{
    ENVIRONMENT_PAGE_SAMPLES, ElevationPage, FieldPyramid, MapPackage, MapRequest,
    PreparedEnvironment, PyramidLevel, WaterPage, ordered_page_root, ordered_water_page_root,
};
use std::path::Path;

pub(super) fn ocean_package(directory: &Path) -> String {
    let elevation = ElevationPage {
        level: 0,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        geographic_height_centimeters: vec![0],
    };
    let water = WaterPage {
        level: 0,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        ocean_coverage_percent: vec![100],
        inland_coverage_percent: vec![0],
    };
    let field = |root| FieldPyramid {
        levels: vec![PyramidLevel {
            samples_per_axis: 1,
            ordered_page_root: root,
        }],
    };
    let environment = PreparedEnvironment {
        samples_per_axis: 1,
        geographic_millimeters_per_sample: 3_840_000,
        page_samples: ENVIRONMENT_PAGE_SAMPLES,
        elevation: field(ordered_page_root(std::slice::from_ref(&elevation)).unwrap()),
        water: Some(field(
            ordered_water_page_root(std::slice::from_ref(&water)).unwrap(),
        )),
        ..PreparedEnvironment::default()
    };
    let package = MapPackage::with_prepared_environment(
        1,
        MapRequest {
            requested_side_meters: 3840,
            ..MapRequest::default()
        },
        vec![],
        Default::default(),
        Default::default(),
        environment,
    )
    .unwrap();
    let hash = package.content_hash_hex();
    std::fs::write(
        directory.join(format!("{hash}.json")),
        serde_json::to_vec(&package).unwrap(),
    )
    .unwrap();
    for layer in ["elevation", "water"] {
        std::fs::create_dir_all(directory.join("pages").join(&hash).join(layer)).unwrap();
    }
    std::fs::write(
        directory
            .join("pages")
            .join(&hash)
            .join("elevation/0-0-0.json"),
        serde_json::to_vec(&elevation).unwrap(),
    )
    .unwrap();
    std::fs::write(
        directory.join("pages").join(&hash).join("water/0-0-0.json"),
        serde_json::to_vec(&water).unwrap(),
    )
    .unwrap();
    hash
}
