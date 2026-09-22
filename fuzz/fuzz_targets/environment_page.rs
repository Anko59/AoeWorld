#![no_main]
use libfuzzer_sys::fuzz_target;

macro_rules! check_page {
    ($bytes:expr, $kind:ty) => {
        if let Ok(page) = serde_json::from_slice::<$kind>($bytes) {
            let valid = page.validate().is_ok();
            assert_eq!(page.content_hash().is_ok(), valid);
            if valid {
                assert!(serde_json::to_vec(&page).ok()
                    .and_then(|bytes| serde_json::from_slice::<$kind>(&bytes).ok())
                    .is_some_and(|value| value == page && value.content_hash() == page.content_hash()));
            }
        }
    };
}

fuzz_target!(|bytes: &[u8]| {
    check_page!(bytes, aoe_map::ElevationPage);
    check_page!(bytes, aoe_map::WaterPage);
    check_page!(bytes, aoe_map::PotentialBiomePage);
    check_page!(bytes, aoe_map::HistoricalLandUsePage);
    check_page!(bytes, aoe_map::HydrologyEvidencePage);
    check_page!(bytes, aoe_map::ModernLandCoverPage);
});
