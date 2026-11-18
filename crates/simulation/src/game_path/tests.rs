use super::segment_length;

fn exact(dx: i64, dy: i64) -> u32 {
    let squared = (dx.unsigned_abs() as u128).pow(2) + (dy.unsigned_abs() as u128).pow(2);
    let floor = squared.isqrt();
    u32::try_from(floor + u128::from(floor * floor != squared))
        .unwrap_or(u32::MAX)
        .max(1)
}

#[test]
fn segment_length_is_the_exact_rounded_up_root() {
    let edges = [
        0,
        1,
        2,
        3,
        1023,
        1024,
        1025,
        46_340,
        46_341,
        1 << 24,
        (1 << 31) - 1,
        1 << 31,
    ];
    for &x in &edges {
        for &y in &edges {
            for (dx, dy) in [(x, y), (-x, y), (x, -y)] {
                assert_eq!(segment_length(dx, dy), exact(dx, dy), "{dx} {dy}");
            }
        }
    }
    // Perfect squares and their neighbours across the whole map range.
    let mut n: i64 = 1;
    while n < 1 << 31 {
        for d in [-1, 0, 1] {
            assert_eq!(segment_length(n + d, 0), exact(n + d, 0));
            assert_eq!(segment_length(n + d, n), exact(n + d, n));
        }
        n = n * 3 / 2 + 1;
    }
    let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
    for _ in 0..100_000 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let (dx, dy) = ((seed >> 40) as i64 - (1 << 23), (seed & 0xff_ffff) as i64);
        assert_eq!(segment_length(dx, dy), exact(dx, dy), "{dx} {dy}");
    }
    assert_eq!(segment_length(i64::MAX, i64::MAX), u32::MAX);
}
