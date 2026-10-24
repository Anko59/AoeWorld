use super::*;
use crate::BuildingKind;

#[test]
fn unexplored_footprints_are_refused_atomically_for_buildings_and_walls() {
    for wall in [false, true] {
        let mut w = world();
        w.initialize_village(PlayerId(0), TileCoord::new(20, 20))
            .unwrap();
        let builder = w.units().find(|u| u.kind == UnitKind::Villager).unwrap().id;
        let before = w.canonical_hash();
        let origin = TileCoord::new(100, 100);
        let result = if wall {
            w.issue_build_wall(PlayerId(0), &[builder], origin, origin)
                .map(|_| ())
        } else {
            w.issue_build(PlayerId(0), &[builder], BuildingKind::House, origin)
                .map(|_| ())
        };
        assert_eq!(
            result.unwrap_err().to_string(),
            "building footprint contains unexplored terrain"
        );
        assert_eq!(w.canonical_hash(), before);
        w.update_observer(EntityId(9000), Some((PlayerId(0), origin, 3)));
        w.update_observer(EntityId(9000), None);
        assert_eq!(w.visibility(PlayerId(0), origin), TileVisibility::Explored);
        if wall {
            w.issue_build_wall(PlayerId(0), &[builder], origin, origin)
                .unwrap();
        } else {
            w.issue_build(PlayerId(0), &[builder], BuildingKind::House, origin)
                .unwrap();
        }
    }
}

#[test]
fn every_footprint_tile_must_be_explored_by_the_ordering_player() {
    let mut w = world();
    w.initialize_village(PlayerId(0), TileCoord::new(20, 20))
        .unwrap();
    let builder = w.units().find(|u| u.kind == UnitKind::Villager).unwrap().id;
    let origin = TileCoord::new(100, 100);
    // A zero-radius observer explores only the origin, not the rest of a house.
    w.update_observer(EntityId(9000), Some((PlayerId(0), origin, 0)));
    assert_eq!(
        w.issue_build(PlayerId(0), &[builder], BuildingKind::House, origin),
        Err(crate::EconomyError::UnexploredFootprint)
    );
    let enemy = w.spawn_villager(PlayerId(1), at(101, 101)).unwrap();
    assert_eq!(
        w.issue_build(PlayerId(0), &[builder], BuildingKind::House, origin),
        Err(crate::EconomyError::UnexploredFootprint)
    );
    let before = w.buildings().count();
    assert_eq!(
        w.issue_build_wall(PlayerId(0), &[builder], origin, TileCoord::new(101, 100)),
        Err(crate::EconomyError::UnexploredFootprint)
    );
    assert_eq!(w.buildings().count(), before);
    w.remove_unit(enemy);
    w.update_observer(EntityId(9000), Some((PlayerId(0), origin, 3)));
    w.update_observer(EntityId(9000), None);
    assert_eq!(w.visibility(PlayerId(0), origin), TileVisibility::Explored);
    w.issue_build(PlayerId(0), &[builder], BuildingKind::House, origin)
        .unwrap();
}
