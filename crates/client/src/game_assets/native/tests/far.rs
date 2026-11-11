use super::*;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn thumbnail_uses_native_owner_weights_and_preserves_unmasked_art_and_shadows() {
    let colour = [12, 30, 220, 255, 80, 90, 100, 255, 0, 0, 0, 0];
    let mask = [0, 0, 0, 0, 7, 0, 0, 128, 0, 0, 0, 0];
    let shadow = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 70];
    let bytes = compressed::encode_frame([3, 1], [&colour, &mask, &shadow]).unwrap();
    let pixels = strategic_frame(&bytes, 3, 1, 0x4191f5).unwrap();
    assert_eq!(&pixels[..4], &colour[..4]);
    assert_eq!(
        &pixels[4..8],
        &aoe_assets::pack::blend_player([80, 90, 100, 255], 7, 128, [65, 145, 245])
    );
    assert_eq!(&pixels[8..], &shadow[8..]);
}
