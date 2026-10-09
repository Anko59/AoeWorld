use super::*;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn shared_alpha_kernel_matches_independent_integer_gold_and_keeps_center_coverage() {
    for seed in 0..64_u32 {
        let taps = std::array::from_fn::<_, 4, _>(|i| {
            let i = i as u32;
            [
                ((seed * 31 + i * 17) % 256) as u8,
                ((seed * 11 + i * 53) % 256) as u8,
                ((seed * 7 + i * 97) % 256) as u8,
                if seed == 0 {
                    0
                } else {
                    [0, 1, 128, 255][i as usize]
                },
            ]
        });
        for alpha in [0, 127, 255] {
            let center = [7, 89, 211, alpha];
            let weight = taps.iter().map(|tap| u64::from(tap[3])).sum::<u64>();
            let mut expected = center;
            if weight != 0 {
                for channel in 0..3 {
                    let numerator = taps
                        .iter()
                        .map(|tap| u64::from(tap[channel]) * u64::from(tap[3]))
                        .sum::<u64>();
                    expected[channel] = ((numerator + weight / 2) / weight) as u8;
                }
            }
            let mut sums = [0; 4];
            for tap in taps {
                add_tap(&mut sums, tap);
            }
            assert_eq!(finish(center, sums), expected);
            assert_eq!(finish(center, sums)[3], alpha);
        }
    }
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
