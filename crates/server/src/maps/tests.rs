use super::*;
use axum::http::HeaderValue;

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

#[test]
fn controller_header_parses_only_complete_hex_resume_tokens() {
    let mut headers = HeaderMap::new();
    assert_eq!(controller_token(&headers), None);

    headers.insert(
        CONTROLLER_TOKEN_HEADER,
        HeaderValue::from_static("000000000000000000000000000000000000000000000000"),
    );
    assert_eq!(controller_token(&headers), Some(ResumeToken([0; 24])));

    headers.insert(
        CONTROLLER_TOKEN_HEADER,
        HeaderValue::from_static("00000000000000000000000000000000000000000000000z"),
    );
    assert_eq!(controller_token(&headers), None);
    headers.insert(
        CONTROLLER_TOKEN_HEADER,
        HeaderValue::from_static("0000000000000000000000000000000000000000000000"),
    );
    assert_eq!(controller_token(&headers), None);
}

#[test]
fn preview_coordinate_rejects_arithmetic_and_i32_overflow() {
    assert_eq!(preview_coordinate(0, 0).expect("zero map"), 0);
    assert!(preview_coordinate(u64::MAX, u16::MAX).is_err());
    assert!(preview_coordinate(i32::MAX as u64 * 2, 15).is_err());
}
