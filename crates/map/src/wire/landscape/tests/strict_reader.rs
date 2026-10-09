use super::*;

#[test]
fn strict_reader_rejects_legacy_and_matches_general_reader_for_three() {
    let encoded = CompactChunk::encode_landscape(&fixture(8)).expect("encode");
    assert_eq!(
        encoded.decode_landscape_v3().expect("strict"),
        encoded.decode_landscape().expect("general")
    );
    for payload_hex in ["0100000000", "0200000000"] {
        let compact = CompactChunk {
            x: 0,
            y: 0,
            payload_hex: payload_hex.into(),
        };
        assert!(compact.decode_landscape().is_ok());
        assert_eq!(
            compact.decode_landscape_v3(),
            Err(CompactChunkError::UnsupportedVersion.into())
        );
    }
}

#[test]
fn direct_legacy_routing_preserves_original_parser_failure_precedence() {
    for payload_hex in [
        "01",
        "02",
        "01g0",
        "02ff",
        "01f",
        "0201000000",
        "0100000100",
    ] {
        let compact = CompactChunk {
            x: i32::MAX,
            y: i32::MIN,
            payload_hex: payload_hex.into(),
        };
        let original = compact.decode().expect_err("malformed old payload");
        assert_eq!(compact.decode_landscape(), Err(original.into()));
    }
}
