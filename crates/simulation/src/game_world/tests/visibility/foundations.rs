use super::*;
use crate::BuildingKind;

#[test]
fn untouched_foundations_have_no_observer_even_after_fog_is_reenabled() {
    for wall in [false, true] {
        let mut w = world();
        w.initialize_village(PlayerId(0), TileCoord::new(20, 20))
            .unwrap();
        let builder = w.units().find(|u| u.kind == UnitKind::Villager).unwrap().id;
        let origin = TileCoord::new(100, 100);
        w.update_observer(EntityId(9000), Some((PlayerId(0), origin, 1)));
        w.update_observer(EntityId(9000), None);
        let before = w.vision_storage(PlayerId(0));
        let id = if wall {
            w.issue_build_wall(
                PlayerId(0),
                &[builder],
                BuildingKind::PalisadeWall,
                origin,
                origin,
                false,
            )
            .unwrap()[0]
        } else {
            w.issue_build(
                PlayerId(0),
                &[builder],
                BuildingKind::Outpost,
                origin,
                false,
            )
            .unwrap()
        };
        w.issue_stop(PlayerId(0), builder).unwrap();
        assert_eq!(w.building(id).unwrap().work_done, 0);
        assert_eq!(w.vision_storage(PlayerId(0)), before);
        assert_eq!(w.visibility(PlayerId(0), origin), TileVisibility::Explored);
        assert!(w.known_buildings(PlayerId(0)).iter().any(|b| b.id == id));
        w.set_fog_disabled(true);
        w.set_fog_disabled(false);
        assert_eq!(w.visibility(PlayerId(0), origin), TileVisibility::Explored);
        w.issue_cancel_construction(PlayerId(0), id).unwrap();
        assert!(w.known_buildings(PlayerId(0)).iter().all(|b| b.id != id));
    }
}

#[test]
fn first_build_work_starts_sight_and_construction_grows_it_to_building_los() {
    let mut w = world();
    w.initialize_village(PlayerId(0), TileCoord::new(20, 20))
        .unwrap();
    let builder = w.spawn_villager(PlayerId(0), at(99, 100)).unwrap();
    let id = w
        .issue_build(
            PlayerId(0),
            &[builder],
            BuildingKind::Outpost,
            TileCoord::new(100, 100),
            false,
        )
        .unwrap();
    assert_eq!(w.building(id).unwrap().work_done, 0);
    w.advance();
    assert!(w.building(id).unwrap().work_done > 0);
    w.remove_unit(builder);
    assert_eq!(
        w.visibility(PlayerId(0), TileCoord::new(100, 100)),
        TileVisibility::Visible
    );
    assert_eq!(
        w.visibility(PlayerId(0), TileCoord::new(102, 100)),
        TileVisibility::Explored
    );
    let builder = w.spawn_villager(PlayerId(0), at(99, 100)).unwrap();
    w.issue_construct(PlayerId(0), &[builder], id).unwrap();
    for _ in 0..149 {
        w.advance();
    }
    w.remove_unit(builder);
    assert_eq!(
        w.visibility(PlayerId(0), TileCoord::new(105, 100)),
        TileVisibility::Visible
    );
    assert_eq!(
        w.visibility(PlayerId(0), TileCoord::new(106, 100)),
        TileVisibility::Unexplored
    );
    let builder = w.spawn_villager(PlayerId(0), at(99, 100)).unwrap();
    w.issue_construct(PlayerId(0), &[builder], id).unwrap();
    for _ in 0..150 {
        w.advance();
    }
    assert!(w.building(id).unwrap().is_complete());
    w.remove_unit(builder);
    assert_eq!(
        w.visibility(PlayerId(0), TileCoord::new(110, 100)),
        TileVisibility::Visible
    );
}

#[test]
fn construction_sight_replays_deterministically() {
    let setup = || {
        let mut w = world();
        w.initialize_village(PlayerId(0), TileCoord::new(20, 20))
            .unwrap();
        let builder = w.spawn_villager(PlayerId(0), at(99, 100)).unwrap();
        w.issue_build(
            PlayerId(0),
            &[builder],
            BuildingKind::Outpost,
            TileCoord::new(100, 100),
            false,
        )
        .unwrap();
        w
    };
    let (mut a, mut b) = (setup(), setup());
    for _ in 0..320 {
        a.advance();
        b.advance();
        assert_eq!(a.canonical_hash(), b.canonical_hash());
    }
}
