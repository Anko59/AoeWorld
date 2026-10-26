use aoe_core::clamp_f64;

#[test]
fn clamp_f64_matches_standard_bits_for_boundaries_signed_zero_and_nan_values() {
    let bounds = [
        (-f64::INFINITY, f64::INFINITY),
        (-10.0, 10.0),
        (0.0, 256.0),
        (-0.0, 0.0),
        (1.0, 1.0),
        (f64::MIN_POSITIVE, f64::MAX),
    ];
    let mut values = vec![
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NAN,
        -0.0,
        0.0,
        1.0,
        -1.0,
        f64::MIN_POSITIVE,
        f64::from_bits(1),
        f64::MAX,
        f64::from_bits(0x7ff0_0000_0000_0001),
        f64::from_bits(0xfff8_1234_5678_1234),
    ];
    let mut state = 0x1234_5678_9abc_def0_u64;
    for _ in 0..2048 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        values.push(f64::from_bits(state));
    }
    for (lo, hi) in bounds {
        for &value in &values {
            assert_eq!(
                clamp_f64(value, lo, hi).to_bits(),
                value.clamp(lo, hi).to_bits()
            );
        }
    }
}

#[test]
fn clamp_f64_still_rejects_reversed_or_nan_bounds() {
    for (lo, hi) in [
        (2.0, 1.0),
        (f64::NAN, 1.0),
        (0.0, f64::NAN),
        (f64::INFINITY, f64::NEG_INFINITY),
    ] {
        assert!(std::panic::catch_unwind(|| clamp_f64(0.0, lo, hi)).is_err());
    }
}
