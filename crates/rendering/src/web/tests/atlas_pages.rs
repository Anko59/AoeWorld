use super::terrain_blend::{assert_pixel, read_pixel, surface_renderer};
use super::*;

fn address(page: u32, x: f32) -> crate::AtlasAddress {
    crate::AtlasAddress {
        page,
        uv: [x / 2048.0, 0.0, 1.0 / 2048.0, 1.0 / 2048.0],
    }
}

#[wasm_bindgen_test]
async fn webgpu_array_same_uv_primary_blends_and_page_two_body_shadow_pixels() {
    // Reuse one real device/three-layer atlas for all probes, then explicitly
    // destroy it. Allocating a device per sentinel caused resource pressure.
    let mut renderer = surface_renderer().await;
    let gameplay = renderer.render_sprites(&[]).unwrap();
    assert_eq!(gameplay.atlas_pages, 3);
    assert_eq!(gameplay.atlas_bytes, 50_331_648);
    let mut triangle = capacity_surface();
    triangle.points = [[16.0, 16.0], [112.0, 16.0], [16.0, 112.0]].map(surface_point);
    triangle.tint = 0;
    for (page, expected) in [
        (0, [255, 0, 0, 255]),
        (1, [0, 255, 0, 255]),
        (2, [0, 0, 255, 255]),
    ] {
        triangle.texture_uv = Some(address(page, 0.0));
        let instance = surface_instance(&triangle, [128.0; 2], 0.0);
        assert_eq!(instance.pages, [page, 0, 0, 0]);
        renderer
            .render_world_layers(&[triangle], &[], [0.0, 0.0, 0.0, 1.0])
            .unwrap();
        assert_pixel(read_pixel(&renderer, 1, [48, 48]).await, expected);
    }
    for (pages, expected) in [
        ([0, 1, 2], [82, 86, 86, 255]),
        ([1, 2, 0], [86, 82, 86, 255]),
        ([2, 0, 1], [86, 86, 82, 255]),
    ] {
        triangle.texture_uv = Some(address(pages[0], 0.0));
        triangle.texture_blend = Some([address(pages[1], 0.0), address(pages[2], 0.0)]);
        let instance = surface_instance(&triangle, [128.0; 2], 0.0);
        assert_eq!(instance.pages, [pages[0], pages[1], pages[2], 0]);
        renderer
            .render_world_layers(&[triangle], &[], [0.0, 0.0, 0.0, 1.0])
            .unwrap();
        assert_pixel(read_pixel(&renderer, 1, [48, 48]).await, expected);
    }

    let body = Sprite {
        position: [0.0; 2],
        radius: [0.75; 2],
        color: [1.0; 4],
        uv: [0.0, 0.0, 2.0 / 2048.0, 1.0 / 2048.0],
        depths: [0.0; 4],
        terrain_blend: [[0.0; 4]; 2],
        pages: [2, 0, 0, 0],
    };
    renderer
        .render_world_layers(&[], &[body], [0.0, 0.0, 0.0, 1.0])
        .unwrap();
    assert_pixel(read_pixel(&renderer, 1, [40, 64]).await, [0, 0, 255, 255]);
    assert_pixel(read_pixel(&renderer, 1, [88, 64]).await, [255, 255, 0, 255]);
    let mirrored = Sprite {
        uv: [2.0 / 2048.0, 0.0, -2.0 / 2048.0, 1.0 / 2048.0],
        ..body
    };
    renderer
        .render_world_layers(&[], &[mirrored], [0.0, 0.0, 0.0, 1.0])
        .unwrap();
    assert_pixel(read_pixel(&renderer, 1, [40, 64]).await, [255, 255, 0, 255]);
    assert_pixel(read_pixel(&renderer, 1, [88, 64]).await, [0, 0, 255, 255]);
    let shadow = Sprite {
        uv: address(2, 2.0).uv,
        ..body
    };
    renderer
        .render_world_layers(&[], &[body, shadow], [0.0, 0.0, 0.0, 1.0])
        .unwrap();
    assert_pixel(read_pixel(&renderer, 2, [40, 64]).await, [0, 0, 127, 255]);
    renderer
        .render_world_layers(&[], &[shadow, body], [0.0, 0.0, 0.0, 1.0])
        .unwrap();
    assert_pixel(read_pixel(&renderer, 2, [40, 64]).await, [0, 0, 255, 255]);
    renderer.device.destroy();
}

#[wasm_bindgen_test]
fn gameplay_grid_and_selection_use_page_two_white_and_reserved_zero() {
    let camera = crate::SceneCamera {
        center: [0.5; 2],
        zoom: 1.0,
        viewport: [128.0; 2],
        focus_elevation_meters: 0.0,
    };
    let grid = crate::game_grid::grid_sprites(camera, crate::game_grid::viewport_bounds(camera));
    let ring = crate::game_grid::selection_ring(camera, [0.5; 2], 0.0);
    assert!(!grid.is_empty());
    assert_eq!(ring.len(), crate::game_grid::SELECTION_RING_SPRITES);
    for sprite in grid.iter().chain(ring.iter().map(|(sprite, _)| sprite)) {
        assert_eq!(sprite.pages, [2, 0, 0, 0]);
    }
}
