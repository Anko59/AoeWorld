use super::*;

const RED_BLUE: [u8; 8] = [0x00, 0xF8, 0x1F, 0x00, 0x01, 0, 0, 0]; // pixel 0 uses colour 1
const SHADOW: [u8; 8] = [200, 0, 0x01, 0, 0, 0, 0, 0]; // pixel 0 is 0, the rest 200

fn pad(file: &mut Vec<u8>) {
    while !(file.len() - HEADER).is_multiple_of(4) {
        file.push(0);
    }
}

fn layer_bytes(rect: Option<[u16; 4]>, flag: u8, cmds: &[(u8, u8)], blocks: &[[u8; 8]]) -> Vec<u8> {
    let mut body = Vec::new();
    if let Some(rect) = rect {
        for value in rect {
            body.extend_from_slice(&value.to_le_bytes());
        }
    }
    body.extend_from_slice(&[flag, 0]);
    body.extend_from_slice(&(cmds.len() as u16).to_le_bytes());
    for (skip, draw) in cmds {
        body.extend_from_slice(&[*skip, *draw]);
    }
    for block in blocks {
        body.extend_from_slice(block);
    }
    let mut out = ((body.len() + 4) as u32).to_le_bytes().to_vec();
    out.extend(body);
    out
}

fn header(frames: u16) -> Vec<u8> {
    let mut file = b"SLDX".to_vec();
    file.extend_from_slice(&[4, 0]);
    file.extend_from_slice(&frames.to_le_bytes());
    file.extend_from_slice(&[0, 0, 16, 0, 0xFF, 0, 0, 0]);
    file
}

fn frame_header(file: &mut Vec<u8>, size: u16, flags: u8, index: u16) {
    for value in [size, size, 2, 3] {
        file.extend_from_slice(&value.to_le_bytes());
    }
    file.push(flags);
    file.push(0);
    file.extend_from_slice(&index.to_le_bytes());
}

/// Two 8x8 frames; each has a 4x4 main block at (4,4) and a shadow block, the
/// second frame reuses both from the first and adds a player block.
fn fixture() -> Vec<u8> {
    let mut file = header(2);
    frame_header(&mut file, 8, 0b0_0011, 0);
    for (flag, blocks) in [(0u8, &[RED_BLUE]), (0, &[SHADOW])] {
        file.extend(layer_bytes(Some([4, 4, 8, 8]), flag, &[(0, 1)], blocks));
        pad(&mut file);
    }
    frame_header(&mut file, 8, 0b1_0011, 1);
    for _ in 0..2 {
        file.extend(layer_bytes(Some([4, 4, 8, 8]), REUSE_FLAG, &[(1, 0)], &[]));
        pad(&mut file);
    }
    file.extend(layer_bytes(
        None,
        0,
        &[(0, 1)],
        &[[255, 0, 0, 0, 0, 0, 0, 0]],
    ));
    pad(&mut file);
    file
}

#[test]
fn decodes_blocks_layers_anchors_and_skip_reuse() {
    let frames = parse(&fixture()).expect("fixture decodes");
    assert_eq!(frames.len(), 2);
    let first = &frames[0];
    assert_eq!((first.width, first.anchor_x, first.anchor_y), (8, 2, 3));
    let main = first.main.as_ref().expect("main");
    assert_eq!((main.x, main.y, main.width, main.height), (4, 4, 4, 4));
    assert_eq!(main.pixels[0], [0, 0, 248, 255]);
    assert_eq!(main.pixels[1], [248, 0, 0, 255]);
    let shadow = first.shadow.as_ref().expect("shadow");
    assert_eq!((shadow.pixels[0][0], shadow.pixels[1][0]), (0, 200));
    assert!(first.player.is_none());

    let second = &frames[1];
    assert_eq!(second.main, first.main, "skipped blocks come from frame 0");
    assert_eq!(second.shadow, first.shadow);
    let player = second.player.as_ref().expect("player");
    assert_eq!((player.x, player.width), (4, 4));
    assert_eq!(player.pixels[0][0], 255);
}

#[test]
fn skip_without_reuse_flag_stays_transparent() {
    let mut file = header(1);
    frame_header(&mut file, 8, 1, 0);
    file.extend(layer_bytes(Some([0, 0, 4, 4]), 0, &[(1, 0)], &[]));
    pad(&mut file);
    let frames = parse(&file).expect("decodes");
    assert!(
        frames[0]
            .main
            .as_ref()
            .unwrap()
            .pixels
            .iter()
            .all(|p| *p == [0; 4])
    );
}

#[test]
fn bc4_uses_the_six_interpolant_mode_when_endpoints_are_ordered_low_to_high() {
    let values = bc4_block([10, 200, 0b1011_0000 | 6, 0, 0, 0, 0, 0]);
    assert_eq!(values[0], 0, "index 6 is zero");
    let high = bc4_block([0, 255, 7, 0, 0, 0, 0, 0]);
    assert_eq!(high[0], 255, "index 7 is 255");
}

#[test]
fn every_truncation_is_an_error_not_a_panic() {
    let file = fixture();
    for length in 0..file.len() {
        assert!(parse(&file[..length]).is_err(), "prefix {length}");
    }
    let mut trailing = file;
    trailing.push(0);
    assert!(parse(&trailing).is_err());
}

#[test]
fn counts_sizes_and_offsets_are_bounded() {
    let mut bad = fixture();
    bad[..4].copy_from_slice(b"SLDY");
    assert!(parse(&bad).is_err());
    let mut bad = fixture();
    bad[4] = 3;
    assert!(matches!(parse(&bad), Err(Error::Unsupported { .. })));
    let mut bad = fixture();
    bad[6..8].copy_from_slice(&u16::MAX.to_le_bytes());
    assert!(parse(&bad).is_err(), "frame count above the limit");
    let mut bad = fixture();
    bad[10] = 17;
    assert!(parse(&bad).is_err(), "header size");
    let mut bad = fixture();
    bad[16..18].copy_from_slice(&5000u16.to_le_bytes());
    assert!(parse(&bad).is_err(), "frame side above the limit");
    // Layer rectangle that is inverted, unaligned, or beyond the canvas.
    for rect in [[8u16, 4, 4, 8], [4, 4, 7, 8], [6, 4, 10, 8]] {
        let mut file = header(1);
        frame_header(&mut file, 8, 1, 0);
        file.extend(layer_bytes(Some(rect), 0, &[(0, 0)], &[]));
        pad(&mut file);
        assert!(parse(&file).is_err(), "{rect:?}");
    }
    // A command that draws past the last block, and a layer length past the file.
    let mut file = header(1);
    frame_header(&mut file, 8, 1, 0);
    file.extend(layer_bytes(
        Some([0, 0, 4, 4]),
        0,
        &[(0, 2)],
        &[RED_BLUE; 2],
    ));
    pad(&mut file);
    assert!(parse(&file).is_err());
    let mut file = header(1);
    frame_header(&mut file, 8, 1, 0);
    file.extend_from_slice(&u32::MAX.to_le_bytes());
    file.extend_from_slice(&[0; 16]);
    assert!(parse(&file).is_err());
}

#[test]
fn total_decoded_pixels_are_budgeted() {
    // Each frame is a fully covered 2048x2048 main layer of skips; the shared
    // budget must stop the file before every frame is held in memory.
    let mut cmds: Vec<(u8, u8)> = vec![(255, 0); 512 * 512 / 255];
    cmds.push(((512 * 512 % 255) as u8, 0));
    let mut file = header(40);
    for index in 0..40 {
        frame_header(&mut file, 2048, 1, index);
        file.extend(layer_bytes(Some([0, 0, 2048, 2048]), 0, &cmds, &[]));
        pad(&mut file);
    }
    let error = parse(&file).expect_err("budget rejects");
    assert!(error.to_string().contains("budget"), "{error}");
}
