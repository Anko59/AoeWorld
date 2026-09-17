#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    let _ = aoe_assets::pack::parse_manifest(bytes);
});
