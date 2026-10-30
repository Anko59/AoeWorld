use super::*;

#[test]
fn batch_lookup_preserves_requested_order_and_rejects_conflicting_aliases() {
    let mut bytes = fixture();
    let sprite_at = 8 + 4 + "synthetic_x1".len() + 4;
    let mut other = fixture();
    other[sprite_at..sprite_at + 4].copy_from_slice(&1000_i32.to_le_bytes());
    bytes.extend_from_slice(&other[8..]);
    bytes.extend_from_slice(&fixture()[8..]);
    let requested = [("synthetic_x1", 1000), ("synthetic_x1", 999)];
    let found = find_sprites(&bytes, &requested).unwrap();
    assert_eq!(
        found.iter().map(|g| g.sprite).collect::<Vec<_>>(),
        [1000, 999]
    );
    assert_eq!(found[1], find_sprite(&bytes, "synthetic_x1", 999).unwrap());
    assert!(find_sprites(&bytes, &[("synthetic_x1", 1001)]).is_err());
    let mut conflict = fixture();
    let at = conflict.len() - 13;
    conflict[at..at + 4].copy_from_slice(&0.05_f32.to_le_bytes());
    bytes.extend_from_slice(&conflict[8..]);
    assert!(find_sprites(&bytes, &requested).is_err());
}

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
    let frame_at = second.len() - 21;
    second[frame_at..frame_at + 2].copy_from_slice(&44_u16.to_le_bytes());
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

#[test]
fn matching_native_sprite_aliases_have_one_consistent_timeline() {
    let mut bytes = fixture();
    bytes.extend_from_slice(&fixture()[8..]);
    let graphic = find_sprite(&bytes, "synthetic_x1", 999).expect("identical DAT alias");
    assert_eq!((graphic.frames, graphic.angles), (45, 16));
    // A conflicting descriptor of that same identity remains ambiguous.
    let mut conflict = fixture();
    let at = conflict.len() - 13;
    conflict[at..at + 4].copy_from_slice(&0.05_f32.to_le_bytes());
    bytes.extend_from_slice(&conflict[8..]);
    assert!(find_sprite(&bytes, "synthetic_x1", 999).is_err());
}

#[test]
fn a_single_native_static_cell_can_have_no_animation_duration() {
    let mut bytes = fixture();
    let count = bytes.len() - 21;
    bytes[count..count + 2].copy_from_slice(&1_u16.to_le_bytes());
    let time = bytes.len() - 13;
    bytes[time..time + 4].copy_from_slice(&0_f32.to_le_bytes());
    bytes[time + 4..time + 8].copy_from_slice(&1_f32.to_le_bytes());
    let graphic = find_sprite(&bytes, "synthetic_x1", 999).expect("static native pose");
    assert_eq!(
        (graphic.frames, graphic.seconds, graphic.replay_delay),
        (1, 0.0, 1.0)
    );
}

fn indexed_record(id: i16, file: &str, layer: u8, children: &[(i16, i16, i16)]) -> Vec<u8> {
    let mut b = Vec::new();
    for text in [file, ""] {
        b.extend([0x60, 0x0a]);
        b.extend((text.len() as u16).to_le_bytes());
        b.extend(text.as_bytes());
    }
    b.extend(i32::from(id).to_le_bytes());
    b.extend([0, 0, layer]);
    b.extend([0; 11]);
    b.extend((children.len() as u16).to_le_bytes());
    b.extend([0; 7]);
    b.extend(30_u16.to_le_bytes());
    b.extend(16_u16.to_le_bytes());
    b.extend(0_f32.to_le_bytes());
    b.extend(0.025_f32.to_le_bytes());
    b.extend(0_f32.to_le_bytes());
    b.push(0);
    b.extend(id.to_le_bytes());
    b.extend([0; 2]);
    for &(child, x, y) in children {
        b.extend(child.to_le_bytes());
        b.extend([0; 6]);
        b.extend(x.to_le_bytes());
        b.extend(y.to_le_bytes());
        b.extend((-1_i16).to_le_bytes());
        b.extend([0; 2]);
    }
    b
}
#[test]
fn graphic_delta_trees_preserve_every_drawable_layer_offset_and_timeline() {
    let mut b = b"VER 8.9\0".to_vec();
    b.extend(indexed_record(10, "W", 0, &[(11, 0, -13), (12, 4, 5)]));
    b.extend(indexed_record(11, "sail_x1", 20, &[]));
    b.extend(indexed_record(12, "hull_x1", 10, &[]));
    let all = indexed(&b).unwrap();
    let g = &all[&10];
    assert_eq!(g.parts.len(), 2);
    assert_eq!(
        (&*g.parts[0].animation.file, g.parts[0].x, g.parts[0].y),
        ("hull_x1", 4, 5)
    );
    assert_eq!(
        (&*g.parts[1].animation.file, g.parts[1].x, g.parts[1].y),
        ("sail_x1", 0, -13)
    );
    assert_eq!(
        (
            g.parts[1].animation.frames,
            g.parts[1].animation.angles,
            g.parts[1].animation.seconds
        ),
        (30, 16, 0.025)
    );
    let mut cyclic = b"VER 8.9\0".to_vec();
    cyclic.extend(indexed_record(13, "", 0, &[(13, 0, 0)]));
    assert!(!indexed(&cyclic).unwrap().contains_key(&13));
}

#[test]
fn missing_delta_references_are_rejected_instead_of_dropping_drawable_parts() {
    let mut b = b"VER 8.9\0".to_vec();
    b.extend(indexed_record(20, "body_x1", 10, &[(21, 0, 0)]));
    assert!(!indexed(&b).unwrap().contains_key(&20));
    b.extend(indexed_record(21, "None", 0, &[]));
    assert_eq!(indexed(&b).unwrap()[&20].parts.len(), 1);
}

#[test]
fn de_buildings_use_named_sld_layers_even_without_classic_sprite_ids() {
    let mut b = b"VER 8.9\0".to_vec();
    let mut record = indexed_record(22, "building_back_x1", 10, &[]);
    let at = 4 + "building_back_x1".len() + 4;
    record[at..at + 4].copy_from_slice(&(-1_i32).to_le_bytes());
    b.extend(record);
    let all = indexed(&b).unwrap();
    assert_eq!(all[&22].parts[0].animation.file, "building_back_x1");
}
