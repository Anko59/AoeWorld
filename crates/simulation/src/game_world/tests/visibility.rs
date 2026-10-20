use super::*;
use crate::{Tech, TileVisibility, UnitKind};

fn at(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}

#[test]
fn los_reveals_a_circle_and_exploration_persists_after_moving() {
    let mut w = world();
    let id = w.spawn_villager(PlayerId(0), at(20, 20)).unwrap();
    assert_eq!(
        w.visibility(PlayerId(0), TileCoord::new(24, 20)),
        TileVisibility::Visible
    );
    assert_eq!(
        w.visibility(PlayerId(0), TileCoord::new(24, 24)),
        TileVisibility::Unexplored
    );
    w.issue_move(id, at(40, 20)).unwrap();
    for _ in 0..700 {
        w.advance();
    }
    assert_eq!(
        w.visibility(PlayerId(0), TileCoord::new(20, 20)),
        TileVisibility::Explored
    );
    assert_eq!(
        w.visibility(PlayerId(1), TileCoord::new(20, 20)),
        TileVisibility::Unexplored
    );
}

#[test]
fn enemies_hide_in_fog_but_owned_units_remain_visible() {
    let mut w = world();
    let own = w.spawn_villager(PlayerId(0), at(20, 20)).unwrap();
    let enemy = w.spawn_unit(PlayerId(1), at(22, 20)).unwrap();
    assert!(w.unit_visible_to(PlayerId(0), enemy));
    w.issue_move(own, at(40, 20)).unwrap();
    for _ in 0..700 {
        w.advance();
    }
    assert!(!w.unit_visible_to(PlayerId(0), enemy));
    assert!(w.unit_visible_to(PlayerId(0), own));
}

#[test]
fn remembered_building_keeps_last_seen_state_until_revisited() {
    let mut w = world();
    let scout = w.spawn_unit(PlayerId(0), at(64, 64)).unwrap();
    w.place_town_center(PlayerId(1), TileCoord::new(65, 65))
        .unwrap();
    w.advance();
    let remembered = w.known_buildings(PlayerId(0));
    assert_eq!(remembered.len(), 1);
    w.issue_move(scout, at(20, 20)).unwrap();
    for _ in 0..700 {
        w.advance();
    }
    assert_eq!(
        w.visibility(PlayerId(0), remembered[0].footprint.min),
        TileVisibility::Explored
    );
    let id = remembered[0].id;
    w.economy.buildings.get_mut(&id).unwrap().hp -= 100;
    w.advance();
    assert_eq!(w.known_buildings(PlayerId(0))[0].hp, remembered[0].hp);
}

#[test]
fn town_watch_increases_reveal_by_four_tiles() {
    let mut w = world();
    w.initialize_village(PlayerId(0), TileCoord::new(64, 64))
        .unwrap();
    let building = w.buildings().next().unwrap();
    let center = building.id;
    let tile = TileCoord::new(
        (building.footprint.min.x + building.footprint.max.x) / 2 + 9,
        (building.footprint.min.y + building.footprint.max.y) / 2,
    );
    assert_eq!(w.visibility(PlayerId(0), tile), TileVisibility::Unexplored);
    w.issue_research(PlayerId(0), center, Tech::TownWatch)
        .unwrap();
    for _ in 0..500 {
        w.advance();
    }
    assert_eq!(w.visibility(PlayerId(0), tile), TileVisibility::Visible);
}

#[test]
fn sparse_storage_and_idle_work_are_independent_of_map_area() {
    for axis in [256, 262_144] {
        let mut w = GameWorld::new(WorldConfig::new(axis, axis, Seed(7)).unwrap()).unwrap();
        w.spawn_kind(PlayerId(0), UnitKind::Militia, at(64, 64))
            .unwrap();
        let (chunks, visible, work) = w.vision_storage(PlayerId(0));
        assert!(chunks <= 4 && visible <= 81 && work <= 162);
        for _ in 0..20 {
            w.advance();
        }
        assert_eq!(w.vision_storage(PlayerId(0)), (chunks, visible, work));
    }
}

#[test]
fn fog_replay_is_deterministic_and_history_affects_hash() {
    let run = || {
        let mut w = world();
        let id = w.spawn_unit(PlayerId(0), at(20, 20)).unwrap();
        w.issue_move(id, at(40, 30)).unwrap();
        for _ in 0..250 {
            w.advance();
        }
        w
    };
    let a = run();
    let b = run();
    assert_eq!(a.canonical_hash(), b.canonical_hash());
    assert_eq!(a.vision_storage(PlayerId(0)), b.vision_storage(PlayerId(0)));
    let mut cleared = run();
    let original = cleared.canonical_hash();
    cleared.vision = Default::default();
    assert_ne!(cleared.canonical_hash(), original);
}

#[test]
fn overlapping_observers_and_death_preserve_only_living_sight() {
    let mut w = world();
    let a = w.spawn_villager(PlayerId(0), at(20, 20)).unwrap();
    let b = w.spawn_villager(PlayerId(0), at(21, 20)).unwrap();
    let tile = TileCoord::new(20, 20);
    w.kill_unit(w.lookup[&a]);
    assert_eq!(w.visibility(PlayerId(0), tile), TileVisibility::Visible);
    w.kill_unit(w.lookup[&b]);
    assert_eq!(w.visibility(PlayerId(0), tile), TileVisibility::Explored);
}

#[test]
fn a_destroyed_owned_building_is_removed_from_knowledge() {
    let mut w = world();
    let id = w
        .place_town_center(PlayerId(0), TileCoord::new(40, 40))
        .unwrap();
    assert_eq!(w.known_buildings(PlayerId(0)).len(), 1);
    w.remove_building(id);
    assert!(w.known_buildings(PlayerId(0)).is_empty());
}

#[test]
fn converted_sheep_reveal_with_their_dat_los() {
    let mut w = world();
    w.spawn_villager(PlayerId(0), at(20, 20)).unwrap();
    w.spawn_animal(crate::AnimalKind::Sheep, at(22, 20))
        .unwrap();
    w.advance();
    assert_eq!(
        w.visibility(PlayerId(0), TileCoord::new(25, 20)),
        TileVisibility::Visible
    );
}

#[test]
fn moving_observer_tile_work_is_independent_of_idle_population() {
    let run = |idle| {
        let mut w = world();
        let id = w.spawn_unit(PlayerId(0), at(20, 20)).unwrap();
        for _ in 0..idle {
            w.spawn_unit(PlayerId(1), at(200, 200)).unwrap();
        }
        let before = w.vision_storage(PlayerId(0)).2;
        w.issue_move(id, at(22, 20)).unwrap();
        for _ in 0..40 {
            w.advance();
        }
        w.vision_storage(PlayerId(0)).2 - before
    };
    let work = run(0);
    assert!(work > 0 && work <= 2 * 2 * 81);
    assert_eq!(work, run(1000));
}
