#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut payload_hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        payload_hex.push(char::from(HEX[usize::from(byte >> 4)]));
        payload_hex.push(char::from(HEX[usize::from(byte & 15)]));
    }
    let encoded = aoe_map::CompactChunk { x: 0, y: 0, payload_hex };
    if let Ok(chunk) = encoded.decode() {
        assert!(aoe_map::CompactChunk::encode(&chunk)
            .and_then(|value| value.decode())
            .is_ok_and(|roundtrip| roundtrip == chunk));
    }
});
