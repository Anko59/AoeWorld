//! Continuation uses the same ten-tile replacement radius as trees and mines.
use super::*;

fn harvesting(kind: AnimalKind, far_dropoff: bool) -> (GameWorld, EntityId, EntityId) {
    let mut w = world();
    w.set_fog_disabled(true);
    w.place_town_center(
        PlayerId(0),
        TileCoord::new(if far_dropoff { 60 } else { 95 }, 95),
    )
    .unwrap();
    let worker = w.spawn_villager(PlayerId(0), at(100, 100)).unwrap();
    let old = w.spawn_animal(kind, at(101, 100)).unwrap();
    w.advance();
    w.issue_hunt(PlayerId(0), &[worker], old).unwrap();
    assert!(w.strike_animal(old, worker, 100));
    run(&mut w, 25);
    while w.animal(old).unwrap().food_remaining > 1 {
        w.deplete_carcass(old).unwrap();
    }
    assert_eq!(w.gathering_kind(worker), Some(aoe_map::ResourceKind::Food));
    (w, worker, old)
}

fn owned_sheep(w: &mut GameWorld, x: i32, y: i32) -> EntityId {
    let helper = w.spawn_unit(PlayerId(0), at(x, y)).unwrap();
    let sheep = w.spawn_animal(AnimalKind::Sheep, at(x, y)).unwrap();
    w.advance();
    assert_eq!(w.animal(sheep).unwrap().owner, Some(PlayerId(0)));
    w.remove_unit(helper);
    sheep
}

fn until_target(w: &mut GameWorld, worker: EntityId, target: EntityId, ticks: u32) {
    for _ in 0..ticks {
        w.advance();
        if w.hunting_target(worker) == Some(target) {
            return;
        }
    }
    panic!(
        "worker did not continue to {target:?}: {:?}",
        w.unit(worker)
    );
}

#[test]
fn shepherd_delivers_the_last_food_and_starts_the_next_owned_sheep() {
    let (mut w, worker, old) = harvesting(AnimalKind::Sheep, false);
    let next = owned_sheep(&mut w, 104, 100);
    until_target(&mut w, worker, next, 500);
    assert_eq!(w.animal(old).unwrap().food_remaining, 0);
    assert_eq!(w.player_economy(PlayerId(0)).unwrap().stockpile.food, 1);
    assert!(!matches!(
        w.unit(worker).unwrap().task,
        UnitTask::Idle | UnitTask::Carrying
    ));
}

#[test]
fn hunter_delivers_the_last_food_and_hunts_the_next_live_deer() {
    let (mut w, worker, _) = harvesting(AnimalKind::Deer, false);
    let next = w.spawn_animal(AnimalKind::Deer, at(104, 100)).unwrap();
    until_target(&mut w, worker, next, 500);
    assert_eq!(w.unit(worker).unwrap().task, UnitTask::Hunting);
    assert_eq!(w.player_economy(PlayerId(0)).unwrap().stockpile.food, 1);
}

#[test]
fn a_hunter_uses_the_nearest_hunt_carcass_and_does_not_switch_to_sheep() {
    let (mut w, worker, _) = harvesting(AnimalKind::Deer, false);
    owned_sheep(&mut w, 102, 100);
    let carcass = w.spawn_animal(AnimalKind::Boar, at(103, 100)).unwrap();
    assert!(w.strike_animal(carcass, worker, 100));
    until_target(&mut w, worker, carcass, 500);
    assert_eq!(w.gathering_kind(worker), Some(aoe_map::ResourceKind::Food));
    assert!(w.animal(carcass).unwrap().food_remaining > 0);
}

#[test]
fn food_return_keeps_its_profession_after_the_empty_carcass_disappears() {
    let (mut w, worker, old) = harvesting(AnimalKind::Sheep, true);
    let next = owned_sheep(&mut w, 104, 100);
    until_target(&mut w, worker, next, 1800);
    assert!(
        w.animal(old).is_none(),
        "the return trip outlasts carcass expiry"
    );
    assert_eq!(w.player_economy(PlayerId(0)).unwrap().stockpile.food, 1);
    assert_eq!(w.unit(worker).unwrap().carried, None);
}

#[test]
fn no_same_kind_within_the_shared_radius_leaves_the_worker_idle() {
    for kind in [AnimalKind::Sheep, AnimalKind::Deer] {
        let (mut w, worker, old) = harvesting(kind, false);
        if kind == AnimalKind::Sheep {
            w.spawn_animal(AnimalKind::Deer, at(104, 100)).unwrap();
            owned_sheep(&mut w, 112, 100); // eleven tiles from the spent source
        } else {
            owned_sheep(&mut w, 104, 100);
            w.spawn_animal(AnimalKind::Boar, at(112, 100)).unwrap();
        }
        run(&mut w, 500);
        assert_eq!(w.hunting_target(worker), None, "{kind:?}");
        assert_eq!(w.unit(worker).unwrap().task, UnitTask::Idle, "{kind:?}");
        assert_eq!(w.unit(worker).unwrap().carried, None);
        assert!(w.animal(old).is_none_or(|a| a.food_remaining == 0));
    }
}

#[test]
fn replacement_query_uses_distance_then_identity_and_ignores_foreign_sheep() {
    let (mut w, worker, old) = harvesting(AnimalKind::Sheep, false);
    let foreign_helper = w.spawn_unit(PlayerId(1), at(102, 100)).unwrap();
    let foreign = w.spawn_animal(AnimalKind::Sheep, at(102, 100)).unwrap();
    w.advance();
    assert_eq!(w.animal(foreign).unwrap().owner, Some(PlayerId(1)));
    w.remove_unit(foreign_helper);
    let low = owned_sheep(&mut w, 104, 100);
    let high = owned_sheep(&mut w, 98, 100);
    assert!(low < high);
    let next = w
        .next_food_animal(
            PlayerId(0),
            TileCoord::new(101, 100),
            AnimalKind::Sheep,
            old,
        )
        .unwrap();
    assert_eq!(next.id, low);
    assert_ne!(next.id, foreign);
    assert_eq!(w.hunting_target(worker), Some(old));
}

#[test]
fn one_hundred_players_do_not_expand_the_local_replacement_query() {
    let (mut w, _, old) = harvesting(AnimalKind::Sheep, false);
    let next = owned_sheep(&mut w, 104, 100);
    for player in 1..100_u16 {
        let x = 160 + i32::from(player % 10) * 8;
        let y = 160 + i32::from(player / 10) * 8;
        w.spawn_unit(PlayerId(player), at(x, y)).unwrap();
        for dy in 0..8 {
            w.spawn_animal(AnimalKind::Sheep, at(x + 1, y + dy))
                .unwrap();
        }
    }
    w.advance();
    assert_eq!(w.unit_count(), 100);
    assert_eq!(w.animals().count(), 794);
    assert_eq!(
        w.next_food_animal(
            PlayerId(0),
            TileCoord::new(101, 100),
            AnimalKind::Sheep,
            old
        )
        .unwrap()
        .id,
        next
    );
    let stats = w.wildlife_stats();
    assert!(stats.food_search_cells <= 16, "{stats:?}");
    assert_eq!(
        stats.food_search_candidates, 2,
        "only the spent source and its neighbour: {stats:?}"
    );
}

#[test]
fn continuation_replays_with_identical_jobs_positions_and_hashes() {
    for kind in [AnimalKind::Sheep, AnimalKind::Deer] {
        let make = || {
            let (mut w, worker, _) = harvesting(kind, false);
            let next = if kind == AnimalKind::Sheep {
                owned_sheep(&mut w, 104, 100)
            } else {
                w.spawn_animal(kind, at(104, 100)).unwrap()
            };
            (w, worker, next)
        };
        let (mut a, worker, next) = make();
        let (mut b, _, _) = make();
        for _ in 0..500 {
            a.advance();
            b.advance();
            assert_eq!(a.canonical_hash(), b.canonical_hash());
        }
        assert_eq!(a.hunting_target(worker), Some(next));
    }
}
