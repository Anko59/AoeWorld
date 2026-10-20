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
