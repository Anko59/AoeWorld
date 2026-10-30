use super::*;
#[test]
fn unit_animation_fields_reject_truncated_or_unbounded_headers() {
    for n in 0..180 {
        assert!(parse(&vec![0; n], 0, 83).is_err());
    }
    assert!(find(b"wrong", &[83]).is_err());
    assert!(find(b"VER 8.9\0", &vec![83; 257]).is_err());
}

fn unit(id: i16) -> Vec<u8> {
    let mut b = vec![0; 180];
    b[0] = 40;
    b[1..3].copy_from_slice(&id.to_le_bytes());
    b[13..15].copy_from_slice(&101_i16.to_le_bytes());
    b[17..19].copy_from_slice(&102_i16.to_le_bytes());
    b[45..47].copy_from_slice(&(-1_i16).to_le_bytes());
    b.extend_from_slice(&[0x60, 0x0a, 4, 0]);
    b.extend_from_slice(b"TEST");
    b.extend_from_slice(&id.to_le_bytes());
    b.extend_from_slice(&id.to_le_bytes());
    b.extend_from_slice(&[0; 4]);
    b.extend_from_slice(&103_i16.to_le_bytes());
    b.extend_from_slice(&104_i16.to_le_bytes());
    b.extend_from_slice(&[0; 37 + 10 + 2 + 14 + 2]);
    b
}
#[test]
fn common_unit_fields_and_shared_task_indirection_are_preserved() {
    let mut b = b"VER 8.9\0".to_vec();
    b.extend(unit(1));
    // Header count and civilization slots intentionally differ.
    b.extend_from_slice(&2_u32.to_le_bytes());
    b.push(0);
    b.extend_from_slice(&[1, 1, 0]);
    let mut task = [0; 69];
    for at in [47, 51, 53] {
        task[at..at + 2].copy_from_slice(&(-1_i16).to_le_bytes());
    }
    task[49..51].copy_from_slice(&105_i16.to_le_bytes());
    task[53..55].copy_from_slice(&106_i16.to_le_bytes());
    b.extend(task);
    b.extend_from_slice(&[1, 0, 1, 0x60, 0x0a, 4, 0]);
    b.extend(b"Gaia");
    b.extend_from_slice(&601_u16.to_le_bytes());
    b.extend(vec![0; 4 + 601 * 4 + 1]);
    b.extend_from_slice(&3_u16.to_le_bytes());
    for _ in 0..3 {
        b.extend_from_slice(&1_i32.to_le_bytes());
    }
    let found = find(&b, &[1]).unwrap();
    let u = &found[&1];
    assert_eq!(
        (
            u.standing, u.dying, u.walking, u.running, u.work, u.carrying
        ),
        (101, 102, 103, 104, 105, 106)
    );
    b.truncate(b.len() - 1);
    assert_eq!(tasks::find(&b, 2).unwrap().len(), 0);
}
