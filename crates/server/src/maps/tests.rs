use super::*;

#[test]
fn fallback_preview_is_bounded_and_does_not_require_activation() {
    let package = MapPackage::new(MAP_SCHEMA_VERSION, MapRequest::default(), Vec::new())
        .expect("fallback package");
    let preview = preview_package(package, None, &|| false).expect("fallback preview");
    assert_eq!(preview.samples_per_axis, PREVIEW_SAMPLES_PER_AXIS);
    assert_eq!(
        preview.cells.len(),
        usize::from(PREVIEW_SAMPLES_PER_AXIS).pow(2)
    );
    assert!(!preview.source_backed);
    assert!(preview.minimum_height_centimeters <= preview.maximum_height_centimeters);
}

#[test]
fn preview_coordinates_stay_inside_tiny_and_large_maps() {
    assert_eq!(preview_coordinate(1, 0).expect("coordinate"), 0);
    assert_eq!(
        preview_coordinate(500, PREVIEW_SAMPLES_PER_AXIS - 1).expect("coordinate"),
        484
    );
}
