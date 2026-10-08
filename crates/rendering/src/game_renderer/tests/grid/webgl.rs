use super::*;
use crate::game_grid;
use aoe_core::TileRect;

#[wasm_bindgen_test]
fn webgl_continuous_grid_alpha_overlay_depth_and_opaque_triangle_control() {
    let (canvas, mut renderer) = target(&atlas(&[[255; 4]]));
    let c = SceneCamera {
        center: [0.0; 2],
        zoom: 1.0,
        viewport: [128.0; 2],
        focus_elevation_meters: 800.0,
    };
    let sprites = game_grid::grid_sprites(c, TileRect::from_xywh(-4, -4, 8, 8));
    assert!(!sprites.is_empty() && sprites.len() <= 512);
    let mut packets = sprites.clone();
    render(&mut renderer, &mut packets);
    assert!(
        packets.iter().all(|sprite| sprite.depths == [0.0; 4]),
        "positive infinity uploads normalize to front overlay depth"
    );
    for x in (16..=48).step_by(2) {
        assert_pixel(&canvas, x, x / 2 + 32, [39, 67, 35, 255]);
        assert_pixel(&canvas, x, x / 2 + 35, [41, 74, 36, 255]);
    }
    render(&mut renderer, &mut []);
    for x in (16..=48).step_by(2) {
        assert_pixel(&canvas, x, x / 2 + 32, [41, 74, 36, 255]);
    }
    // Place the absolute-world crossing at the exact pixel centre. Two strips
    // independently composite .2 alpha: combined opacity 1-(1-.2)^2 = .36.
    let mut crossing = c;
    crossing.center = [-3.0 / 256.0, -1.0 / 256.0];
    let mut cross_packets = game_grid::grid_sprites(crossing, TileRect::from_xywh(-4, -4, 8, 8));
    render(&mut renderer, &mut cross_packets);
    assert_pixel(&canvas, 64, 64, [37, 61, 34, 255]);
    render(&mut renderer, &mut []);
    assert_pixel(&canvas, 64, 64, [41, 74, 36, 255]);
    // Original untextured surface packets already carry uv.w=1. Consume that
    // same fourth component without changing their opaque color/depth pixels.
    let mut opaque = [Sprite {
        position: [-1.0, 1.0],
        radius: [1.0, 1.0],
        color: [-1.0, -1.0, 0.0, -2.0],
        uv: [120.0 / 255.0, 80.0 / 255.0, 200.0 / 255.0, 1.0],
        depths: [0.0; 4],
        terrain_blend: [[0.0; 4]; 2],
        pages: [0; 4],
    }];
    render(&mut renderer, &mut opaque);
    assert_pixel(&canvas, 32, 32, [120, 80, 200, 255]);
}
