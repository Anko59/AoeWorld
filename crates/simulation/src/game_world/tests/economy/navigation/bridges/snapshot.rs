//! A saved bridge keeps its deck, and units standing on it.
use super::*;
use aoe_core::BridgeSpan;

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

/// Seeding once left a finished bridge shell with no deck when its span
/// crossed another deck. That shell cannot stand again: the restore drops it,
/// counts it, and still gives the game back.
#[test]
fn a_deckless_bridge_shell_crossing_a_deck_is_dropped_on_restore() {
    let mut world = river_width(true, 30..34);
    let spans = [
        BridgeSpan::new(TileCoord::new(29, 20), TileCoord::new(34, 20)).unwrap(),
        BridgeSpan::new(TileCoord::new(31, 18), TileCoord::new(31, 23)).unwrap(),
    ];
    for (n, span) in spans.into_iter().enumerate() {
        world
            .insert_building_shape(
                PlayerId(0),
                BuildingKind::Bridge,
                span.footprint(),
                true,
                Some(span),
            )
            .unwrap();
        let Terrain::Map { overlay, .. } = &mut world.terrain else {
            unreachable!()
        };
        // The second span has no deck: the overlay refuses a crossing.
        assert_eq!(overlay.add_bridge(span).is_ok(), n == 0);
    }
    let (restored, report) =
        crate::game_world::tests::snapshot_roundtrip::roundtrip(&world, river_width(true, 30..34));
    assert_eq!(
        (
            report.units_lost,
            report.buildings_lost,
            report.bridges_dropped
        ),
        (0, 0, 1),
        "{report:?}"
    );
    assert_eq!(restored.buildings().count(), 1);
    assert!(restored.tile_passable(TileCoord::new(31, 20)));
}
