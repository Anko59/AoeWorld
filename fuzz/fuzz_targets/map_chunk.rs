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
});
