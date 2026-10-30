use super::*;

#[test]
fn shared_projectile_filename_selects_the_requested_native_sprite() {
    let mut data = fixture();
    let sprite_at = 8 + 4 + "synthetic_x1".len() + 4;
    data[sprite_at..sprite_at + 4].copy_from_slice(&3986_i32.to_le_bytes());
    let mut second = fixture();
    second[sprite_at..sprite_at + 4].copy_from_slice(&4500_i32.to_le_bytes());
    data.extend_from_slice(&second[8..]);
    assert!(find(&data, "synthetic_x1").is_err());
    assert_eq!(
        find_sprite(&data, "synthetic_x1", 4500).unwrap().sprite,
        4500
    );
    assert!(find_sprite(&data, "synthetic_x1", 999).is_err());
    data.extend_from_slice(&second[8..]);
    assert!(find_sprite(&data, "synthetic_x1", 4500).is_err());
}
fn fixture() -> Vec<u8> {
    let mut b = b"VER 8.9\0".to_vec();
    for name in ["synthetic_x1", ""] {
        b.extend_from_slice(&[0x60, 0x0a]);
        b.extend_from_slice(&(name.len() as u16).to_le_bytes());
        b.extend_from_slice(name.as_bytes());
    }
    b.extend_from_slice(&999_i32.to_le_bytes());
    b.extend_from_slice(&[0; 23]);
    b.extend_from_slice(&45_u16.to_le_bytes());
    b.extend_from_slice(&16_u16.to_le_bytes());
    b.extend_from_slice(&0_f32.to_le_bytes());
    b.extend_from_slice(&0.0375_f32.to_le_bytes());
    b.extend_from_slice(&0_f32.to_le_bytes());
    b.extend_from_slice(&[0; 5]);
    b
}
#[test]
fn native_graphic_topology_and_fractional_timing_are_read_from_dat() {
    let b = fixture();
    let g = find(&b, "synthetic_x1").unwrap();
    assert_eq!((g.sprite, g.frames, g.angles), (999, 45, 16));
    assert_eq!(g.seconds, 0.0375);
    assert_eq!(g.replay_delay, 0.0);
    for end in 0..b.len() {
        assert!(find(&b[..end], "synthetic_x1").is_err(), "prefix {end}");
    }
    let mut duplicate = b.clone();
    duplicate.extend_from_slice(&b[8..]);
    assert!(find(&duplicate, "synthetic_x1").is_err());
    assert!(find(&b, "absent_x1").is_err());
}
#[test]
fn nonfinite_or_zero_animation_timing_is_rejected() {
    for value in [f32::NAN, f32::INFINITY, -1.0, 0.0] {
        let mut b = fixture();
        let at = b.len() - 13;
        b[at..at + 4].copy_from_slice(&value.to_le_bytes());
        assert!(find(&b, "synthetic_x1").is_err());
    }
}
