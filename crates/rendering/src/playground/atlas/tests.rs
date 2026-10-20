use super::*;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn equal_uvs_on_distinct_pages_remain_distinct_addresses() {
    let uv = [0.0, 0.0, 0.25, 0.5];
    assert_ne!(AtlasAddress::new(0, uv), AtlasAddress::new(1, uv));
    assert_eq!(GAME_ATLAS_BYTES, 67_108_864);
    assert_eq!(AtlasAddress::WHITE.page, 2);
}

#[wasm_bindgen_test]
fn declared_sheet_preserves_authored_signed_coordinate_phase() {
    let sheet = TerrainTopology::PeriodicXMajorReversedY {
        columns: 10,
        rows: 10,
    };
    for (x, y, expected) in [(0, 0, 0), (0, 1, 9), (-1, -1, 91), (10, 10, 0)] {
        assert_eq!(sheet.periodic_frame(x, y, 100), Some(expected));
    }
    assert_eq!(sheet.periodic_frame(0, 0, 99), None);
    assert_eq!(
        TerrainTopology::CoordinateStableAccents.periodic_frame(0, 0, 100),
        None
    );
    assert_eq!(
        TerrainTopology::PeriodicXMajorReversedY {
            columns: 0,
            rows: 10
        }
        .periodic_frame(0, 0, 0),
        None
    );
}
