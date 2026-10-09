use super::*;
use wasm_bindgen_test::wasm_bindgen_test;

fn page() -> Vec<u8> {
    vec![0; GAME_ATLAS_PAGE_BYTES]
}

#[wasm_bindgen_test]
fn an_empty_page_needs_no_rows() {
    assert_eq!(occupied_rows(&page()), 0);
}

#[wasm_bindgen_test]
fn rows_end_at_the_last_row_holding_any_nonzero_byte() {
    let row = GAME_ATLAS_SIDE as usize * 4;
    let mut pixels = page();
    pixels[0] = 1;
    assert_eq!(occupied_rows(&pixels), 1);
    // Alpha alone counts: a transparent-colour but opaque texel is content.
    pixels[17 * row + 3] = 255;
    assert_eq!(occupied_rows(&pixels), 18);
    pixels[GAME_ATLAS_PAGE_BYTES - 1] = 9;
    assert_eq!(occupied_rows(&pixels), GAME_ATLAS_SIDE);
}

#[wasm_bindgen_test]
fn a_gap_of_empty_rows_inside_the_content_is_still_uploaded() {
    let row = GAME_ATLAS_SIDE as usize * 4;
    let mut pixels = page();
    pixels[2 * row] = 1;
    pixels[40 * row + row - 1] = 1;
    assert_eq!(occupied_rows(&pixels), 41);
}
