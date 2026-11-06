//! A saved bridge keeps its deck, and units standing on it.
use super::*;

#[test]
fn a_completed_bridge_and_a_unit_on_its_deck_survive_a_save() {
    let (mut world, worker) = setup();
    let bridge = world
        .issue_build_bridge(
            PlayerId(0),
            &[worker],
            TileCoord::new(29, 20),
            TileCoord::new(34, 20),
            false,
        )
        .unwrap();
    advance(&mut world, 20 * 100);
    assert!(world.building(bridge).unwrap().is_complete());
    let deck = TileCoord::new(31, 20);
    world
        .issue_move(worker, WorldPosition::from_tile_center(deck).unwrap())
        .unwrap();
    advance(&mut world, 20 * 15);
    assert_eq!(world.unit(worker).unwrap().position.tile_floor(), deck);
    let (restored, report) =
        crate::game_world::tests::snapshot_roundtrip::roundtrip(&world, river_width(true, 30..34));
    assert_eq!(
        (report.units_lost, report.buildings_lost),
        (0, 0),
        "{report:?}"
    );
    assert!(restored.tile_passable(deck), "the deck is walkable again");
    assert!(restored.building(bridge).unwrap().is_complete());
    assert_eq!(restored.unit(worker).unwrap().position.tile_floor(), deck);
    let across = WorldPosition::from_tile_center(TileCoord::new(36, 20)).unwrap();
    let mut restored = restored;
    restored.issue_move(worker, across).unwrap();
    advance(&mut restored, 20 * 20);
    assert_eq!(
        restored.unit(worker).unwrap().position.tile_floor(),
        TileCoord::new(36, 20)
    );
}
