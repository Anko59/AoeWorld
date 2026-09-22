use super::*;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn terrain_instance_sentinel_cannot_match_atlas_uv_rectangles() {
    let regular_atlas_frame = Sprite {
        position: [0.0; 2],
        radius: [1.0; 2],
        color: [1.0; 4],
        uv: [0.1, 0.2, 0.3, 0.4],
    };
    let horizontally_flipped_atlas_frame = Sprite {
        uv: [0.4, 0.2, -0.3, 0.4],
        ..regular_atlas_frame
    };
    let terrain = surface_instance([[0.1, 0.2], [0.3, 0.4], [0.5, 0.6]], [0.2, 0.3, 0.4, 1.0]);

    assert!(regular_atlas_frame.uv[3] >= 0.0);
    assert!(horizontally_flipped_atlas_frame.uv[3] >= 0.0);
    assert_eq!(terrain.uv[3], -1.0);
    assert_eq!(terrain.position, [0.1, 0.2]);
    assert_eq!(terrain.radius, [0.3, 0.4]);
    assert_eq!(terrain.uv[..2], [0.5, 0.6]);
}
