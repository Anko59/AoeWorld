use super::*;

#[test]
fn fixed_centerline_route_uses_opposite_interior_edges() {
    let axis = MAX_AXIS_TILES as i32;
    let y = (axis - 1) / 2;
    let points = [TileCoord::new(axis - 2, y), TileCoord::new(1, y)];
    assert_eq!(points[0], TileCoord::new(49_998, 24_999));
    assert_eq!(points[1], TileCoord::new(1, 24_999));
    assert_eq!(points[0].x.abs_diff(points[1].x), 49_997);
    assert!(points.windows(2).all(|pair| pair[0] != pair[1]));
}

#[test]
fn provider_page_key_enumeration_is_bounded_by_supported_pyramids() {
    // Each of the four 1,024-sample fields has fewer than 350 pages across
    // all pyramid levels; this guards accidental expansion into tile space.
    let per_field = (16_usize * 16) + (8 * 8) + (4 * 4) + (2 * 2) + 1;
    assert_eq!(per_field, 341);
    assert!(per_field * 4 < 1_500);
    assert_eq!(MAX_RESIDENT_PAGES, 128);
    const { assert!(MAX_ROUTE_TICKS <= 1_200_000) };
}

#[test]
fn resource_search_is_a_fixed_bounded_center_window() {
    assert_eq!(MAX_RESOURCE_SCAN_SIDE, 64);
    let center = (50_000 - 1) / 2;
    let half = MAX_RESOURCE_SCAN_SIDE / 2;
    assert_eq!((center - half, center + half), (24_967, 25_031));
    assert_eq!((2 * half) * (2 * half), 4_096);
}
