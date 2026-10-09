use super::appearance_tests::{camera, map};
use super::*;

#[wasm_bindgen_test]
fn fixed_texture_frame_loop_preserves_array_map_option_and_uv_bits() {
    let template = projected_surface_triangles(&map(2), camera([0.5, 0.5], 1.0, [256.0, 128.0]))[0];
    for uv in [
        [0.0, -0.0, 0.01, 0.02],
        [f32::from_bits(0x7fc0_0001), f32::INFINITY, -0.0, 0.01],
    ] {
        for missing in 0..3 {
            let mut art = test_art(GameFrame {
                atlas: crate::AtlasAddress { page: 0, uv },
                size: [97.0, 49.0],
                anchor: [48.0, 24.0],
            });
            if missing != 0 {
                art.grass.clear();
                art.terrain[1].clear();
            }
            if missing == 2 {
                for frames in &mut art.terrain {
                    frames.clear();
                }
            }
            for materials in [None, Some([0, 1, 255]), Some([2; 3]), Some([6, 0, 2])] {
                let mut triangle = template;
                triangle.material = 1;
                triangle.texture_materials = materials;
                let frames = materials.unwrap_or([triangle.material; 3]).map(|material| {
                    terrain_texture_frame(&art, material, triangle.texture_tile)
                        .map(|frame| frame.atlas)
                });
                let expected_blend = match frames {
                    [Some(a), Some(b), Some(c)] if a != b || a != c => Some([b, c]),
                    _ => None,
                };
                apply_terrain_textures(std::slice::from_mut(&mut triangle), &art);
                assert_eq!(
                    triangle
                        .texture_uv
                        .map(|uv| (uv.page, uv.uv.map(f32::to_bits))),
                    frames[0].map(|uv| (uv.page, uv.uv.map(f32::to_bits))),
                );
                assert_eq!(
                    triangle
                        .texture_blend
                        .map(|rects| rects.map(|uv| (uv.page, uv.uv.map(f32::to_bits)))),
                    expected_blend.map(|rects| rects.map(|uv| (uv.page, uv.uv.map(f32::to_bits)))),
                );
            }
        }
    }
}

#[wasm_bindgen_test]
fn fixed_height_conversion_preserves_old_array_map_bits() {
    let mut seed = 0x917a_2345_u32;
    for _ in 0..1_024 {
        let mut corners = [0_i16; 4];
        for corner in &mut corners {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *corner = seed as i16;
        }
        for triangulation in 0..3 {
            for (x, y) in [(0.0, -0.0), (0.25, 0.75), (0.75, 0.25), (f64::NAN, 0.5)] {
                let expected =
                    sample_float_surface_height(corners.map(f64::from), triangulation, x, y);
                assert_eq!(
                    sample_surface_height(corners, triangulation, x, y).to_bits(),
                    expected.to_bits(),
                );
            }
        }
    }
}
