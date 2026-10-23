//! GPU pixels: hidden contours, transparent coverage and normal visible bodies.
use super::terrain_blend::{assert_pixel, read_pixels, surface_renderer_with_atlas};
use super::*;

#[wasm_bindgen_test]
async fn resource_hover_modes_follow_visible_sprite_alpha_and_preserve_the_interior() {
    let mut renderer = surface_renderer_with_atlas(&atlas()).await;
    for mode in [0, 16, 32] {
        let resource = Sprite {
            pages: [0, 0, 0, mode],
            ..unit([0.0; 4])
        };
        let counters = renderer.render_sprites(&[resource]).unwrap();
        assert_eq!(counters.draw_calls, if mode == 0 { 1 } else { 2 });
        let pixels = read_pixels(&renderer, 1, [[40, 64], [44, 64], [64, 64], [36, 64]]).await;
        assert_pixel(pixels[2], [220, 30, 80, 255]);
        assert_pixel(pixels[3], [0, 0, 0, 255]);
        if mode == 0 {
            assert_pixel(pixels[0], [220, 30, 80, 255]);
        } else {
            assert!(pixels[0][1] > 60, "warm rim must reach the body edge");
        }
        if mode == 32 {
            assert!(pixels[1][1] > 50, "glow has a softer, wider falloff");
        } else {
            assert_pixel(pixels[1], [220, 30, 80, 255]);
        }
    }
    renderer.device.destroy();
}

#[wasm_bindgen_test]
async fn resource_hover_never_bleeds_through_a_nearer_object_or_recolours_its_shadow() {
    let mut renderer = surface_renderer_with_atlas(&atlas()).await;
    for mode in [16, 32] {
        let resource = Sprite {
            pages: [0, 0, 0, mode],
            ..unit([0.0; 4])
        };
        renderer.render_sprites(&[resource, blocker(4)]).unwrap();
        let pixels = read_pixels(&renderer, 2, [[87, 64], [64, 64], [40, 64]]).await;
        assert_pixel(pixels[0], [240, 200, 20, 255]);
        assert_pixel(pixels[1], [240, 200, 20, 255]);
        assert!(pixels[2][1] > 60);
        let shadow = Sprite {
            color: [0.0, 0.0, 0.0, 0.2],
            ..resource
        };
        renderer.render_sprites(&[shadow]).unwrap();
        let pixels = read_pixels(&renderer, 1, [[40, 64]]).await;
        assert_pixel(pixels[0], [0, 0, 0, 255]);
    }
    renderer.device.destroy();
}

fn atlas() -> Vec<u8> {
    let mut pixels = vec![0; crate::GAME_ATLAS_BYTES];
    for y in 0..16 {
        for x in 0..16 {
            let at = (y * 2048 + x) * 4;
            if (2..14).contains(&x) && (2..14).contains(&y) {
                pixels[at..at + 4].copy_from_slice(&[220, 30, 80, 255]);
            }
            let at = (y * 2048 + x + 32) * 4;
            if !(2..4).contains(&x) || !(7..9).contains(&y) {
                pixels[at..at + 4].copy_from_slice(&[240, 200, 20, 255]);
            }
        }
    }
    pixels
}

#[wasm_bindgen_test]
async fn a_hovered_low_object_does_not_gain_tall_object_occlusion() {
    let mut renderer = surface_renderer_with_atlas(&atlas()).await;
    renderer
        .render_sprites(&[unit([0.0, 0.0, 1.0, 1.0]), blocker(16)])
        .unwrap();
    let pixels = read_pixels(&renderer, 2, [[87, 64], [40, 64]]).await;
    assert_pixel(pixels[0], [240, 200, 20, 255]);
    assert_pixel(pixels[1], [220, 30, 80, 255]);
    renderer.device.destroy();
}

fn unit(color: [f32; 4]) -> Sprite {
    Sprite {
        position: [0.0; 2],
        radius: [0.5; 2],
        color: [1.0; 4],
        uv: [0.0, 0.0, 16.0 / 2048.0, 16.0 / 2048.0],
        terrain_blend: [color, [0.0; 4]],
        pages: [0, 0, 0, 8],
        depths: [0.0; 4],
    }
}

fn blocker(kind: u32) -> Sprite {
    Sprite {
        position: [0.5, 0.0],
        radius: [0.5, 0.75],
        uv: [32.0 / 2048.0, 0.0, 16.0 / 2048.0, 16.0 / 2048.0],
        pages: [0, 0, 0, kind],
        depths: [1.0; 4],
        ..unit([0.0; 4])
    }
}

#[wasm_bindgen_test]
async fn hidden_unit_contours_use_each_player_colour_and_visible_parts_stay_native() {
    let mut renderer = surface_renderer_with_atlas(&atlas()).await;
    for color in [[0.0, 0.0, 1.0, 1.0], [0.0, 1.0, 0.0, 1.0]] {
        let actor = unit(color);
        for mirrored in [false, true] {
            let mut actor = actor;
            if mirrored {
                actor.uv[0] += actor.uv[2];
                actor.uv[2] = -actor.uv[2];
            }
            for reversed in [false, true] {
                let objects = if reversed {
                    [blocker(4), actor]
                } else {
                    [actor, blocker(4)]
                };
                renderer.render_sprites(&objects).unwrap();
                let pixels = read_pixels(
                    &renderer,
                    2,
                    [[87, 64], [48, 64], [72, 64], [80, 64], [95, 64]],
                )
                .await;
                assert_pixel(pixels[0], color.map(|c| (c * 255.0) as u8));
                assert_pixel(pixels[1], [220, 30, 80, 255]);
                assert_pixel(pixels[2], [220, 30, 80, 255]); // Transparent window.
                assert_pixel(pixels[3], [240, 200, 20, 255]); // Hidden interior.
                assert_pixel(pixels[4], [240, 200, 20, 255]); // Outside actor alpha.
            }
        }
    }
    renderer.device.destroy();
}

#[wasm_bindgen_test]
async fn contours_require_a_tall_occluder_and_a_unit_behind_it() {
    let mut renderer = surface_renderer_with_atlas(&atlas()).await;
    for kind in [0, 1, 8] {
        // Ordinary objects, terrain quads, other units.
        renderer
            .render_sprites(&[unit([0.0, 0.0, 1.0, 1.0]), blocker(kind)])
            .unwrap();
        assert_pixel(
            read_pixels(&renderer, 2, [[87, 64]]).await[0],
            [240, 200, 20, 255],
        );
    }
    let mut actor = unit([0.0, 0.0, 1.0, 1.0]);
    actor.depths = [2.0; 4];
    renderer.render_sprites(&[blocker(4), actor]).unwrap();
    assert_pixel(
        read_pixels(&renderer, 2, [[87, 64]]).await[0],
        [220, 30, 80, 255],
    );
    renderer.device.destroy();
}

#[wasm_bindgen_test]
async fn equal_depth_contours_follow_the_final_painter_and_never_replace_visible_parts() {
    let mut renderer = surface_renderer_with_atlas(&atlas()).await;
    let mut actor = unit([0.0, 0.0, 1.0, 1.0]);
    actor.depths = [1.0; 4];
    for (sprites, expected) in [
        ([actor, blocker(4)], [0, 0, 255, 255]),
        ([blocker(4), actor], [220, 30, 80, 255]),
    ] {
        renderer.render_sprites(&sprites).unwrap();
        assert_pixel(read_pixels(&renderer, 2, [[87, 64]]).await[0], expected);
    }
    renderer.device.destroy();
}

#[wasm_bindgen_test]
async fn translucent_shadows_do_not_turn_visible_unit_pixels_into_contours() {
    let mut pixels = atlas();
    for y in 0..16 {
        for x in 32..48 {
            let alpha = &mut pixels[(y * 2048 + x) * 4 + 3];
            if *alpha > 0 {
                *alpha = 128;
            }
        }
    }
    let mut renderer = surface_renderer_with_atlas(&pixels).await;
    renderer
        .render_sprites(&[unit([0.0, 0.0, 1.0, 1.0]), blocker(4)])
        .unwrap();
    assert_pixel(
        read_pixels(&renderer, 2, [[87, 64]]).await[0],
        [230, 115, 50, 255],
    );
    renderer.device.destroy();
}

#[wasm_bindgen_test]
async fn contours_survive_resize_with_one_extra_draw_and_the_same_instance_buffer() {
    let mut renderer = surface_renderer_with_atlas(&atlas()).await;
    let scene = [unit([0.0, 0.0, 1.0, 1.0]), blocker(4)];
    let first = renderer.render_sprites(&scene).unwrap();
    renderer.resize(256, 128);
    let resized = renderer.render_sprites(&scene).unwrap();
    assert_pixel(
        read_pixels(&renderer, 2, [[175, 64]]).await[0],
        [0, 0, 255, 255],
    );
    assert_eq!(resized.draw_calls, 2);
    assert_eq!(resized.gpu_buffer_bytes, first.gpu_buffer_bytes);
    assert_eq!(
        resized.persistent_gpu_resources,
        first.persistent_gpu_resources
    );
    renderer.device.destroy();
}
