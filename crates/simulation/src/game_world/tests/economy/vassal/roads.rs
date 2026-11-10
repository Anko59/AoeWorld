use super::*;

#[test]
fn vassal_roads_follow_fortifications_and_count_toward_development() {
    let (mut w, v, _) = setup();
    // The whole planned fortification stands; only the road is missing.
    for (kind, rect) in w.vassal_fortification(v) {
        w.insert_building(PlayerId(0), kind, rect, true).unwrap();
    }
    w.spawn_villager(PlayerId(0), WorldPosition::new(88 * 1024, 96 * 1024))
        .unwrap();
    advance(&mut w, 40);
    let status = w.vassal_status(v).unwrap();
    assert!(!status.developed);
    assert_eq!(status.next, Some(BuildingKind::Road));
    assert!(w.buildings().any(|b| b.kind == BuildingKind::Road));
    for _ in 0..12000 {
        w.advance();
        if w.vassal_status(v).unwrap().developed {
            break;
        }
    }
    assert!(
        w.vassal_status(v).unwrap().developed,
        "{:?}",
        w.vassal_status(v)
    );
    assert!(
        w.buildings()
            .any(|b| b.kind == BuildingKind::Road && b.is_complete())
    );
}

#[test]
fn road_demo_uses_ordinary_movement_and_a_fortified_vassal() {
    let (mut w, _) = GameWorld::roads_demo(aoe_core::Seed(7)).unwrap();
    assert_eq!(
        w.buildings()
            .filter(|b| b.kind == BuildingKind::Road)
            .count(),
        29
    );
    let v = w
        .buildings()
        .flat_map(|b| &b.garrison)
        .find(|g| g.unit.kind == UnitKind::Vassal)
        .unwrap()
        .unit
        .id;
    assert!(!w.vassal_status(v).unwrap().development);
    advance(&mut w, 40);
    assert_eq!(
        w.buildings()
            .filter(|b| b.kind == BuildingKind::Road)
            .count(),
        29
    );
    // Planning is now asynchronous. Resolve the paused fixture before testing
    // that enabling development does not take the villagers' live orders.
    w.vassal_network(v);
    let lanes: Vec<_> = w
        .units()
        .filter(|u| u.kind == UnitKind::Villager && [120, 123].contains(&u.position.tile_floor().y))
        .map(|u| (u.id, u.position))
        .collect();
    assert_eq!(lanes.len(), 2);
    for (id, from) in lanes {
        assert!(!w.unit(id).unwrap().moving);
        w.issue_move(id, WorldPosition::new(100 * 1024 + 512, from.y))
            .unwrap();
    }
    // The 100-tile domain covers the lanes: walkers are not idle, so the
    // Vassal starts its road without taking them.
    w.issue_vassal_development(PlayerId(0), v, true).unwrap();
    advance(&mut w, 20);
    assert!(
        w.buildings()
            .filter(|b| b.kind == BuildingKind::Road)
            .count()
            > 29
    );
    assert!(
        w.units()
            .filter(|u| u.moving && [120, 123].contains(&u.position.tile_floor().y))
            .count()
            == 2
    );
}

#[test]
fn vassal_road_purchases_wait_for_unfinished_fortifications() {
    let (mut w, _, _) = setup();
    w.set_fog_disabled(true);
    w.spawn_villager(PlayerId(0), WorldPosition::new(88 * 1024, 96 * 1024))
        .unwrap();
    advance(&mut w, 80);
    assert!(!w.buildings().any(|b| b.kind == BuildingKind::Road));
    assert!(w.buildings().any(|b| !b.is_complete()));
}
