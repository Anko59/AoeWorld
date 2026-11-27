//! What a seat knows of buildings. `GameWorld::known_buildings` also compares
//! every answer below with a scan of the whole world (test builds only).
use super::*;
use crate::BuildingKind;
use crate::game_world::tests::snapshot_roundtrip::roundtrip;
use aoe_core::{BridgeSpan, TileRect};

/// An observer no unit carries: it sees a disc for as long as a test wants.
const EYE: EntityId = EntityId(9000);

fn look(w: &mut GameWorld, player: u16, tile: TileCoord, los: u8) {
    w.update_observer(EYE, Some((PlayerId(player), tile, los)));
}

fn look_away(w: &mut GameWorld) {
    w.update_observer(EYE, None);
}

fn house(w: &mut GameWorld, player: u16, x: i32, y: i32) -> EntityId {
    let footprint = TileRect::from_xywh(x, y, 2, 2);
    w.insert_building(PlayerId(player), BuildingKind::House, footprint, true)
        .unwrap()
}

fn plan(w: &mut GameWorld, player: u16, x: i32, y: i32) -> EntityId {
    let footprint = TileRect::from_xywh(x, y, 2, 2);
    w.insert_building(PlayerId(player), BuildingKind::House, footprint, false)
        .unwrap()
}

fn hit(w: &mut GameWorld, id: EntityId) -> u16 {
    let building = w.building_mut(id).unwrap();
    building.hp -= 1;
    building.hp
}

/// The buildings `player` knows, with the hit points it knows them by.
fn known(w: &GameWorld, player: u16) -> Vec<(EntityId, u16)> {
    let known = w.known_buildings(PlayerId(player));
    known.iter().map(|b| (b.id, b.hp)).collect()
}

fn ids(w: &GameWorld, player: u16) -> Vec<EntityId> {
    known(w, player).into_iter().map(|(id, _)| id).collect()
}

/// The hit points `player`'s memory holds, whatever it sees now.
fn memory(w: &GameWorld, player: u16, id: EntityId) -> Option<u16> {
    w.remembered_building(PlayerId(player), id).map(|b| b.hp)
}

/// Live buildings read and footprint tiles asked for sight by one answer.
fn work(w: &GameWorld, player: u16) -> (usize, usize) {
    let (reads, probes) = (w.building_reads(), w.footprint_probes());
    let _ = w.known_from_memory(PlayerId(player));
    (w.building_reads() - reads, w.footprint_probes() - probes)
}

/// Every subscription and every building rebuild asks for this list while the
/// world is locked, so it must not read the buildings of the whole world.
#[test]
fn an_answer_reads_no_building_the_seat_does_not_know_and_none_memory_holds() {
    let mut w = world();
    w.spawn_villager(PlayerId(0), at(64, 64)).unwrap();
    let own = house(&mut w, 0, 20, 20);
    let seen = house(&mut w, 1, 66, 64);
    for n in 0..100 {
        house(&mut w, 1, 140 + n % 10 * 4, 140 + n / 10 * 4);
    }
    w.advance();
    assert_eq!(ids(&w, 0), [own, seen]);
    assert_eq!(work(&w, 0), (0, 0));
    assert_eq!(ids(&w, 1).len(), 101);
    assert_eq!(work(&w, 1), (0, 0));
    hit(&mut w, seen);
    let (reads, probes) = work(&w, 0);
    assert_eq!(reads, 1, "only the building that waits for the refresh");
    assert!((1..=4).contains(&probes), "its own footprint: {probes}");
}

/// A seat that explored the map pays nothing for what it no longer sees, and
/// what happens there stays unknown.
#[test]
fn many_memories_in_the_fog_are_answered_without_reads_or_sight() {
    let mut w = world();
    let mut houses = Vec::new();
    for n in 0..40 {
        houses.push(house(&mut w, 1, 100 + n % 8 * 4, 100 + n / 8 * 4));
    }
    look(&mut w, 0, TileCoord::new(115, 109), 30);
    w.remember_visible_buildings();
    look_away(&mut w);
    let seen = known(&w, 0);
    assert_eq!(ids(&w, 0), houses);
    for &id in &houses {
        hit(&mut w, id);
    }
    w.remember_visible_buildings();
    assert_eq!(known(&w, 0), seen, "damage in the fog");
    assert_ne!(known(&w, 1), seen, "the owner knows");
    assert_eq!(work(&w, 0), (0, 0));
}

#[test]
fn a_finished_house_seen_between_ticks_is_known_before_memory_takes_it() {
    let mut w = world();
    let seen = house(&mut w, 1, 66, 64);
    w.advance();
    assert!(ids(&w, 0).is_empty());
    w.spawn_villager(PlayerId(0), at(64, 64)).unwrap();
    assert_eq!(memory(&w, 0, seen), None);
    assert_eq!(ids(&w, 0), [seen]);
}

#[test]
fn a_change_waiting_for_the_memory_refresh_is_known_as_it_stands() {
    let mut w = world();
    w.spawn_villager(PlayerId(0), at(64, 64)).unwrap();
    let seen = house(&mut w, 1, 66, 64);
    w.advance();
    let hp = hit(&mut w, seen);
    assert_eq!(memory(&w, 0, seen), Some(hp + 1));
    let planned = plan(&mut w, 0, 20, 20);
    assert_eq!(memory(&w, 0, planned), None);
    let planned_hp = w.building(planned).unwrap().hp;
    assert_eq!(known(&w, 0), [(seen, hp), (planned, planned_hp)]);
    assert_eq!(ids(&w, 1), [seen], "a plan is its owner's alone");
}

/// Nothing marks a queue whose clock runs, so memory cannot be taken for it.
#[test]
fn a_running_queue_is_known_as_far_as_its_clock_ran() {
    let mut w = world();
    w.set_work_sleep(true);
    let owner = PlayerId(0);
    let center = w.place_town_center(owner, TileCoord::new(20, 20)).unwrap();
    w.economy.players.get_mut(&owner).unwrap().stockpile.food = 1_000;
    w.issue_train(owner, center, UnitKind::Villager).unwrap();
    w.advance();
    w.advance();
    let progress = |b: &crate::GameBuilding| b.training_queue[0].progress_ticks;
    let remembered = progress(w.remembered_building(owner, center).unwrap());
    w.economy.queues.training.tick += 3;
    assert_eq!(progress(&w.known_buildings(owner)[0]), remembered + 3);
}

/// The tech rescales foundations without marking them.
#[test]
fn a_foundation_rescaled_by_a_tech_is_known_rescaled() {
    let mut w = world();
    let owner = PlayerId(0);
    let site = plan(&mut w, 0, 20, 20);
    w.start_foundation(site).unwrap();
    w.remember_visible_buildings();
    let total = w.building(site).unwrap().work_total;
    w.complete_research(owner, Tech::TreadmillCrane, &mut Vec::new());
    let remembered = w.remembered_building(owner, site).unwrap();
    assert_eq!(remembered.work_total, total);
    let rescaled = w.known_buildings(owner)[0].work_total;
    assert_eq!(rescaled, total * 100 / 120);
}

/// A tile of a bridge's rectangle off its deck raises no sight event, so
/// memory misses what is seen from there; a subscription has always shown it.
#[test]
fn a_bridge_is_known_from_a_tile_of_its_rectangle_off_its_deck() {
    let mut w = world();
    let span = BridgeSpan::new(TileCoord::new(100, 100), TileCoord::new(105, 103)).unwrap();
    let (kind, site) = (BuildingKind::Bridge, span.footprint());
    let bridge = w
        .insert_building_shape(PlayerId(1), kind, site, true, Some(span))
        .unwrap();
    let corner = TileCoord::new(100, 103);
    assert!(site.contains(corner) && !span.contains(corner));
    // Never seen before.
    look(&mut w, 2, corner, 0);
    assert_eq!(memory(&w, 2, bridge), None);
    assert_eq!(ids(&w, 2), [bridge]);
    // Seen on its deck, then damaged in the fog.
    look(&mut w, 0, span.from, 0);
    w.remember_visible_buildings();
    look_away(&mut w);
    let hp = hit(&mut w, bridge);
    w.remember_visible_buildings();
    assert_eq!(known(&w, 0), [(bridge, hp + 1)]);
    look(&mut w, 0, corner, 0);
    assert_eq!(memory(&w, 0, bridge), Some(hp + 1));
    assert_eq!(known(&w, 0), [(bridge, hp)]);
    assert_eq!(work(&w, 1).0, 1, "a bridge off the axes is always read");
}

/// Orders never put a building on another one, but setup does and a save
/// restores finished ones as they stood: the tile then reports only the last.
#[test]
fn a_building_under_another_one_is_known_from_the_shared_tile() {
    let mut w = world();
    let under = house(&mut w, 1, 66, 64);
    let over = house(&mut w, 1, 67, 64);
    let (mut restored, report) = roundtrip(&w, world());
    assert_eq!(report.buildings_lost, 0);
    for w in [&mut w, &mut restored] {
        look(w, 0, TileCoord::new(67, 64), 0);
        assert_eq!(memory(w, 0, under), None);
        assert_eq!(ids(w, 0), [under, over]);
        w.remove_building(over);
        assert_eq!(ids(w, 0), [under], "its tile reports nothing now");
    }
}

#[test]
fn a_restored_seat_knows_what_it_knew_before_any_tick() {
    let mut w = world();
    w.spawn_villager(PlayerId(0), at(64, 64)).unwrap();
    let seen = house(&mut w, 1, 66, 64);
    let fogged = house(&mut w, 1, 100, 100);
    let gone = house(&mut w, 1, 104, 100);
    look(&mut w, 0, TileCoord::new(103, 101), 6);
    w.remember_visible_buildings();
    look_away(&mut w);
    let hp = hit(&mut w, fogged);
    w.remove_building(gone);
    w.advance();
    let saved = known(&w, 0);
    assert_eq!(ids(&w, 0), [seen, fogged, gone]);
    assert!(saved.contains(&(fogged, hp + 1)), "last sighting");
    let (restored, _) = roundtrip(&w, world());
    assert_eq!(known(&restored, 0), saved);
    assert_eq!(known(&restored, 1), known(&w, 1));
    assert_eq!(work(&restored, 0), (0, 0));
}

/// Conversion removes the building and sets the new one's hit points after
/// its first memory refresh, so only the pending change holds them.
#[test]
fn a_converted_building_is_known_as_it_stands_to_each_side() {
    let mut w = world();
    w.spawn_villager(PlayerId(0), at(64, 64)).unwrap();
    let old = house(&mut w, 1, 66, 64);
    look(&mut w, 2, TileCoord::new(66, 64), 1);
    w.remember_visible_buildings();
    look_away(&mut w);
    let hp = hit(&mut w, old);
    w.advance();
    w.convert_building(old, PlayerId(0));
    let new = w.buildings().next().unwrap().id;
    assert_ne!(new, old);
    assert_eq!(memory(&w, 0, new), Some(hp + 1));
    assert_eq!(known(&w, 0), [(new, hp)], "the new owner");
    assert!(ids(&w, 1).is_empty(), "the old owner lost it");
    assert_eq!(known(&w, 2), [(old, hp + 1)], "a last sighting");
}
