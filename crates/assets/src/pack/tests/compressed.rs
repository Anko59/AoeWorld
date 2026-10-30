use super::*;

#[test]
fn lossless_frame_codec_preserves_colour_owner_weight_and_shadow_alpha() {
    let planes = [
        vec![120, 100, 80, 128, 30, 40, 50, 255],
        vec![7, 0, 0, 119, 2, 0, 0, 255],
        vec![0, 0, 0, 32, 0, 0, 0, 127],
    ];
    let slices = planes.each_ref().map(Vec::as_slice);
    let bytes = encode_frame([2, 1], slices).unwrap();
    assert_eq!(decode_frame(&bytes, [2, 1]).unwrap(), planes);
    assert!(decode_frame(&bytes, [1, 2]).is_err());
    assert_eq!(encode_frame([2, 1], slices).unwrap(), bytes);
}

#[test]
fn compressed_frames_reject_invalid_shape_truncation_and_unused_payload() {
    let plane = [10, 20, 30, 128];
    for size in [[0, 1], [1, 0], [2045, 1], [1, 2045], [2, 1]] {
        assert!(encode_frame(size, [&plane; 3]).is_err());
    }
    let bytes = encode_frame([1, 1], [&plane; 3]).unwrap();
    for length in 0..bytes.len() {
        assert!(
            decode_frame(&bytes[..length], [1, 1]).is_err(),
            "prefix {length}"
        );
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(decode_frame(&trailing, [1, 1]).is_err());
    assert!(decode_frame(&bytes, [2, 1]).is_err());
    assert!(decode_frame(&bytes, [0, 1]).is_err());
    assert!(decode_frame(&[0x78, 0x01, 0xff], [1, 1]).is_err());
}

#[test]
fn compressed_frames_bound_decoding_to_the_declared_pixel_count() {
    use std::io::Write;
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(&[1, 0, 1, 0]).unwrap();
    for _ in 0..256 {
        encoder.write_all(&[0; 4096]).unwrap();
    }
    let bytes = encoder.finish().unwrap();
    assert!(decode_frame(&bytes, [1, 1]).is_err());
}
