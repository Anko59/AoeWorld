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
    let texture = parse(&fixture(8, 4, 2)).expect("decodes");
    assert_eq!((texture.width, texture.height), (8, 4));
    assert!(texture.pixels.iter().all(|p| *p == [255, 0, 0, 255]));
    assert!(parse(&fixture(8, 4, 1)).is_err(), "truncated data");
    assert!(parse(&fixture(6, 4, 2)).is_err(), "unaligned");
    assert!(parse(&fixture(8192, 4, 0)).is_err(), "too wide");
    assert!(parse(&fixture(0, 4, 0)).is_err(), "empty");
    let mut other = fixture(4, 4, 1);
    other[84..88].copy_from_slice(b"DXT2");
    assert!(matches!(parse(&other), Err(Error::Unsupported { .. })));
    assert!(parse(&[0; 3]).is_err());
}

#[test]
fn dds_expands_all_rgb565_channels_with_bit_replication() {
    let mut bytes = fixture(4, 4, 1);
    bytes[128..132].copy_from_slice(&[0x51, 0x8C, 0, 0]);
    let texture = parse(&bytes).unwrap();
    assert!(
        texture
            .pixels
            .iter()
            .all(|pixel| *pixel == [140, 138, 140, 255])
    );
}

fn bc7_fixture() -> Vec<u8> {
    let mut bytes = fixture(4, 4, 0);
    bytes[84..88].copy_from_slice(b"DX10");
    for field in [98_u32, 3, 0, 1, 0] {
        bytes.extend(field.to_le_bytes());
    }
    let mut block = [0_u8; 16];
    let mut bit = 0;
    let mut push = |value: u32, count: usize| {
        for i in 0..count {
            block[bit / 8] |= (((value >> i) & 1) as u8) << (bit % 8);
            bit += 1;
        }
    };
    push(64, 7); // Mode 6, one subset, RGBA endpoints and four-bit indices.
    for value in [0, 0, 0, 0, 127, 127, 64, 64] {
        push(value, 7);
    }
    push(1, 1);
    push(1, 1);
    push(0, 3);
    for _ in 1..16 {
        push(0, 4);
    }
    assert_eq!(bit, 128);
    bytes.extend(block);
    bytes
}
#[test]
fn current_dds_decodes_bc7_with_unpremultiplied_rgba_and_bounds_every_prefix() {
    let file = bc7_fixture();
    let t = parse(&file).unwrap();
    assert_eq!((t.width, t.height), (4, 4));
    assert!(t.pixels.iter().all(|p| *p == [1, 1, 255, 129]));
    for end in 0..file.len() {
        assert!(parse(&file[..end]).is_err(), "prefix {end}");
    }
    let mut array = file.clone();
    array[140..144].copy_from_slice(&2_u32.to_le_bytes());
    assert!(
        parse(&array).is_err(),
        "a 2D cell decoder does not accept texture arrays"
    );
}
#[test]
fn current_dds_bc3_uses_four_colours_for_either_endpoint_order() {
    for (colors, bits, expected) in [
        ([0x00, 0xf8, 0x1f, 0x00], 0_u32, [255, 0, 0, 128]),
        ([0x1f, 0x00, 0x00, 0xf8], u32::MAX, [170, 0, 85, 128]),
    ] {
        let mut file = fixture(4, 4, 0);
        file[84..88].copy_from_slice(b"DXT5");
        file.extend([128, 128, 0, 0, 0, 0, 0, 0]);
        file.extend(colors);
        file.extend(bits.to_le_bytes());
        assert!(parse(&file).unwrap().pixels.iter().all(|p| *p == expected));
    }
}
#[test]
fn current_dds_uncompressed_mask_order_and_row_pitch_are_respected() {
    let mut file = fixture(3, 2, 0);
    file[84..88].fill(0);
    file[80..84].copy_from_slice(&0x41_u32.to_le_bytes());
    file[88..92].copy_from_slice(&32_u32.to_le_bytes());
    file[20..24].copy_from_slice(&16_u32.to_le_bytes());
    for (i, mask) in [0x00ff0000_u32, 0x0000ff00, 0x000000ff, 0xff000000]
        .into_iter()
        .enumerate()
    {
        file[92 + 4 * i..96 + 4 * i].copy_from_slice(&mask.to_le_bytes());
    }
    for _ in 0..2 {
        for _ in 0..3 {
            file.extend([77, 33, 11, 99]);
        }
        file.extend([0xff; 4]);
    }
    let t = parse(&file).unwrap();
    assert_eq!((t.width, t.height), (3, 2));
    assert_eq!(t.pixels, vec![[11, 33, 77, 99]; 6]);
    file.pop();
    assert!(parse(&file).is_err());
}
