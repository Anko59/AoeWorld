use super::*;

#[test]
fn frame_chunks_roundtrip_and_share_identical_pixel_payloads() {
    let directory = tempfile::tempdir().unwrap();
    let mut writer = Writer::new(directory.path());
    let planes = [vec![1, 2, 3, 255], vec![7, 0, 0, 119], vec![0, 0, 0, 77]];
    let first = writer
        .push([1, 1], planes.each_ref().map(Vec::as_slice))
        .unwrap();
    let alias = writer
        .push([1, 1], planes.each_ref().map(Vec::as_slice))
        .unwrap();
    assert_eq!(first, alias);
    let mut changed = planes.clone();
    changed[0][0] = 8;
    let second = writer
        .push([1, 1], changed.each_ref().map(Vec::as_slice))
        .unwrap();
    assert_ne!(first, second);
    let chunks = writer.finish().unwrap();
    assert_eq!(chunks.len(), 1);
    let bytes = std::fs::read(directory.path().join(&chunks[0].file)).unwrap();
    assert_eq!(chunks[0].hash, blake3::hash(&bytes).to_hex().as_str());
    assert_eq!(chunks[0].bytes as usize, bytes.len());
    assert_eq!(
        compressed::decode_frame(first.slice(&bytes).unwrap(), [1, 1]).unwrap(),
        planes
    );
    assert_eq!(
        compressed::decode_frame(second.slice(&bytes).unwrap(), [1, 1]).unwrap(),
        changed
    );
}

#[test]
fn independent_frame_chunks_split_without_losing_any_source_pixels() {
    let directory = tempfile::tempdir().unwrap();
    let mut writer = Writer::new(directory.path());
    let mut state = 0x1234_5678_u32;
    let mut frames = Vec::new();
    for _ in 0..7 {
        let planes: [Vec<u8>; 3] = std::array::from_fn(|_| {
            (0..128 * 128 * 4)
                .map(|_| {
                    state ^= state << 13;
                    state ^= state >> 17;
                    state ^= state << 5;
                    state as u8
                })
                .collect()
        });
        let location = writer
            .push([128, 128], planes.each_ref().map(Vec::as_slice))
            .unwrap();
        frames.push((location, planes));
    }
    let chunks = writer.finish().unwrap();
    assert!(chunks.len() > 1);
    assert!(
        chunks
            .iter()
            .all(|chunk| chunk.bytes as usize <= TARGET_CHUNK_BYTES)
    );
    for (location, planes) in frames {
        let chunk = &chunks[usize::from(location.chunk)];
        let bytes = std::fs::read(directory.path().join(&chunk.file)).unwrap();
        assert_eq!(
            compressed::decode_frame(location.slice(&bytes).unwrap(), [128, 128]).unwrap(),
            planes
        );
    }
}

#[test]
fn frame_chunk_references_reject_invalid_lengths_and_arithmetic_bounds() {
    let bytes = [1, 2, 3, 4];
    let valid = FrameLocation {
        chunk: 0,
        offset: 1,
        length: 3,
    };
    assert_eq!(valid.slice(&bytes).unwrap(), &[2, 3, 4]);
    for (offset, length) in [(0, 0), (u32::MAX, 1), (1, u32::MAX), (3, 2)] {
        assert!(
            FrameLocation {
                chunk: 0,
                offset,
                length
            }
            .slice(&bytes)
            .is_err()
        );
    }
}
