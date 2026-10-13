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
