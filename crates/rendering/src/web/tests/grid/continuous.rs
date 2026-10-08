use super::super::terrain_blend::{assert_pixel, read_pixel_with_clear, surface_renderer};
use super::*;

#[wasm_bindgen_test]
async fn webgpu_continuous_grid_diagonal_alpha_gap_toggle_and_opaque_control() {
    let mut renderer = surface_renderer().await;
    let c = SceneCamera {
        center: [0.0; 2],
        zoom: 1.0,
        viewport: [128.0; 2],
        focus_elevation_meters: 800.0,
    };
    let sprites = game_grid::grid_sprites(c, aoe_core::TileRect::from_xywh(-4, -4, 8, 8));
    assert!(!sprites.is_empty() && sprites.len() <= 512);
    let clear = [41.0 / 255.0, 74.0 / 255.0, 36.0 / 255.0, 1.0];
    let color = wgpu::Color {
        r: clear[0],
        g: clear[1],
        b: clear[2],
        a: clear[3],
    };
    renderer.render_sprites_with_clear(&sprites, clear).unwrap();
    for x in (16..=48).step_by(2) {
        assert_pixel(
            read_pixel_with_clear(&renderer, sprites.len() as u32, [x, x / 2 + 32], color).await,
            [39, 67, 35, 255],
        );
        assert_pixel(
            read_pixel_with_clear(&renderer, sprites.len() as u32, [x, x / 2 + 35], color).await,
            [41, 74, 36, 255],
        );
    }
    renderer.render_sprites_with_clear(&[], clear).unwrap();
    for x in (16..=48).step_by(2) {
        assert_pixel(
            read_pixel_with_clear(&renderer, 0, [x, x / 2 + 32], color).await,
            [41, 74, 36, 255],
        );
    }
    // At an exact pixel-centred crossing the two alpha-.2 strips independently
    // composite, yielding .36 combined opacity rather than a union-path .2.
    let mut crossing = c;
    crossing.center = [-3.0 / 256.0, -1.0 / 256.0];
    let cross_packets =
        game_grid::grid_sprites(crossing, aoe_core::TileRect::from_xywh(-4, -4, 8, 8));
    renderer
        .render_sprites_with_clear(&cross_packets, clear)
        .unwrap();
    assert_pixel(
        read_pixel_with_clear(&renderer, cross_packets.len() as u32, [64, 64], color).await,
        [37, 61, 34, 255],
    );
    renderer.render_sprites_with_clear(&[], clear).unwrap();
    assert_pixel(
        read_pixel_with_clear(&renderer, 0, [64, 64], color).await,
        [41, 74, 36, 255],
    );
    // Legacy procedural surfaces already encode alpha one in the fourth UV word.
    let opaque = [Sprite {
        position: [-1.0, 1.0],
        radius: [1.0, 1.0],
        color: [-1.0, -1.0, 0.0, -2.0],
        uv: [120.0 / 255.0, 80.0 / 255.0, 200.0 / 255.0, 1.0],
        depths: [0.0; 4],
        terrain_blend: [[0.0; 4]; 2],
        pages: [0; 4],
    }];
    renderer.render_sprites_with_clear(&opaque, clear).unwrap();
    assert_pixel(
        read_pixel_with_clear(&renderer, 1, [32, 32], color).await,
        [120, 80, 200, 255],
    );
    renderer.device.destroy();
}
