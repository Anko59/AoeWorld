#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    // Feed the parser the fuzz bytes as the hex field itself so malformed
    // ASCII, odd-length input, and non-UTF-8 byte sequences reach decode_hex.
    let payload_hex = String::from_utf8_lossy(bytes).into_owned();
    let encoded = aoe_map::CompactChunk {
        x: 0,
        y: 0,
        payload_hex,
    };
    if let Ok(chunk) = encoded.decode() {
        assert!(
            aoe_map::CompactChunk::encode(&chunk)
                .and_then(|value| value.decode())
                .is_ok_and(|roundtrip| roundtrip == chunk)
        );
    }
    // New schema-aware caller must retain every explicit scene field. Legacy
    // projections may contain coordinates forbidden by v3, so only v3 inputs
    // require a new-format roundtrip; all formats still reach the new reader.
    let landscape = encoded.decode_landscape();
    if encoded.payload_hex.starts_with("03")
        && let Ok(chunk) = landscape
    {
        assert!(
            aoe_map::CompactChunk::encode_landscape(&chunk)
                .and_then(|value| value.decode_landscape())
                .is_ok_and(|roundtrip| roundtrip == chunk)
        );
    }
});
