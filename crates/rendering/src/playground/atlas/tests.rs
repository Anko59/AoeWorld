use super::*;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn equal_uvs_on_distinct_pages_remain_distinct_addresses() {
    let uv = [0.0, 0.0, 0.25, 0.5];
    assert_ne!(AtlasAddress::new(0, uv), AtlasAddress::new(1, uv));
    // Five unchanged colour pages plus three aligned player-mask pages. The
    // masks let every owner share one set of art instead of baking blue.
    assert_eq!(GAME_ATLAS_BYTES, 134_217_728);
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

#[wasm_bindgen_test]
fn atlas_growth_is_even_bounded_and_preserves_aligned_masks() {
    for pages in [8, 10, 12, 64] {
        assert_eq!(
            atlas_pages(pages * GAME_ATLAS_PAGE_BYTES),
            Some(pages as u32)
        );
    }
    for pages in [0, 1, 6, 9, 63, 66] {
        assert_eq!(atlas_pages(pages * GAME_ATLAS_PAGE_BYTES), None);
    }
    assert_eq!(atlas_pages(GAME_ATLAS_BYTES + 1), None);
    assert_eq!(atlas_mask_page(2), 5);
    assert_eq!(atlas_mask_page(4), 7);
    assert_eq!(atlas_mask_page(8), 9);
    assert_eq!(atlas_mask_page(62), 63);
}
