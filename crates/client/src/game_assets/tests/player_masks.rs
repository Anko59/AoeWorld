//! Retain real player masks instead of baking one player's RGB into everybody.
use super::*;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn streamed_owner_masks_keep_native_rgb_coverage_and_partial_base_alpha() {
    let mut manifest: Manifest = serde_json::from_value(manifest::fixture()).unwrap();
    let frame = &mut manifest.frames[0];
    frame.width = 2;
    frame.height = 1;
    let placed = Placement {
        page: 2,
        x: 2,
        y: 2,
        width: 2,
        height: 1,
    };
    let mask_placed = Placement { page: 5, ..placed };
    let mut colour = vec![0; GAME_ATLAS_PAGE_BYTES];
    let mut player = vec![0; GAME_ATLAS_PAGE_BYTES];
    colour[..8].copy_from_slice(&[117, 120, 115, 128, 65, 145, 245, 255]);
    player[..4].copy_from_slice(&[7, 0, 0, 119]);
    let mut pixels = vec![0; GAME_ATLAS_PAGE_BYTES];
    let mut masks = vec![0; GAME_ATLAS_PAGE_BYTES];
    placement::copy_plane(
        frame,
        placed,
        placement::Plane::Color,
        &colour,
        &mut pixels,
        [0; 3],
    )
    .unwrap();
    placement::copy_plane(
        frame,
        placed,
        placement::Plane::PlayerCoverage,
        &player,
        &mut pixels,
        [0; 3],
    )
    .unwrap();
    placement::copy_plane(
        frame,
        mask_placed,
        placement::Plane::MaskBase,
        &colour,
        &mut masks,
        [0; 3],
    )
    .unwrap();
    placement::copy_plane(
        frame,
        mask_placed,
        placement::Plane::MaskPlayer,
        &player,
        &mut masks,
        [0; 3],
    )
    .unwrap();
    let at = (2 * 2048 + 2) * 4;
    assert_eq!(
        &pixels[at..at + 8],
        &[117, 120, 115, 187, 65, 145, 245, 255]
    );
    assert_eq!(&masks[at..at + 8], &[7, 119, 128, 0, 0, 0, 255, 0]);
}
