use super::{browser_run_deadline, server_binary};
use std::time::Duration;

#[test]
fn the_instrumented_debug_server_gets_its_own_deadline() {
    assert_eq!(browser_run_deadline(None), Duration::from_secs(2400));
    assert_eq!(browser_run_deadline(Some("1")), Duration::from_secs(2400));
}

#[test]
fn coverage_mode_cannot_silently_select_an_uninstrumented_server() {
    assert_eq!(server_binary(None), Ok("target/release/aoe-server"));
    assert_eq!(
        server_binary(Some("1")),
        Ok("target/llvm-cov-target/debug/aoe-server")
    );
    for invalid in ["", "0", "true", "release"] {
        assert!(server_binary(Some(invalid)).is_err());
    }
}

#[test]
fn creator_fallback_preview_fixture_has_no_passable_tiles_at_valid_minimum_size() {
    use aoe_core::TileCoord;
    use aoe_map::{MAP_SCHEMA_VERSION, MapPackage, MapRequest, Ratio};
    let request = MapRequest {
        center_latitude_e7: -791_524_000,
        center_longitude_e7: 0,
        requested_side_meters: 256,
        compression: Ratio::new(2, 1).unwrap(),
        seed: 1,
        ..MapRequest::default()
    };
    let package = MapPackage::new(MAP_SCHEMA_VERSION, request, vec![]).unwrap();
    assert_eq!(package.estimate.tiles_per_side, 64);
    let generator = package.generator();
    for y in 0..64 {
        for x in 0..64 {
            assert!(!generator.tile_at(TileCoord::new(x, y)).unwrap().passable);
        }
    }
}

#[test]
fn a_filtered_test_e2e_run_uses_the_quarantine_and_only_the_duel_check_does_not() {
    use super::Mode;
    let selected = vec!["tests/memory.shared-surfaces.spec.ts".to_owned()];
    // `make test-e2e AOE_E2E_SPECS=…` (closing-review F5/F6) and the whole suite.
    assert_eq!(Mode::of(&selected, true), Mode::Quarantined);
    assert_eq!(Mode::of(&[], true), Mode::Quarantined);
    // The focused duel check.
    assert_eq!(Mode::of(&selected, false), Mode::Focused);
}
