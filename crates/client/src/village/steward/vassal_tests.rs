use super::*;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn vassal_squad_rows_show_each_live_activity_and_omit_empty_squads() {
    let squads = [[2, 1, 5], [0; 3], [12, 0, 0], [0; 3], [0; 3]];
    assert_eq!(
        squads_text(squads),
        "Infantry · 5 attacking, 1 moving, 2 holding\nCavalry · 12 holding\n"
    );
    assert!(squads_text([[0; 3]; 5]).is_empty());
}
