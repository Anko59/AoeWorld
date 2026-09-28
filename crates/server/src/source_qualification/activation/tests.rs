use super::*;
use aoe_core::Seed;

#[test]
fn activation_component_diagnostic_keeps_explicit_bounds() {
    assert_eq!(
        ACTIVATION_COMPONENT_POLICY,
        "ordinary-bounded-component-diagnostic-v1"
    );
    assert_eq!(MAX_ACTIVATION_COMPONENT_TILES, 65_536);
    assert_eq!(MAX_ACTIVATION_COMPONENT_PROBE_WORK, 524_288);
}

#[test]
fn ordinary_component_enumeration_is_deterministic_and_contains_fixed_leg_target() {
    let config = WorldConfig::new(64, 64, Seed(3)).expect("config");
    let terrain = Terrain::uniform(config.seed.0);
    let start = TileCoord::new(10, 10);
    let first = enumerate_component(&terrain, config, start).expect("component");
    let second = enumerate_component(&terrain, config, start).expect("component");

    assert_eq!(first.tiles, second.tiles);
    assert_eq!(first.bounds, second.bounds);
    assert_eq!(first.probe_work, second.probe_work);
    assert!(first.tiles.contains(&TileCoord::new(12, 10)));
    assert!(!first.truncated);
    assert!(first.probe_work <= MAX_ACTIVATION_COMPONENT_PROBE_WORK);
}
