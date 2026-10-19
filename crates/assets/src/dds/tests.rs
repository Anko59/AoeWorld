use super::*;

fn fixture(width: u32, height: u32, blocks: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; HEADER];
    bytes[..4].copy_from_slice(b"DDS ");
    bytes[4..8].copy_from_slice(&124u32.to_le_bytes());
    bytes[12..16].copy_from_slice(&height.to_le_bytes());
    bytes[16..20].copy_from_slice(&width.to_le_bytes());
    bytes[84..88].copy_from_slice(b"DXT1");
    for _ in 0..blocks {
        bytes.extend_from_slice(&[0x00, 0xF8, 0x1F, 0x00, 0, 0, 0, 0]);
    }
    bytes
}

#[test]
fn decodes_blocks_and_rejects_bad_headers() {
    let texture = parse_bc1(&fixture(8, 4, 2)).expect("decodes");
    assert_eq!((texture.width, texture.height), (8, 4));
    assert!(texture.pixels.iter().all(|p| *p == [255, 0, 0, 255]));
    assert!(parse_bc1(&fixture(8, 4, 1)).is_err(), "truncated data");
    assert!(parse_bc1(&fixture(6, 4, 2)).is_err(), "unaligned");
    assert!(parse_bc1(&fixture(8192, 4, 0)).is_err(), "too wide");
    assert!(parse_bc1(&fixture(0, 4, 0)).is_err(), "empty");
    let mut other = fixture(4, 4, 1);
    other[84..88].copy_from_slice(b"DXT5");
    assert!(matches!(parse_bc1(&other), Err(Error::Unsupported { .. })));
    assert!(parse_bc1(&[0; 3]).is_err());
}

#[test]
fn dds_expands_all_rgb565_channels_with_bit_replication() {
    let mut bytes = fixture(4, 4, 1);
    bytes[128..132].copy_from_slice(&[0x51, 0x8C, 0, 0]);
    let texture = parse_bc1(&bytes).unwrap();
    assert!(
        texture
            .pixels
            .iter()
            .all(|pixel| *pixel == [140, 138, 140, 255])
    );
}
