//! Exact pixel/depth parity against the frozen previous raster, not tolerance tests.
use super::*;
use aoe_core::ScreenPoint;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn canvas_integer_alpha_preserves_all_source_target_and_alpha_bytes() {
    let mut depth = [f64::NEG_INFINITY];
    for alpha in 1..255_u16 {
        let source_alpha = f32::from(alpha) / 255.0;
        let target_alpha = 1.0 - source_alpha;
        for source in 0..256_u16 {
            for target in 0..256_u16 {
                let expected = (f32::from(source) * source_alpha + f32::from(target) * target_alpha)
                    .round() as u8;
                let mut color = [target as u8; 4];
                write_fragment(
                    0,
                    0,
                    1,
                    0.0,
                    [source as u8, source as u8, source as u8, alpha as u8],
                    &mut color,
                    &mut depth,
                );
                assert_eq!(color, [expected, expected, expected, 255]);
                assert_eq!(depth[0].to_bits(), 0.0_f64.to_bits());
            }
        }
    }
}

#[wasm_bindgen_test]
fn canvas_texel_coordinates_preserve_floor_clamp_for_ieee_values() {
    let side = GAME_ATLAS_SIDE as usize;
    let mut pixels = vec![0; crate::GAME_ATLAS_BYTES];
    for (index, texel) in pixels.chunks_exact_mut(4).enumerate() {
        let (x, y) = (index % side, index / side);
        texel.copy_from_slice(&[x as u8, (x >> 8) as u8, y as u8, (y >> 8) as u8]);
    }
    let edge = f64::from(GAME_ATLAS_SIDE - 1);
    let values = [
        f64::NEG_INFINITY,
        -f64::MAX,
        -1.5,
        -1.0,
        -f64::MIN_POSITIVE,
        -0.0,
        0.0,
        f64::MIN_POSITIVE,
        0.5,
        1.0,
        1.5,
        edge - 0.5,
        edge,
        edge + 0.5,
        edge + 1.0,
        f64::MAX,
        f64::INFINITY,
        f64::NAN,
    ];
    for x in values {
        for y in values {
            assert_eq!(
                sample_atlas(&pixels, 0, x, y),
                reference::sample_atlas(&pixels, 0, x, y)
            );
        }
    }
    let mut seed = 0xa375_1234_019a_55df_u64;
    for _ in 0..10_000 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let x = f64::from_bits(seed);
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let y = f64::from_bits(seed);
        assert_eq!(
            sample_atlas(&pixels, 0, x, y),
            reference::sample_atlas(&pixels, 0, x, y)
        );
    }
}

const WIDTH: u32 = 48;
const HEIGHT: u32 = 32;
const BOUNDS: [u32; 4] = [0, 0, WIDTH, HEIGHT];

fn atlas() -> Vec<u8> {
    (0..crate::GAME_ATLAS_BYTES)
        .map(|index| {
            if index % 4 == 3 {
                [0, 1, 127, 254, 255][(index / 4) % 5]
            } else {
                (index.wrapping_mul(73).wrapping_add(index / 17) % 256) as u8
            }
        })
        .collect()
}

fn buffers(seed: usize) -> (Vec<u8>, Vec<f64>) {
    let count = WIDTH as usize * HEIGHT as usize;
    let color = (0..count * 4)
        .map(|index| (index * 13 + seed) as u8)
        .collect();
    let depth = (0..count)
        .map(|index| [f64::NEG_INFINITY, -0.0, 0.0, 8.0, 80.0, f64::NAN][(index + seed) % 6])
        .collect();
    (color, depth)
}

fn parity(actual: &(Vec<u8>, Vec<f64>), expected: &(Vec<u8>, Vec<f64>)) {
    assert_eq!(actual.0, expected.0, "RGBA changed");
    assert_eq!(
        actual
            .1
            .iter()
            .map(|value| value.to_bits())
            .collect::<Vec<_>>(),
        expected
            .1
            .iter()
            .map(|value| value.to_bits())
            .collect::<Vec<_>>(),
        "depth bits changed",
    );
}

fn triangle(mode: u8, tint: u8, textured: bool, blend: bool) -> ProjectedSurfaceTriangle {
    let screen = [[-3.25, 2.0], [46.0, -2.125], [21.75, 33.0]];
    ProjectedSurfaceTriangle {
        appearance: 0,
        points: std::array::from_fn(|index| SurfacePoint {
            world: [
                index as f64 * 5.0,
                index as f64 * 2.0,
                [-4.0, 2.0, 13.0][index],
            ],
            screen: ScreenPoint {
                x: screen[index][0],
                y: screen[index][1],
            },
        }),
        color: [0.31, 0.77, 0.25],
        tile: [0, 0],
        skirt: mode % 2 == 0,
        material: 0,
        texture_mode: mode,
        tint,
        texture_uv: textured.then_some(crate::AtlasAddress {
            page: 0,
            uv: [0.03125, 0.0625, 97.0 / 2048.0, 49.0 / 2048.0],
        }),
        texture_blend: blend.then_some([
            crate::AtlasAddress {
                page: 1,
                uv: [0.5, 0.125, 97.0 / 2048.0, 49.0 / 2048.0],
            },
            crate::AtlasAddress {
                page: 2,
                uv: [1.0, -0.001, 1.0 / 2048.0, 0.0],
            },
        ]),
        texture_tile: [0, 0],
        texture_materials: None,
        pickable: true,
        order: 0,
    }
}

#[wasm_bindgen_test]
fn canvas_surface_kernel_preserves_texture_blend_water_and_depth_bits() {
    let atlas = atlas();
    for mode in 0..8 {
        for tint in [0, 1, 2, 3, 4, 11] {
            for (textured, blend) in [(false, false), (true, false), (true, true)] {
                let triangle = triangle(mode, tint, textured, blend);
                let mut actual = buffers(usize::from(mode + tint));
                let mut expected = actual.clone();
                raster_surface(
                    &triangle,
                    &atlas,
                    BOUNDS,
                    WIDTH,
                    &mut actual.0,
                    &mut actual.1,
                );
                reference::raster_surface(
                    &triangle,
                    &atlas,
                    BOUNDS,
                    WIDTH,
                    &mut expected.0,
                    &mut expected.1,
                );
                parity(&actual, &expected);
            }
        }
    }
}

#[wasm_bindgen_test]
fn canvas_sprite_kernel_preserves_flip_tint_alpha_equal_depth_and_background() {
    let atlas = atlas();
    for flipped in [false, true] {
        for tint in [
            [1.0; 4],
            [0.8, 0.7, 0.6, 0.28],
            [0.0; 4],
            [f32::INFINITY, -1.0, f32::NAN, 1.0],
        ] {
            for layer_depth in [
                f64::NEG_INFINITY,
                -0.0,
                0.0,
                8.0,
                80.0,
                f64::NAN,
                f64::INFINITY,
            ] {
                let sprite = crate::web::Sprite {
                    position: [0.17, -0.11],
                    radius: [0.25; 2],
                    color: tint,
                    uv: [
                        0.03125,
                        0.0625,
                        if flipped {
                            -97.0 / 2048.0
                        } else {
                            97.0 / 2048.0
                        },
                        49.0 / 2048.0,
                    ],
                    depths: [0.0; 4],
                    terrain_blend: [[0.0; 4]; 2],
                    pages: [0; 4],
                };
                let frame = crate::GameFrame {
                    atlas: crate::AtlasAddress {
                        page: sprite.pages[0],
                        uv: sprite.uv,
                    },
                    size: [43.25, 29.75],
                    anchor: [20.0, 27.0],
                };
                let mut actual = buffers(3);
                let mut expected = actual.clone();
                raster_sprite(
                    sprite,
                    frame,
                    layer_depth,
                    &atlas,
                    [f64::from(WIDTH), f64::from(HEIGHT)],
                    BOUNDS,
                    WIDTH,
                    &mut actual.0,
                    &mut actual.1,
                );
                reference::raster_sprite(
                    sprite,
                    frame,
                    layer_depth,
                    &atlas,
                    [f64::from(WIDTH), f64::from(HEIGHT)],
                    BOUNDS,
                    WIDTH,
                    &mut expected.0,
                    &mut expected.1,
                );
                parity(&actual, &expected);
            }
        }
    }
}

// Test-only old channel formulas complement the unchanged full-raster oracle.
fn original_sprite_transform(mut source: [u8; 4], multiply: [f32; 4]) -> [u8; 4] {
    for channel in 0..4 {
        source[channel] = (f32::from(source[channel]) * multiply[channel])
            .clamp(0.0, 255.0)
            .round() as u8;
    }
    source
}

fn original_terrain_tint(mut texel: [u8; 4], tint: u8) -> [u8; 4] {
    match tint {
        1..=3 => {
            let factor = [0.92, 0.78, 0.72][usize::from(tint - 1)];
            for channel in &mut texel[..3] {
                *channel = (f32::from(*channel) * factor).round() as u8;
            }
        }
        4 => {
            for (channel, water) in texel[..3].iter_mut().zip(WATER_TINT) {
                *channel = (f32::from(*channel) * 0.86 + f32::from(water) * 0.14).round() as u8;
            }
        }
        _ => {}
    }
    texel
}

#[wasm_bindgen_test]
fn canvas_shared_transform_preserves_every_terrain_tint_and_channel_byte() {
    for value in 0..=u8::MAX {
        let source = [
            value,
            value.wrapping_add(73),
            value.wrapping_add(127),
            value.wrapping_add(181),
        ];
        // New procedural codes have their own shared-kernel pixel regressions;
        // preserve the frozen oracle for all pre-existing/unknown codes.
        for tint in (0..=u8::MAX)
            .filter(|tint| !(5..=10).contains(tint) && *tint != 12 && !(21..=26).contains(tint))
        {
            assert_eq!(
                tint_sample(source, tint),
                original_terrain_tint(source, tint)
            );
        }
    }
}

#[wasm_bindgen_test]
fn canvas_shared_transform_preserves_arbitrary_ieee_sprite_factors() {
    let special = [
        f32::NEG_INFINITY,
        -f32::MAX,
        -1.0,
        -f32::MIN_POSITIVE,
        -0.0,
        0.0,
        f32::MIN_POSITIVE,
        0.5,
        1.0,
        f32::MAX,
        f32::INFINITY,
        f32::from_bits(0x7fc0_0001),
        f32::from_bits(0xffc0_0002),
        f32::from_bits(0x7f80_0001),
    ];
    for value in 0..=u8::MAX {
        let source = [value; 4];
        for factor in special {
            let multiply = [factor; 4];
            assert_eq!(
                transform_sample(source, multiply, [0.0; 4]),
                original_sprite_transform(source, multiply),
            );
        }
    }
    let mut seed = 0x619a_f235_u32;
    for _ in 0..100_000 {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let source = seed.to_le_bytes();
        let mut multiply = [0.0; 4];
        for factor in &mut multiply {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *factor = f32::from_bits(seed);
        }
        assert_eq!(
            transform_sample(source, multiply, [0.0; 4]),
            original_sprite_transform(source, multiply),
        );
    }
}

#[wasm_bindgen_test]
fn canvas_invalid_atlas_preserves_zero_alpha_and_untextured_surfaces() {
    for atlas in [&[][..], &[255; 16][..]] {
        for textured in [false, true] {
            let triangle = triangle(0, 4, textured, true);
            let mut actual = buffers(1);
            let mut expected = actual.clone();
            raster_surface(
                &triangle,
                atlas,
                BOUNDS,
                WIDTH,
                &mut actual.0,
                &mut actual.1,
            );
            reference::raster_surface(
                &triangle,
                atlas,
                BOUNDS,
                WIDTH,
                &mut expected.0,
                &mut expected.1,
            );
            parity(&actual, &expected);
        }
    }
}
