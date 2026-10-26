use super::clamp_f64;

#[test]
fn clamp_matches_ieee_values_signed_zero_and_nan() {
    let values = [
        f64::NEG_INFINITY,
        -30.0,
        -0.0,
        0.0,
        f64::from_bits(1),
        9.0,
        30.0,
        f64::INFINITY,
        f64::NAN,
    ];
    for (min, max) in [
        (f64::NEG_INFINITY, f64::INFINITY),
        (-10.0, 17.0),
        (-0.0, 0.0),
        (0.0, -0.0),
        (1.0, 1.0),
    ] {
        for value in values {
            let expected = value.clamp(min, max);
            let actual = clamp_f64(value, min, max);
            if expected.is_nan() {
                assert!(actual.is_nan());
            } else {
                assert_eq!(actual.to_bits(), expected.to_bits());
            }
        }
    }
}

#[test]
fn clamp_rejects_reversed_or_nan_bounds_like_the_standard_operation() {
    for (min, max) in [
        (1.0, -1.0),
        (f64::NAN, 1.0),
        (0.0, f64::NAN),
        (f64::NAN, f64::NAN),
    ] {
        assert!(std::panic::catch_unwind(|| 0.0_f64.clamp(min, max)).is_err());
        assert!(std::panic::catch_unwind(|| clamp_f64(0.0, min, max)).is_err());
    }
}
