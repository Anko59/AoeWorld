use super::*;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn diamond_mask_stair_steps_do_not_make_holes_in_continuous_ground_edges() {
    let mut atlas = vec![0; crate::GAME_ATLAS_BYTES];
    for y in 0_i32..49 {
        for x in 0_i32..97 {
            if (x - 48).abs() + 2 * (y - 24).abs() <= 48 {
                let offset = (y as usize * 2048 + x as usize) * 4;
                atlas[offset..offset + 4].copy_from_slice(&[210, 220, 230, 255]);
            }
        }
    }
    let address = crate::AtlasAddress {
        page: 0,
        uv: [0.0, 0.0, 97.0 / 2048.0, 49.0 / 2048.0],
    };
    for at in [
        [0.759, 0.259],
        [0.241, 0.259],
        [0.759, 0.741],
        [0.241, 0.741],
    ] {
        assert_eq!(sample(&atlas, address, at, None), [210, 220, 230, 255]);
    }
    assert_eq!(
        sample(&atlas, address, [0.0, 0.0], None),
        [0; 4],
        "outside the diamond stays transparent"
    );
}

#[wasm_bindgen_test]
fn threshold_nonfinite_and_quadrants_keep_nearest_ieee_fallback() {
    let address = crate::AtlasAddress {
        page: 0,
        uv: [0.0, 0.0, 49.0 / 2048.0, 25.0 / 2048.0],
    };
    let atlas = crate::game_renderer::filter_fixture::atlas();
    let local = [0.65625, 0.34375];
    let nearest = sample_terrain_atlas(&atlas, address, local);
    for gradient in [
        [[1.25 / 48.0, 0.0], [0.0, 0.0]],
        [[f64::NAN, 0.0], [0.0, 1.0]],
        [[f64::INFINITY, 0.0], [0.0, 1.0]],
        [[0.0, 0.0], [f64::NEG_INFINITY, 0.0]],
    ] {
        let kernel = offsets(address, gradient);
        assert!(kernel.is_none());
        assert_eq!(sample(&atlas, address, local, kernel), nearest);
    }
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for axis in 0..4 {
            let mut malformed = address;
            malformed.uv[axis] = value;
            let kernel = offsets(malformed, [[3.0 / 48.0, 0.0], [0.0, 3.0 / 24.0]]);
            assert!(kernel.is_none());
            assert_eq!(
                sample(&atlas, malformed, local, kernel),
                sample_terrain_atlas(&atlas, malformed, local)
            );
        }
    }
    let kernel = offsets(address, [[3.0 / 48.0, 0.0], [0.0, 3.0 / 24.0]]).unwrap();
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let local = [value, 0.5];
        assert_eq!(
            sample(&atlas, address, local, Some(kernel)),
            sample_terrain_atlas(&atlas, address, local)
        );
    }
    assert_eq!(
        kernel,
        [
            [-0.015625, -0.03125],
            [-0.015625, 0.03125],
            [0.015625, -0.03125],
            [0.015625, 0.03125]
        ]
    );
    assert_eq!(
        sample(&atlas, address, local, Some(kernel)),
        [128, 128, 128, 255]
    );
}

#[wasm_bindgen_test]
fn owner_masks_colour_canvas_taps_before_minification_without_tinting_native_blue() {
    let mut atlas = vec![0; crate::GAME_ATLAS_BYTES];
    atlas[..8].copy_from_slice(&[65, 145, 245, 255, 65, 145, 245, 255]);
    let mask = 3 * crate::GAME_ATLAS_PAGE_BYTES;
    atlas[mask..mask + 4].copy_from_slice(&[7, 255, 255, 0]);
    for colour in crate::PLAYER_COLOURS {
        let sprite = Sprite {
            position: [0.0; 2],
            radius: [1.0; 2],
            color: [1.0; 4],
            uv: [0.0, 0.0, 2.0 / 2048.0, 1.0 / 2048.0],
            depths: [0.0; 4],
            terrain_blend: [[0.0; 4]; 2],
            pages: [0; 4],
        }
        .with_owner(crate::SceneUnitVisual::new(false).with_owner(
            Some(colour),
            false,
            false,
        ));
        assert_eq!(
            owner_sample(&atlas, sprite, 0.0, 0.0),
            [colour[0], colour[1], colour[2], 255]
        );
        assert_eq!(owner_sample(&atlas, sprite, 1.0, 0.0), [65, 145, 245, 255]);
        let averaged = sprite_sample(
            &atlas,
            sprite,
            sprite.uv,
            [1.0 / 2048.0, 0.5 / 2048.0],
            &[
                [-0.5 / 2048.0, 0.0],
                [-0.5 / 2048.0, 0.0],
                [0.5 / 2048.0, 0.0],
                [0.5 / 2048.0, 0.0],
            ],
            [0; 4],
        );
        assert_eq!(
            averaged,
            [
                ((u16::from(colour[0]) + 65).div_ceil(2)) as u8,
                ((u16::from(colour[1]) + 145).div_ceil(2)) as u8,
                ((u16::from(colour[2]) + 245).div_ceil(2)) as u8,
                255
            ]
        );
    }
}
