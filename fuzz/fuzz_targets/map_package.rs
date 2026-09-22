#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    if let Ok(request) = serde_json::from_slice::<aoe_map::MapRequest>(bytes) {
        let _ = request.normalized();
        let _ = request.estimate();
    }
    if let Ok(package) = serde_json::from_slice::<aoe_map::MapPackage>(bytes)
        && package.validate().is_ok() {
        assert!(serde_json::to_vec(&package).ok()
            .and_then(|bytes| serde_json::from_slice::<aoe_map::MapPackage>(&bytes).ok())
            .is_some_and(|value| value == package && value.validate().is_ok()));
    }
});
