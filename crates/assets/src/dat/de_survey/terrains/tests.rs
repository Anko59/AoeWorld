use super::*;
fn fixture() -> Vec<u8> {
    let mut b = b"VER 8.9\0".to_vec();
    for _ in 0..200 {
        b.extend([1, 0, 0, 0, 0, 0, 0, 0]);
        for s in ["Ground", "g_grs"] {
            b.extend(0xa60_u16.to_le_bytes());
            b.extend((s.len() as u16).to_le_bytes());
            b.extend(s.as_bytes());
        }
        b.extend([0; 28]);
        b.extend([0x60, 0xa, 9, 0]);
        b.extend(b"grass.png");
        b.extend([0; 8 + 22 + 19 * 6]);
        b.extend((-1_i16).to_le_bytes());
        b.extend([10, 0, 10, 0]);
        b.extend([0; 30 * 7 + 4]);
    }
    b
}
#[test]
fn one_current_terrain_chain_is_required_and_names_are_safe() {
    let mut b = fixture();
    let t = find(&b).unwrap();
    assert_eq!(t.len(), 200);
    assert_eq!(t[199].texture, "g_grs");
    b.pop();
    assert!(find(&b).is_err());
    let mut b = fixture();
    let at = b.windows(5).position(|s| s == b"g_grs").unwrap();
    b[at] = b'/';
    assert!(find(&b).is_err());
}
