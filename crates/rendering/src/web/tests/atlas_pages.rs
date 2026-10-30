use super::terrain_blend::{assert_pixel, read_pixel, read_pixels, surface_renderer};
use super::*;

fn address(page: u32, x: f32) -> crate::AtlasAddress {
    crate::AtlasAddress {
        page,
        uv: [x / 2048.0, 0.0, 1.0 / 2048.0, 1.0 / 2048.0],
    }
}

#[wasm_bindgen_test]
async fn webgpu_array_same_uv_primary_blends_and_page_two_body_shadow_pixels() {
    // Reuse one real device/eight-layer atlas for all probes, then explicitly
    // destroy it. Allocating a device per sentinel caused resource pressure.
    let mut renderer = surface_renderer().await;
    let gameplay = renderer.render_sprites(&[]).unwrap();
    // Three aligned player-mask pages now accompany the five colour pages.
    assert_eq!(gameplay.atlas_pages, 8);
    assert_eq!(gameplay.atlas_bytes, 134_217_728);
    let mut triangle = capacity_surface();
    triangle.points = [[16.0, 16.0], [112.0, 16.0], [16.0, 112.0]].map(surface_point);
    triangle.tint = 0;
    for (page, expected) in [
        (0, [255, 0, 0, 255]),
        (1, [0, 255, 0, 255]),
        (2, [0, 0, 255, 255]),
        (3, [255, 0, 255, 255]),
        (4, [255, 128, 0, 255]),
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
    let pixels = read_pixels(&renderer, 1, [[40, 64], [88, 64]]).await;
    assert_pixel(pixels[0], [0, 0, 255, 255]);
    assert_pixel(pixels[1], [255, 255, 0, 255]);
    let mirrored = Sprite {
        uv: [2.0 / 2048.0, 0.0, -2.0 / 2048.0, 1.0 / 2048.0],
        ..body
    };
    renderer
        .render_world_layers(&[], &[mirrored], [0.0, 0.0, 0.0, 1.0])
        .unwrap();
    let pixels = read_pixels(&renderer, 1, [[40, 64], [88, 64]]).await;
    assert_pixel(pixels[0], [255, 255, 0, 255]);
    assert_pixel(pixels[1], [0, 0, 255, 255]);
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
fn selection_uses_an_analytic_ellipse_and_grid_lines_are_solid_packets() {
    let camera = crate::SceneCamera {
        center: [0.5; 2],
        zoom: 1.0,
        viewport: [128.0; 2],
        focus_elevation_meters: 0.0,
    };
    let point = |x, y| aoe_core::ScreenPoint { x, y };
    let grid = crate::game_grid::grid_sprites(&[[point(0.0, 32.0), point(64.0, 64.0)]], [128.0; 2]);
    let ring = crate::game_grid::selection_ring(camera, [0.5; 2], 0.0);
    assert_eq!(grid.len(), 2);
    assert_eq!(ring.len(), crate::game_grid::SELECTION_RING_SPRITES);
    for (sprite, _) in &ring {
        assert_eq!(sprite.pages, [2, 0, 0, 2]);
        assert_eq!(sprite.uv[2], 2.0);
    }
    // Solid surface packets never sample the atlas: colour and alpha ride in uv.
    for sprite in &grid {
        assert_eq!(sprite.color[3], -2.0);
        assert_eq!(sprite.pages, [0; 4]);
        assert!(sprite.uv[3] > 0.0 && sprite.uv[3] < 1.0);
    }
}

#[wasm_bindgen_test]
async fn webgpu_source_batch_copies_rectangles_and_aligned_owner_masks() {
    let mut game = crate::GameRenderer::WebGpu(Box::new(surface_renderer().await));
    let mut upload = game.begin_game_atlas(10).unwrap();
    assert!(
        upload.resident_buffer().is_none(),
        "GPU blits never allocate a CPU destination atlas"
    );
    let mut colour = vec![0; crate::GAME_ATLAS_PAGE_BYTES];
    let mut mask = vec![0; crate::GAME_ATLAS_PAGE_BYTES];
    colour[..4].copy_from_slice(&[255, 0, 0, 255]);
    let at = (4 * 2048 + 3) * 4;
    colour[at..at + 4].copy_from_slice(&[120, 90, 60, 255]);
    mask[at..at + 4].copy_from_slice(&[7, 255, 255, 0]);
    let copy = crate::AtlasSourceCopy {
        source: [3, 4],
        destination: [2, 2],
        size: [1, 1],
        colour: 8,
        mask: Some(9),
    };
    assert!(
        game.write_game_atlas_source(
            &mut upload,
            &colour,
            Some(&mask),
            &[crate::AtlasSourceCopy {
                source: [65535, 0],
                ..copy
            }]
        )
        .is_err()
    );
    assert!(
        game.write_game_atlas_source(&mut upload, &colour, None, &[copy])
            .is_err()
    );
    let completion = game
        .write_game_atlas_source(&mut upload, &colour, Some(&mask), &[copy])
        .unwrap();
    crate::GameRenderer::atlas_source_ready(completion)
        .await
        .unwrap();
    game.finish_game_atlas(upload).unwrap();
    let crate::GameRenderer::WebGpu(renderer) = &mut game else {
        unreachable!();
    };
    let body = Sprite {
        radius: [0.75; 2],
        uv: [2.0 / 2048.0, 2.0 / 2048.0, 1.0 / 2048.0, 1.0 / 2048.0],
        pages: [8, 0, 0, 0],
        ..solid_sprite()
    }
    .with_owner(crate::SceneUnitVisual::new(false).with_owner(
        Some([65, 145, 245]),
        false,
        false,
    ));
    renderer
        .render_world_layers(&[], &[body], [0.0, 0.0, 0.0, 1.0])
        .unwrap();
    assert_pixel(read_pixel(renderer, 1, [48, 48]).await, [65, 145, 245, 255]);
    assert_eq!(renderer.render_sprites(&[]).unwrap().atlas_pages, 10);
    renderer.device.destroy();
}
