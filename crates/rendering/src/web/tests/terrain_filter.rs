use super::terrain_blend::{assert_pixel, read_pixel, surface_renderer_with_atlas};
use super::*;
use crate::game_renderer::filter_fixture as fixture;

#[wasm_bindgen_test]
async fn webgpu_ground_shows_black_explored_and_visible_fog() {
    let mut renderer = surface_renderer_with_atlas(&fixture::atlas()).await;
    let (faces, probe, _) = fixture::case(1);
    for (light, expected) in [(0.0, 0), (0.45, 115), (1.0, 255)] {
        let mut packet = crate::web::surface_instance(&faces[1], [128.0; 2], 0.0);
        crate::game_renderer::fog::surface_packet(&mut packet, [light; 3]);
        renderer
            .render_world_layers(&[], &[packet], [0.0, 0.0, 0.0, 1.0])
            .unwrap();
        assert_pixel(
            read_pixel(&renderer, 1, probe).await,
            [expected, expected, expected, 255],
        );
    }
    renderer.device.destroy();
}

#[wasm_bindgen_test]
async fn webgpu_terrain_filter_attenuates_checker_preserves_coverage_depth_and_rects() {
    let mut renderer = surface_renderer_with_atlas(&fixture::atlas()).await;
    for index in 0..fixture::CASES {
        let (mut faces, probe, expected) = fixture::case(index);
        for _ in 0..2 {
            renderer
                .render_world_layers(&faces, &[], [0.0, 0.0, 0.0, 1.0])
                .unwrap();
            assert_pixel(read_pixel(&renderer, 2, probe).await, expected);
            faces.reverse();
        }
    }
    renderer
        .render_world_layers(&[], &[fixture::sprite().0], [0.0, 0.0, 0.0, 1.0])
        .unwrap();
    // Minified object sprites take the bounded box kernel (two white, two
    // black taps of the checker at 8x8 pixels), matching the Canvas kernel.
    assert_pixel(
        read_pixel(&renderer, 1, [48, 48]).await,
        [128, 128, 128, 255],
    );
    renderer.device.destroy();
}

#[wasm_bindgen_test]
async fn webgpu_terrain_quads_keep_dev_pixels_at_quarter_and_native_zoom_with_objects() {
    use super::terrain_blend::read_pixels_with_clear;
    use crate::game_renderer::minification_tests::{fixture as object, terrain_pixels as terrain};
    for zoom in [0.25, 1.0] {
        let (_, quads, atlas) = terrain::scene(zoom);
        let expected = terrain::nearest_image(&quads, &atlas, true);
        let mut renderer = surface_renderer_with_atlas(&atlas).await;
        for objects in [false, true] {
            let mut packets: Vec<_> = quads.iter().map(|q| q.0).collect();
            if objects {
                packets.push(
                    object::placed(object::CHECKER, [4.0; 2], [4.0; 2], false, object::PHASE)
                        .sprite,
                );
            }
            renderer
                .render_world_layers(&[], &packets, [0.0, 0.0, 0.0, 1.0])
                .unwrap();
            let clear = wgpu::Color {
                r: 41.0 / 255.0,
                g: 74.0 / 255.0,
                b: 36.0 / 255.0,
                a: 1.0,
            };
            let pixels =
                read_pixels_with_clear(&renderer, packets.len() as u32, terrain::PROBES, clear)
                    .await;
            for (pixel, [x, y]) in pixels.into_iter().zip(terrain::PROBES) {
                assert_eq!(
                    pixel,
                    object::pixel(&expected, x, y),
                    "terrain zoom{zoom} objects{objects} pixel{x},{y}"
                );
            }
            if objects {
                assert_pixel(
                    read_pixel(&renderer, packets.len() as u32, [5, 5]).await,
                    [0, 128, 0, 255],
                );
            }
        }
        renderer.device.destroy();
    }
}
