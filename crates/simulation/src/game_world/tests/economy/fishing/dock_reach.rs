//! A Dock foundation is only given to a builder that can walk to it inside
//! the territory.
use super::steward::domain;
use super::*;

#[test]
fn a_dock_foundation_waits_for_a_builder_that_can_reach_it() {
    let mut w = coast();
    let shore = site(&w);
    let (_, king) = domain(&mut w, shore);
    let rect = aoe_core::TileRect::from_xywh(shore.x, shore.y, 3, 3);
    let dock = w
        .insert_building(PlayerId(0), BuildingKind::Dock, rect, false)
        .unwrap();
    // The foundation's surroundings are cut off from the villager's ground.
    w.steward_cut_territory(king, Some(rect));
    let attended = |w: &GameWorld| w.economy.builders.values().any(|t| t.building == dock);
    for _ in 0..20 * 40 {
        w.advance();
        assert!(!attended(&w), "a builder was sent to an unreachable Dock");
    }
    assert!(!w.building(dock).unwrap().is_complete());
    // Control: with ordinary labelling the same foundation is built.
    w.steward_cut_territory(king, None);
    let mut built = false;
    for _ in 0..20 * 120 {
        w.advance();
        built |= attended(&w) || w.building(dock).unwrap().is_complete();
    }
    assert!(built, "a reachable Dock foundation must be built");
}
