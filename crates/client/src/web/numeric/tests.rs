use super::format;
use wasm_bindgen_test::wasm_bindgen_test;

fn check(value: f64, digits: u32) {
    let precision = digits as usize;
    assert_eq!(
        format(value, digits),
        format!("{value:.precision$}"),
        "bits {:016x}, precision {digits}",
        value.to_bits(),
    );
}

#[wasm_bindgen_test]
fn exact_fixed_matches_rust_at_ieee_and_rounding_boundaries() {
    let values = [
        0.0,
        -0.0,
        f64::from_bits(1),
        -f64::from_bits(1),
        f64::from_bits(0x000f_ffff_ffff_ffff),
        f64::MIN_POSITIVE,
        -f64::MIN_POSITIVE,
        f64::EPSILON,
        f64::MAX,
        f64::MIN,
        0.05,
        -0.05,
        0.15,
        -0.15,
        0.25,
        -0.25,
        0.75,
        -0.75,
        1.005,
        -1.005,
        2.675,
        -2.675,
        1.25,
        -1.25,
        2.5,
        -2.5,
        3.5,
        -3.5,
        0.0005,
        -0.0005,
        0.9995,
        -0.9995,
        f64::from_bits(1.0_f64.to_bits() - 1),
        1.0,
        f64::from_bits(1.0_f64.to_bits() + 1),
        70_000.0,
        -70_000.0,
        2_097_152.5,
        -2_097_152.5,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
        f64::from_bits(0xfff8_0000_0000_0001),
        f64::from_bits(0x7ff0_0000_0000_0001),
    ];
    for value in values {
        for digits in 0..=3 {
            check(value, digits);
        }
    }
    check(-0.0, 20);
    check(f64::MAX, 20);
}

#[wasm_bindgen_test]
fn exact_fixed_matches_rust_for_seeded_ieee_bit_patterns() {
    let mut bits = 0x43a1_9876_2b5d_0ef1_u64;
    for _ in 0..512 {
        bits = bits
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        for digits in 0..=3 {
            check(f64::from_bits(bits), digits);
        }
    }
}

#[wasm_bindgen_test]
fn exact_fixed_preserves_all_four_ui_readout_inputs() {
    for centimeters in [i32::MIN, -70_001, -101, -1, 0, 1, 101, 70_001, i32::MAX] {
        check(f64::from(centimeters) / 100.0, 2);
    }
    for bytes in [0_u32, 1, 52_428, 52_429, 524_288, 1_048_576, u32::MAX] {
        check(f64::from(bytes) / (1024.0 * 1024.0), 1);
    }
    for zoom in [0.0_f32, -0.0, 0.1, 0.15, 1.005, 2.675, 8.0, f32::MAX] {
        for digits in 0..=3 {
            let precision = digits as usize;
            assert_eq!(
                format(f64::from(zoom), digits),
                format!("{zoom:.precision$}")
            );
        }
    }
}
