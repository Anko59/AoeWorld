use super::*;
use wasm_bindgen_test::wasm_bindgen_test;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[wasm_bindgen_test]
fn compiled_recipe_layout_and_all_truncation_boundaries_are_checked() {
    let mut bytes = vec![1];
    bytes.extend(7i32.to_le_bytes());
    for value in [2.0f32, 3.0] {
        bytes.extend(value.to_le_bytes());
    }
    bytes.extend(15004i32.to_le_bytes());
    for value in [10u16, 10, 0, 0] {
        bytes.extend(value.to_le_bytes());
    }
    bytes.extend(0u16.to_le_bytes()); // No shared damage graphics.
    bytes.push(0); // No damage states.
    let buildings = decode(&hex(&bytes), &[]).unwrap();
    assert_eq!(buildings.len(), 1);
    assert_eq!(buildings[0].unit_id, 7);
    assert_eq!(buildings[0].footprint, [2.0, 3.0]);
    assert_eq!(buildings[0].foundation_slp, Some(15004));
    assert_eq!(buildings[0].foundation_size, [10, 10]);
    for len in 0..bytes.len() {
        assert!(decode(&hex(&bytes[..len]), &[]).is_err());
    }
    for invalid in ["0", "xx", "00ff", "21"] {
        assert!(decode(invalid, &[]).is_err());
    }
    assert!(decode("00", &[]).unwrap().is_empty());
    bytes.truncate(21);
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(999u32.to_le_bytes());
    assert!(
        decode(&hex(&bytes), &[])
            .err()
            .unwrap()
            .contains("frame index")
    );
}

#[wasm_bindgen_test]
fn damage_instances_share_animation_and_keep_each_offset() {
    let mut bytes = vec![1];
    bytes.extend(7i32.to_le_bytes());
    for value in [2.0f32, 2.0] {
        bytes.extend(value.to_le_bytes());
    }
    bytes.extend((-1i32).to_le_bytes());
    for value in [0u16, 0, 0, 0, 1] {
        bytes.extend(value.to_le_bytes());
    }
    bytes.extend(u32::MAX.to_le_bytes());
    bytes.extend(20u16.to_le_bytes());
    bytes.extend(1u16.to_le_bytes());
    bytes.push(0);
    bytes.extend(0.05f32.to_le_bytes());
    for value in [0i32, 0] {
        bytes.extend(value.to_le_bytes());
    }
    for value in [0.0f32, 0.0] {
        bytes.extend(value.to_le_bytes());
    }
    bytes.push(1);
    bytes.extend(25i16.to_le_bytes());
    bytes.push(0);
    bytes.extend(2u16.to_le_bytes());
    for [x, y] in [[-12i32, 5], [17, -9]] {
        bytes.extend(0u16.to_le_bytes());
        bytes.extend(x.to_le_bytes());
        bytes.extend(y.to_le_bytes());
    }
    let buildings = decode(&hex(&bytes), &[]).unwrap();
    let stage = &buildings[0].damage[0];
    assert_eq!(stage.damage_percent, 25);
    assert_eq!(
        stage.parts.iter().map(|p| p.offset).collect::<Vec<_>>(),
        [[-12, 5], [17, -9]]
    );
    assert!(
        stage
            .parts
            .iter()
            .all(|p| p.frames == 20 && p.frame_rate == 0.05)
    );
    for len in 0..bytes.len() {
        assert!(decode(&hex(&bytes[..len]), &[]).is_err());
    }
    let first_index = bytes.len() - 20;
    bytes[first_index] = 1;
    assert!(decode(&hex(&bytes), &[]).is_err());
}
