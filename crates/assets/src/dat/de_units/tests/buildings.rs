use super::*;
fn building() -> Vec<u8> {
    let id = 109_i16;
    let mut b = vec![0; 158];
    b[0] = 80;
    b[1..3].copy_from_slice(&id.to_le_bytes());
    b[13..15].copy_from_slice(&100_i16.to_le_bytes());
    b[17..19].copy_from_slice(&101_i16.to_le_bytes());
    b[45..47].copy_from_slice(&(-1_i16).to_le_bytes());
    b[29..33].copy_from_slice(&1.5_f32.to_le_bytes());
    b[33..37].copy_from_slice(&2_f32.to_le_bytes());
    b[157] = 1;
    b.extend(110_i16.to_le_bytes());
    b.extend(50_i16.to_le_bytes());
    b.push(1);
    b.extend([0; 22]);
    b.extend([0x60, 0x0a, 4, 0]);
    b.extend(b"TEST");
    b.extend(id.to_le_bytes());
    b.extend(id.to_le_bytes());
    b.extend([0; 4 + 41 + 10 + 2 + 14 + 2 + 2 + 2 + 2 + 6 + 12 + 2 + 3 + 2 + 13 + 4 + 4]);
    b.extend((-1_i16).to_le_bytes());
    b.extend([0; 32 + 18]);
    b.extend(1_u16.to_le_bytes());
    b.extend([0; 9 + 97]);
    for id in [200_i16, -1, 201, 202, -1, -1] {
        b.extend(id.to_le_bytes());
    }
    b.push(0);
    b.extend(0_i16.to_le_bytes());
    b.push(0);
    for value in [-1_i16, 27, -1, -1] {
        b.extend(value.to_le_bytes());
    }
    b.push(0);
    b.extend(110_i16.to_le_bytes());
    b.extend(1.0_f32.to_le_bytes());
    b.extend((-1.0_f32).to_le_bytes());
    for _ in 0..3 {
        b.extend((-1_i16).to_le_bytes());
        b.extend([0; 8]);
    }
    b.extend([0; 33]);
    b
}
#[test]
fn current_de_building_tail_preserves_every_graphic_role_annex_and_damage_state() {
    let b = building();
    let u = parse(&b, 0, 109).unwrap();
    let native = u.building.unwrap();
    assert_eq!(u.footprint, [3.0, 4.0]);
    assert_eq!(
        (
            native.construction,
            native.destruction,
            native.rubble,
            native.foundation
        ),
        (200, 201, 202, 27)
    );
    assert_eq!(native.annexes.len(), 1);
    assert_eq!(
        (native.annexes[0].unit, native.annexes[0].offset),
        (110, [1.0, -1.0])
    );
    assert_eq!(
        (u.damage[0].graphic, u.damage[0].percent, u.damage[0].mode),
        (110, 50, 1)
    );
    for end in b.len() - 98..b.len() {
        assert!(parse(&b[..end], 0, 109).is_err(), "tail prefix {end}");
    }
}
