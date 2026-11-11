use super::*;
use aoe_core::TileRect;

#[test]
fn removing_obstacles_preserves_witnesses_but_adding_them_does_not() {
    let mut old = ResourceOverlay::default();
    let rect = TileRect::from_xywh(20, 20, 2, 2);
    old.add_footprint(rect).unwrap();
    let mut opened = old.clone();
    opened.remove_footprint(rect);
    assert!(opened.navigation_preserves_walkability(&old));
    assert!(!old.navigation_preserves_walkability(&opened));
}

#[test]
fn depleted_resources_only_open_ground_and_snapshot_regrowth_invalidates() {
    let old = ResourceOverlay::default();
    let mut depleted = old.clone();
    depleted.navigation_toggle(TileCoord::new(20, 20), 2, 77);
    assert!(depleted.navigation_preserves_walkability(&old));
    assert!(!old.navigation_preserves_walkability(&depleted));
}

#[test]
fn bridge_direction_edits_are_never_assumed_to_only_open_ground() {
    let old = ResourceOverlay::default();
    let mut deck = old.clone();
    deck.navigation_toggle(TileCoord::new(20, 20), 4, 1);
    assert!(!deck.navigation_preserves_walkability(&old));
    assert!(!old.navigation_preserves_walkability(&deck));
    assert!(deck.clone().navigation_preserves_walkability(&deck));
}
