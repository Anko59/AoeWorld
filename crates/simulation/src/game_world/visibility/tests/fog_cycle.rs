//! Turning fog on (a fog cycle, every restore) asks no player that sees
//! nothing about a building, and applies the observers' discs through the
//! vision batch in slices. Both leave exactly what asking everyone and
//! applying every disc alone left.
mod scene;
use crate::{BuildingKind, GameWorld, TileVisibility};
use aoe_core::{EntityId, PlayerId, Seed, TileCoord, WorldConfig};
use scene::{
    BLIND, assert_same, at, healers_awake, hidden, house, observers, one_by_one, scene, sheep,
    sleepers, watchers, world,
};
use std::collections::{BTreeMap, BTreeSet};

/// Times `step` asked one player whether it sees one building.
fn questions(w: &mut GameWorld, step: impl FnOnce(&mut GameWorld)) -> u64 {
    let before = w.vision.asked;
    step(w);
    w.vision.asked - before
}

/// A rebuild with live sight, or the one of a whole fog cycle.
fn rebuild(w: &mut GameWorld, cycle: bool) {
    if cycle {
        w.set_fog_disabled(true);
        w.set_fog_disabled(false);
    } else {
        w.rebuild_building_sight_index();
    }
}

/// Twenty houses of five players, each seen by its owner alone.
fn twenty_houses() -> GameWorld {
    let mut w = world();
    for n in 0..20_u16 {
        house(&mut w, n % 5, 40 + 10 * i32::from(n), 40, true);
    }
    w
}

/// Twenty scouts, eight houses and two owned sheep.
fn thirty_observers() -> GameWorld {
    let mut w = world();
    for n in 0..20_u16 {
        let x = 40 + 4 * i32::from(n);
        w.spawn_unit(PlayerId(n % 5), at(x, 40)).unwrap();
    }
    for n in 0..8_u16 {
        house(&mut w, n % 5, 40 + 4 * i32::from(n), 60, true);
    }
    sheep(&mut w, 60, 80, Some(0));
    sheep(&mut w, 64, 80, Some(3));
    w
}

/// Who sees each building, from the fog tiles alone.
fn seers(w: &GameWorld) -> BTreeMap<EntityId, BTreeSet<PlayerId>> {
    w.buildings()
        .map(|b| {
            let mut seers = BTreeSet::from([b.player]);
            if !b.is_unstarted() {
                for &player in w.vision.players.keys() {
                    if w.visibility_tiles(player, b.footprint).contains(&2) {
                        seers.insert(player);
                    }
                }
            }
            (b.id, seers)
        })
        .collect()
}

#[test]
fn turning_fog_on_asks_no_player_that_sees_nothing() {
    let mut w = twenty_houses();
    w.set_fog_disabled(true);
    let cycle = questions(&mut w, |w| w.set_fog_disabled(false));
    // All five see now, so the same rebuild does ask: the probe is wired.
    // Eighty questions are enough for a rebuild that skips owners.
    let live = questions(&mut w, |w| w.rebuild_building_sight_index());
    assert!((80..=100).contains(&live), "{live} questions with sight");
    assert_eq!(cycle, 0, "questions to players that see nothing");
}

#[test]
fn a_rebuild_with_live_sight_asks_only_the_players_who_see() {
    let mut w = twenty_houses();
    for player in [5, 6] {
        let scout = w.spawn_unit(PlayerId(player), at(43, 42)).unwrap();
        w.remove_unit(scout);
        assert_eq!(w.vision_storage(PlayerId(player)).1, 0);
    }
    assert_eq!(w.vision.players.len(), 7);
    let live = questions(&mut w, |w| w.rebuild_building_sight_index());
    assert!((80..=100).contains(&live), "{live} questions with sight");
    assert_eq!(watchers(&w), seers(&w));
}

#[test]
fn a_rebuild_with_live_sight_keeps_every_watcher() {
    let (mut w, _) = scene();
    let before = watchers(&w);
    w.rebuild_building_sight_index();
    let after = watchers(&w);
    assert_eq!(after, seers(&w));
    // A rebuild looks at a bridge's rectangle, moving sight at its deck.
    for b in w.buildings().filter(|b| b.bridge_span.is_none()) {
        assert_eq!(after[&b.id], before[&b.id], "{:?}", b.id);
        assert!(!after[&b.id].contains(&BLIND));
    }
    assert!(after.values().any(|seers| seers.len() == 1));
    assert!(after.values().any(|seers| seers.len() > 2));
    for id in after.keys() {
        assert!(w.vision.dirty_buildings.contains(id), "{id:?}");
    }
}

#[test]
fn private_foundations_stay_private_through_a_rebuild_and_a_fog_cycle() {
    let (owner, enemy) = (PlayerId(0), PlayerId(1));
    let mut w = world();
    w.spawn_unit(enemy, at(60, 64)).unwrap();
    let site = house(&mut w, 0, 62, 64, false);
    let tile = TileCoord::new(62, 64);
    for cycle in [false, true] {
        rebuild(&mut w, cycle);
        assert_eq!(w.visibility(enemy, tile), TileVisibility::Visible);
        assert_eq!(watchers(&w)[&site], BTreeSet::from([owner]));
        assert_eq!(w.building_at(tile), None);
        assert_eq!(w.building_at_for(enemy, tile), None);
        assert_eq!(w.building_at_for(owner, tile), Some(site));
        assert_eq!(w.vision.private_foundations.len(), 4);
        assert!(w.known_buildings(enemy).is_empty());
    }
    // Work makes it public: the enemy who sees the site watches at once.
    w.start_foundation(site).unwrap();
    assert_eq!(watchers(&w)[&site], BTreeSet::from([owner, enemy]));
    assert_eq!(w.building_at(tile), Some(site));
}

#[test]
fn angled_bridges_keep_their_deck_through_a_rebuild_and_a_fog_cycle() {
    let (mut w, _) = scene();
    // The owner, and the player whose scout sees the deck.
    let sure = BTreeSet::from([PlayerId(3), PlayerId(1)]);
    for cycle in [false, true] {
        rebuild(&mut w, cycle);
        let bridges = w
            .buildings()
            .filter_map(|b| b.bridge_span.map(|span| (b.id, span)))
            .collect::<Vec<_>>();
        assert_eq!(bridges.len(), 2);
        for (id, span) in bridges {
            let (rect, deck) = (span.footprint(), span.tiles());
            let beside = TileCoord::new(rect.min.x, rect.max.y - 1);
            for y in rect.min.y..rect.max.y {
                for x in rect.min.x..rect.max.x {
                    let tile = TileCoord::new(x, y);
                    let found = w.building_at(tile);
                    if deck.contains(&tile) {
                        assert_eq!(found, Some(id), "{tile:?}");
                        continue;
                    }
                    let kind = found.map(|id| w.building(id).unwrap().kind);
                    let wall = (tile == beside).then_some(BuildingKind::PalisadeWall);
                    assert_eq!(kind, wall, "{tile:?}");
                }
            }
            assert!(watchers(&w)[&id].is_superset(&sure), "{id:?}");
        }
    }
}

#[test]
fn a_fog_cycle_applies_its_thirty_observers_in_one_batch() {
    let mut w = thirty_observers();
    w.set_fog_disabled(true);
    let before = w.vision_batch_applications();
    w.set_fog_disabled(false);
    assert_eq!(observers(&w), 30);
    assert_eq!(w.vision_batch_applications() - before, 1);
}

#[test]
fn restoring_a_save_applies_its_sight_in_one_batch() {
    let saved = thirty_observers().snapshot([0; 32]).unwrap();
    let mut w = world();
    w.restore_snapshot(&saved, [0; 32], &|| false).unwrap();
    assert_eq!(observers(&w), 30);
    assert_eq!(w.vision_batch_applications(), 1);
}

/// The queue of a cycle is bounded, whatever the population: 65,536 changes.
/// A slice that ends exactly full leaves no empty batch behind it.
#[test]
fn a_fog_cycle_of_one_slice_takes_one_batch_and_one_more_change_takes_two() {
    let config = WorldConfig::new(1_024, 1_024, Seed(7)).unwrap();
    let mut w = GameWorld::with_population(config, 65_537, 0, 2).unwrap();
    for (changes, batches) in [(65_537, 2), (65_536, 1)] {
        // The population is built with fog off; the second cycle has one unit
        // fewer, so its only slice is exactly full.
        if w.unit_count() > changes {
            w.set_fog_disabled(true);
            let last = w.units().last().unwrap().id;
            w.remove_unit(last);
        }
        let before = w.vision_batch_applications();
        w.set_fog_disabled(false);
        let applied = w.vision_batch_applications() - before;
        assert_eq!(observers(&w), changes);
        assert_eq!(applied, batches, "{changes} changes");
    }
}

#[test]
fn a_batched_fog_cycle_equals_one_by_one_for_every_slice_and_thread_count() {
    let reference = one_by_one();
    let sight = &reference.vision.players[&PlayerId(0)];
    assert_eq!(sight.ghost_order.len(), 1, "one site revisited, one not");
    assert_eq!(sight.resources.len(), 1);
    assert!(sight.changed.len() > 1);
    assert!(reference.vision.players.contains_key(&PlayerId(6)));
    let crowd = |seers: &BTreeSet<PlayerId>| seers.len() > 2;
    assert!(watchers(&reference).values().any(crowd));
    for threads in [1, 8] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        for slice in [1, 7, 65_536] {
            pool.install(|| {
                let label = format!("slices of {slice}, {threads} threads");
                let (mut got, mut want) = (hidden(), one_by_one());
                let before = got.vision_batch_applications();
                got.switch_fog(false, slice);
                let applied = (got.vision_batch_applications() - before) as usize;
                // Not only equal to the reference, which shares the unit path:
                // `hidden` proved both asleep, so fog coming back woke them.
                let [militia, monk] = sleepers(&got);
                assert!(!got.combat.quiet.resting(militia), "{label}: militia");
                assert!(healers_awake(&mut got).contains(&monk), "{label}: monk");
                assert_same(&label, &mut got, &mut want);
                // A full last slice may or may not be followed by an empty one.
                let changes = observers(&got);
                let batches = changes.div_ceil(slice)..=changes / slice + 1;
                assert!(batches.contains(&applied), "{label}: {applied} batches");
            });
        }
    }
}

#[test]
fn sight_after_a_fog_cycle_applies_at_once() {
    let mut w = twenty_houses();
    rebuild(&mut w, true);
    assert!(w.vision.batch.is_none());
    w.spawn_unit(PlayerId(0), at(80, 80)).unwrap();
    let tile = TileCoord::new(84, 80);
    assert_eq!(w.visibility(PlayerId(0), tile), TileVisibility::Visible);
}
