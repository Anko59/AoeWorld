use super::*;
use crate::{Tech, TileVisibility, UnitKind};
mod foundations;
mod placement;

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
    w.economy.buildings.get_mut(&id).unwrap().age = crate::Age::Feudal;
    let owner = w.economy.players.get_mut(&PlayerId(1)).unwrap();
    owner.age = crate::Age::Feudal;
    owner.researched |= Tech::FeudalAge.bit();
    w.advance();
    assert_eq!(w.known_buildings(PlayerId(0))[0].hp, remembered[0].hp);
    assert_eq!(w.known_buildings(PlayerId(0))[0].age, crate::Age::Dark);
    w.issue_move(scout, at(64, 64)).unwrap();
    for _ in 0..700 {
        w.advance();
    }
    assert_eq!(w.known_buildings(PlayerId(0))[0].age, crate::Age::Feudal);
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
    let owner = w.economy.players.get_mut(&PlayerId(0)).unwrap();
    owner.age = crate::Age::Feudal;
    owner.researched |= Tech::FeudalAge.bit();
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

#[test]
fn rejected_foundations_leave_all_visibility_and_memory_unchanged() {
    use crate::{BuildingKind, EconomyError};
    for wall_order in [false, true] {
        let mut w = world();
        w.initialize_village(PlayerId(0), TileCoord::new(20, 20))
            .unwrap();
        w.initialize_village(PlayerId(1), TileCoord::new(200, 200))
            .unwrap();
        let builder = w
            .units()
            .find(|u| u.player == PlayerId(0) && u.kind == UnitKind::Villager)
            .unwrap()
            .id;
        let enemy_builder = w
            .units()
            .find(|u| u.player == PlayerId(1) && u.kind == UnitKind::Villager)
            .unwrap()
            .id;
        // Each player must survey its build tiles. Only the enclosed centre
        // was explored by player 0, so the enclosing enemy walls stay unknown.
        w.update_observer(
            EntityId(9000),
            Some((PlayerId(1), TileCoord::new(100, 100), 3)),
        );
        w.update_observer(EntityId(9000), None);
        w.update_observer(
            EntityId(9001),
            Some((PlayerId(0), TileCoord::new(100, 100), 0)),
        );
        w.update_observer(EntityId(9001), None);
        for y in 99..=101 {
            for x in 99..=101 {
                if (x, y) != (100, 100) {
                    w.issue_build(
                        PlayerId(1),
                        &[enemy_builder],
                        BuildingKind::PalisadeWall,
                        TileCoord::new(x, y),
                    )
                    .unwrap();
                }
            }
        }
        let vision_hash = |w: &GameWorld| {
            let mut hash = blake3::Hasher::new();
            w.vision.update_hash(&mut hash);
            hash.finalize()
        };
        let before = vision_hash(&w);
        let revision = w.vision_revision();
        let storage = w.vision_storage(PlayerId(0));
        let overview = w.vision_overview(PlayerId(0));
        let stock = w.player_economy(PlayerId(0)).unwrap().stockpile;
        let tile = TileCoord::new(100, 100);
        assert_eq!(w.visibility(PlayerId(0), tile), TileVisibility::Explored);
        assert!(
            w.known_buildings(PlayerId(0))
                .iter()
                .all(|b| b.player == PlayerId(0))
        );
        let result = if wall_order {
            w.issue_build_wall(PlayerId(0), &[builder], tile, tile)
                .map(|_| ())
        } else {
            w.issue_build(PlayerId(0), &[builder], BuildingKind::Outpost, tile)
                .map(|_| ())
        };
        assert_eq!(
            result,
            Err(EconomyError::Movement(GameWorldError::Unreachable))
        );
        assert_eq!(
            vision_hash(&w),
            before,
            "rejected order changed exploration or memory"
        );
        assert_eq!(w.vision_revision(), revision);
        assert_eq!(w.vision_storage(PlayerId(0)), storage);
        assert_eq!(w.vision_overview(PlayerId(0)), overview);
        assert_eq!(w.player_economy(PlayerId(0)).unwrap().stockpile, stock);
    }
}

#[test]
fn minimap_covers_every_pixel_of_small_explored_tiles_across_chunk_edges() {
    for (width, height) in [(64, 64), (64, 96), (256, 256), (512, 512)] {
        let mut w = GameWorld::new(WorldConfig::new(width, height, Seed(7)).unwrap()).unwrap();
        let id = EntityId(999);
        let player = PlayerId(0);
        // Visit every tile through the same observer update used by movement.
        for y in 0..height {
            for x in 0..width {
                w.update_observer(id, Some((player, TileCoord::new(x, y), 0)));
            }
        }
        let overview = w
            .vision_overview(player)
            .into_iter()
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(overview.len(), 256 * 256, "black gaps on {width}x{height}");
        for y in [0, 31, 32, height - 1] {
            for x in [0, 31, 32, width - 1] {
                let tile = TileCoord::new(x, y);
                w.update_observer(id, Some((player, tile, 0)));
                let overview = w
                    .vision_overview(player)
                    .into_iter()
                    .collect::<std::collections::BTreeMap<_, _>>();
                let x0 = x * 256 / width;
                let y0 = y * 256 / height;
                let x1 = ((x + 1) * 256 + width - 1) / width;
                let y1 = ((y + 1) * 256 + height - 1) / height;
                for py in y0..y1 {
                    for px in x0..x1 {
                        assert_eq!(overview[&((py * 256 + px) as u16)], 2);
                    }
                }
            }
        }
        w.update_observer(id, None);
        assert!(
            w.vision_overview(player)
                .iter()
                .all(|(_, state)| *state == 1)
        );
    }
}

#[test]
fn minimap_reveals_only_the_observed_tile_pixels_and_counts_shared_pixels() {
    let player = PlayerId(0);
    for axis in [64, 512] {
        let mut w = GameWorld::new(WorldConfig::new(axis, axis, Seed(7)).unwrap()).unwrap();
        let first = EntityId(999);
        let second = EntityId(1000);
        w.update_observer(first, Some((player, TileCoord::new(32, 32), 0)));
        let overview = w.vision_overview(player);
        let pixels_per_axis = (256 / axis).max(1);
        assert_eq!(overview.len(), (pixels_per_axis * pixels_per_axis) as usize);
        assert!(overview.iter().all(|(pixel, state)| {
            let x = i32::from(*pixel % 256);
            let y = i32::from(*pixel / 256);
            *state == 2
                && (32 * 256 / axis..32 * 256 / axis + pixels_per_axis).contains(&x)
                && (32 * 256 / axis..32 * 256 / axis + pixels_per_axis).contains(&y)
        }));
        let other_tile = if axis == 512 {
            TileCoord::new(33, 33)
        } else {
            TileCoord::new(32, 32)
        };
        w.update_observer(second, Some((player, other_tile, 0)));
        w.update_observer(first, None);
        assert_eq!(w.vision_overview(player), overview);
        w.update_observer(second, None);
        assert!(
            w.vision_overview(player)
                .iter()
                .all(|(_, state)| *state == 1)
        );
    }
}

#[test]
fn a_moved_observer_bumps_its_owner_revision_through_the_edge_path() {
    let mut w = world();
    let id = w.spawn_villager(PlayerId(0), at(20, 20)).unwrap();
    let before = w.player_vision_revision(PlayerId(0));
    // Same owner and LOS: the update takes the disc-edge path.
    w.update_observer(id, Some((PlayerId(0), TileCoord::new(21, 20), 3)));
    assert_ne!(w.player_vision_revision(PlayerId(0)), before);
    assert_eq!(
        w.visibility(PlayerId(0), TileCoord::new(24, 20)),
        TileVisibility::Visible
    );
    // An unchanged observer is a no-op and keeps the revision.
    let moved = w.player_vision_revision(PlayerId(0));
    assert!(!w.update_observer(id, Some((PlayerId(0), TileCoord::new(21, 20), 3))));
    assert_eq!(w.player_vision_revision(PlayerId(0)), moved);
}

#[test]
fn re_enabling_fog_bumps_the_revision_of_a_player_whose_observer_vanished() {
    let mut w = world();
    let id = w.spawn_villager(PlayerId(0), at(20, 20)).unwrap();
    let enabled = w.player_vision_revision(PlayerId(0));
    w.set_fog_disabled(true);
    // The observer disappears while fog is off, so no update reaches vision.
    w.remove_unit(id);
    w.set_fog_disabled(false);
    assert_ne!(
        w.player_vision_revision(PlayerId(0)),
        enabled,
        "re-enabling fog cleared this player's sight and must bump it"
    );
}
