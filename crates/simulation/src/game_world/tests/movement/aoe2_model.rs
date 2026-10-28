//! Integrated traffic regressions: every actor must progress, not just the leader.
use super::*;
use crate::{BuildingKind, UnitKind, UnitTask};

fn at(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}

fn distance(a: WorldPosition, b: WorldPosition) -> i64 {
    let x = i64::from(a.x) - i64::from(b.x);
    let y = i64::from(a.y) - i64::from(b.y);
    x * x + y * y
}

#[test]
fn small_units_can_arrive_at_separate_positions_in_the_same_tile() {
    let mut w = world();
    let a = w
        .spawn_villager(
            PlayerId(0),
            WorldPosition::new(30 * 1024 + 220, 30 * 1024 + 220),
        )
        .unwrap();
    let b = w
        .spawn_villager(
            PlayerId(0),
            WorldPosition::new(30 * 1024 + 800, 30 * 1024 + 800),
        )
        .unwrap();
    assert!(w.arrival_position_free(w.lookup[&a]));
    assert!(w.arrival_position_free(w.lookup[&b]));
    let before = [w.unit(a).unwrap().position, w.unit(b).unwrap().position];
    for _ in 0..100 {
        w.advance();
    }
    assert_eq!(
        [w.unit(a).unwrap().position, w.unit(b).unwrap().position],
        before
    );
}

#[test]
fn two_groups_of_twenty_cross_without_deadlock_or_cancelled_orders() {
    let mut w = world();
    w.set_fog_disabled(true);
    let mut actors = Vec::new();
    for side in 0..2 {
        for i in 0..20 {
            let start = at(if side == 0 { 30 + i % 4 } else { 55 - i % 4 }, 40 + i / 4);
            let target = at(if side == 0 { 55 - i % 4 } else { 30 + i % 4 }, 40 + i / 4);
            let id = w.spawn_villager(PlayerId(side as u16), start).unwrap();
            w.issue_move(id, target).unwrap();
            actors.push((id, target, start, 0));
        }
    }
    for tick in 0..1600 {
        w.advance();
        for (n, a) in actors.iter().enumerate() {
            for b in &actors[n + 1..] {
                assert!(
                    distance(w.unit(a.0).unwrap().position, w.unit(b.0).unwrap().position)
                        >= 410 * 410,
                    "traffic overlap at tick {tick}"
                );
            }
        }
        for (id, target, previous, stalled) in &mut actors {
            let unit = w.unit(*id).unwrap();
            if unit.position != *previous || distance(unit.position, *target) < 1024 * 1024 {
                *stalled = 0;
            } else {
                *stalled += 1;
            }
            assert!(
                *stalled <= 160,
                "actor {id:?} stuck at tick {tick}: {unit:?}"
            );
            *previous = unit.position;
        }
    }
    for (id, target, _, _) in actors {
        assert!(
            distance(w.unit(id).unwrap().position, target) <= 1024 * 1024,
            "{id:?}: {:?}",
            w.unit(id)
        );
        assert!(w.movement_order(id).is_none());
        assert_eq!(w.units[w.lookup[&id]].last_movement_error, None);
    }
}

fn attacking_army(kind: UnitKind, count: i32) -> (GameWorld, EntityId, Vec<EntityId>) {
    let mut w = world();
    w.set_fog_disabled(true);
    w.place_town_center(PlayerId(1), TileCoord::new(90, 90))
        .unwrap();
    let house = w
        .insert_building(
            PlayerId(1),
            BuildingKind::House,
            TileRect::from_xywh(60, 60, 2, 2),
            true,
        )
        .unwrap();
    // Keep the target alive long enough to observe every attacker's first strike.
    w.economy.buildings.get_mut(&house).unwrap().hp = 20_000;
    let ids = (0..count)
        .map(|i| {
            w.spawn_kind(PlayerId(0), kind, at(50 - i / 4, 59 + i % 4))
                .unwrap()
        })
        .collect::<Vec<_>>();
    for &id in &ids {
        w.issue_attack(PlayerId(0), id, house).unwrap();
    }
    (w, house, ids)
}

#[test]
fn twelve_militia_surround_a_house_and_every_one_strikes() {
    let (mut w, house, ids) = attacking_army(UnitKind::Militia, 12);
    let mut hit = BTreeSet::new();
    let mut last = ids
        .iter()
        .map(|&id| (w.unit(id).unwrap().position, 0))
        .collect::<Vec<_>>();
    for tick in 0..1200 {
        w.advance();
        for (n, &id) in ids.iter().enumerate() {
            let unit = w.unit(id).unwrap();
            if w.units[w.lookup[&id]].next_hit_tick > w.tick().0 {
                hit.insert(id);
            }
            if unit.position != last[n].0 || hit.contains(&id) {
                last[n].1 = 0;
            } else {
                last[n].1 += 1;
            }
            assert!(last[n].1 <= 160, "{id:?} stalled at {tick}: {unit:?}");
            last[n].0 = unit.position;
        }
        if hit.len() == ids.len() {
            break;
        }
    }
    assert_eq!(hit.len(), ids.len(), "every militia must strike");
    assert!(w.building(house).unwrap().hp < 20_000);
    for (n, &id) in ids.iter().enumerate() {
        for &other in &ids[n + 1..] {
            assert!(
                distance(
                    w.unit(id).unwrap().position,
                    w.unit(other).unwrap().position
                ) >= 410 * 410
            );
        }
    }
}

#[test]
fn eight_archers_stop_at_range_and_fire_without_crowding_the_house() {
    let (mut w, _, ids) = attacking_army(UnitKind::Archer, 8);
    let mut fired = BTreeSet::new();
    let mut stalled = vec![0; ids.len()];
    let mut previous = ids
        .iter()
        .map(|&id| w.unit(id).unwrap().position)
        .collect::<Vec<_>>();
    for tick in 0..700 {
        w.advance();
        for (n, &id) in ids.iter().enumerate() {
            let unit = w.unit(id).unwrap();
            if w.units[w.lookup[&id]].next_hit_tick > w.tick().0 {
                fired.insert(id);
            }
            if fired.contains(&id) {
                assert_eq!(unit.task, UnitTask::Attacking);
                assert!(!unit.moving);
                let goal = w
                    .attack_goal(unit.attack_target.unwrap(), PlayerId(0))
                    .unwrap();
                let range = crate::game_world::navy::attack_range(&w.unit_stats(id).unwrap());
                assert!(goal.distance_squared(unit.position) <= range * range);
                assert!(goal.distance_squared(unit.position) >= 3 * 1024 * 3 * 1024);
            }
            if unit.position != previous[n] || fired.contains(&id) {
                stalled[n] = 0;
            } else {
                stalled[n] += 1;
            }
            assert!(stalled[n] <= 160, "{id:?} stalled at {tick}");
            previous[n] = unit.position;
        }
    }
    assert_eq!(fired.len(), ids.len());
}

#[path = "aoe2_model/gathering.rs"]
mod gathering;

#[test]
fn ships_use_their_own_radius_and_sweep_across_cell_boundaries() {
    let mut w = world();
    // The collision probe is independent of terrain; spawn on land then use
    // the same mirror update as the public naval spawn path.
    let a = w.spawn_villager(PlayerId(0), at(31, 30)).unwrap();
    let b = w
        .spawn_villager(
            PlayerId(0),
            WorldPosition::new(at(31, 30).x + 850, at(31, 30).y),
        )
        .unwrap();
    for id in [a, b] {
        let index = w.lookup[&id];
        w.untrack_unit_at(id, w.units[index].state.position);
        w.units[index].state.kind = UnitKind::Galley;
        let unit = w.units[index].state;
        w.track_unit_cell(&unit);
    }
    assert!(w.local_position_occupied(w.lookup[&a], w.unit(a).unwrap().position));
    assert!(!w.local_unit_step_clear(
        w.lookup[&a],
        WorldPosition::new(at(31, 30).x - 512, at(31, 30).y),
        w.unit(a).unwrap().position
    ));
}

#[test]
fn a_large_nearby_population_cannot_hide_the_actual_blocker() {
    let mut w = world();
    w.set_fog_disabled(true);
    let actor = w.spawn_villager(PlayerId(0), at(33, 33)).unwrap();
    for _ in 0..128 {
        w.spawn_villager(PlayerId(0), at(38, 38)).unwrap();
    }
    w.spawn_villager(
        PlayerId(0),
        WorldPosition::new(at(33, 33).x + 200, at(33, 33).y),
    )
    .unwrap();
    assert!(w.local_position_occupied(w.lookup[&actor], w.unit(actor).unwrap().position));
}

#[test]
fn a_congested_army_has_a_shared_local_search_budget() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    #[derive(Debug, Default)]
    struct Searches(AtomicUsize);
    impl crate::PlanningProbe for Searches {
        fn begin(&self) {}
        fn end(&self) {}
        fn movement_span(&self, name: &'static str) -> Option<Box<dyn aoe_map::NavigationSpan>> {
            if name == "local_refinement" {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
            None
        }
    }
    let mut w = world();
    w.set_fog_disabled(true);
    let probe = Arc::new(Searches::default());
    w.set_planning_probe(probe.clone());
    for n in 0..128 {
        w.spawn_kind(
            PlayerId(0),
            UnitKind::Knight,
            WorldPosition::new(40 * 1024 + 512, (10 * 1024) + n * 512),
        )
        .unwrap();
    }
    for n in 0..64 {
        let id = w.spawn_villager(PlayerId(0), at(30, 12 + n)).unwrap();
        w.issue_move(id, at(50, 12 + n)).unwrap();
    }
    let mut searched = false;
    for tick in 0..400 {
        probe.0.store(0, Ordering::Relaxed);
        w.advance();
        let count = probe.0.load(Ordering::Relaxed);
        searched |= count > 0;
        assert!(count <= 4, "{count} local searches at tick {tick}");
    }
    assert!(searched, "must exercise the refinement path");
}

#[test]
fn expensive_attack_searches_defer_orders_under_one_tick_budget() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    #[derive(Debug, Default)]
    struct Searches(AtomicUsize);
    impl crate::PlanningProbe for Searches {
        fn begin(&self) {}
        fn end(&self) {}
        fn movement_span(&self, name: &'static str) -> Option<Box<dyn aoe_map::NavigationSpan>> {
            if name == "attack_position_search" {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
            None
        }
    }
    let mut w = world();
    w.set_fog_disabled(true);
    let probe = Arc::new(Searches::default());
    w.set_planning_probe(probe.clone());
    let target = w
        .place_town_center(PlayerId(1), TileCoord::new(210, 180))
        .unwrap();
    let ids = (0..12)
        .map(|n| w.spawn_militia(PlayerId(0), at(30, 30 + n)).unwrap())
        .collect::<Vec<_>>();
    for &id in &ids {
        w.issue_attack(PlayerId(0), id, target).unwrap();
    }
    assert!((1..=4).contains(&probe.0.load(Ordering::Relaxed)));
    let starts = ids
        .iter()
        .map(|&id| w.unit(id).unwrap().position)
        .collect::<Vec<_>>();
    for tick in 0..150 {
        probe.0.store(0, Ordering::Relaxed);
        w.advance();
        assert!(
            probe.0.load(Ordering::Relaxed) <= 4,
            "attack search burst at {tick}"
        );
        assert!(
            ids.iter()
                .all(|&id| w.unit(id).unwrap().attack_target == Some(target))
        );
    }
    for (&id, before) in ids.iter().zip(starts) {
        assert_ne!(
            w.unit(id).unwrap().position,
            before,
            "deferred attacker {id:?} never moved"
        );
    }
}

#[test]
fn a_shared_click_at_the_map_edge_keeps_legal_connected_arrivals() {
    let mut w = world();
    let a = w.spawn_villager(PlayerId(0), at(5, 5)).unwrap();
    let b = w.spawn_villager(PlayerId(0), at(6, 5)).unwrap();
    for id in [a, b] {
        w.issue_move(id, at(0, 0)).unwrap();
    }
    for _ in 0..400 {
        w.advance();
    }
    for id in [a, b] {
        let u = w.unit(id).unwrap();
        assert!(w.config().valid_ground_position(u.position));
        assert!(distance(u.position, at(0, 0)) <= 3 * 1024 * 3 * 1024);
        assert!(w.movement_order(id).is_none());
    }
    assert!(distance(w.unit(a).unwrap().position, w.unit(b).unwrap().position) >= 410 * 410);
}

#[test]
fn approaching_melee_units_reserve_contact_positions_using_both_body_radii() {
    let mut w = world();
    w.set_fog_disabled(true);
    let target = w
        .spawn_kind(PlayerId(1), UnitKind::Vassal, at(60, 60))
        .unwrap();
    w.units[w.lookup[&target]].state.hp = 20_000;
    // Contact geometry needs a stationary target: automatic combat would
    // otherwise let the King react to the hits and leave its spot.
    w.units[w.lookup[&target]].state.stance = aoe_core::CombatStance::NoAttack;
    let ids = (0..6)
        .map(|n| {
            w.spawn_militia(PlayerId(0), at(50 - n / 3, 59 + n % 3))
                .unwrap()
        })
        .collect::<Vec<_>>();
    for &id in &ids {
        w.issue_attack(PlayerId(0), id, target).unwrap();
    }
    let mut hit = BTreeSet::new();
    let mut previous = ids
        .iter()
        .map(|&id| (w.unit(id).unwrap().position, 0))
        .collect::<Vec<_>>();
    for tick in 0..1000 {
        w.advance();
        for (n, &id) in ids.iter().enumerate() {
            let u = w.unit(id).unwrap();
            if w.units[w.lookup[&id]].next_hit_tick > w.tick().0 {
                hit.insert(id);
                let target = w.unit(target).unwrap();
                let r = i64::from(u.kind.collision_radius() + target.kind.collision_radius());
                let d = distance(u.position, target.position);
                assert!(
                    d >= r * r && d <= (r + 104) * (r + 104),
                    "remote melee contact: {u:?}"
                );
            }
            if u.position != previous[n].0 || hit.contains(&id) {
                previous[n].1 = 0;
            } else {
                previous[n].1 += 1;
            }
            assert!(previous[n].1 <= 160, "{id:?} stalled at tick {tick}");
            previous[n].0 = u.position;
        }
        if hit.len() == ids.len() {
            break;
        }
    }
    assert_eq!(hit.len(), ids.len());
}

#[path = "aoe2_model/obstruction.rs"]
mod obstruction;
