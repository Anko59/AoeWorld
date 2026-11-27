use super::*;
use crate::BuildingKind;
use aoe_core::{PlayerId, TileRect};

const OWNER: PlayerId = PlayerId(0);
const RIVAL: PlayerId = PlayerId(1);

fn at(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}

pub(super) fn setup() -> (GameWorld, EntityId, EntityId, EntityId) {
    let (mut world, _) = super::tests::fixture(128, &[TileCoord::new(50, 50)]);
    world.set_fog_disabled(true);
    let monk = world.spawn_kind(OWNER, UnitKind::Monk, at(50, 51)).unwrap();
    let relic = world.allocate_entity_id().unwrap();
    world.relics.ground.insert(relic, at(55, 51));
    let monastery = world
        .insert_building(
            OWNER,
            BuildingKind::Monastery,
            TileRect::from_xywh(60, 50, 3, 3),
            true,
        )
        .unwrap();
    (world, monk, relic, monastery)
}

pub(super) fn until(world: &mut GameWorld, done: impl Fn(&GameWorld) -> bool) {
    for _ in 0..400 {
        if done(world) {
            return;
        }
        world.advance();
    }
    assert!(done(world), "the ordered action never finished");
}

#[test]
fn only_owned_monks_pick_up_one_relic_and_death_drops_the_same_id() {
    let (mut world, monk, relic, _) = setup();
    let villager = world.spawn_villager(OWNER, at(55, 52)).unwrap();
    assert!(world.issue_pickup_relic(OWNER, villager, relic).is_err());
    assert!(world.issue_pickup_relic(RIVAL, monk, relic).is_err());
    assert!(world.issue_delete(OWNER, &[relic]).is_err());
    assert!(world.issue_attack(OWNER, monk, relic).is_err());
    world.issue_pickup_relic(OWNER, monk, relic).unwrap();
    assert_eq!(world.carried_relic(monk), None);
    until(&mut world, |w| w.carried_relic(monk) == Some(relic));
    assert_eq!(world.ground_relics().count(), 0);
    let second = world.allocate_entity_id().unwrap();
    world.relics.ground.insert(second, at(55, 53));
    assert!(world.issue_pickup_relic(OWNER, monk, second).is_err());
    let death = world.unit(monk).unwrap().position;
    world.issue_delete(OWNER, &[monk]).unwrap();
    assert_eq!(world.carried_relic(monk), None);
    assert!(world.ground_relics().any(|pair| pair == (relic, death)));
}

#[test]
fn a_replacement_move_cancels_pickup_even_when_it_targets_the_same_point() {
    let (mut world, monk, relic, _) = setup();
    world.issue_pickup_relic(OWNER, monk, relic).unwrap();
    world.issue_move(monk, at(55, 51)).unwrap();
    for _ in 0..200 {
        world.advance();
    }
    assert_eq!(world.carried_relic(monk), None);
}

#[test]
fn monastery_pays_one_gold_each_two_seconds_and_releases_relics_when_razed() {
    let (mut world, monk, relic, monastery) = setup();
    let gold = |w: &GameWorld| w.player_economy(OWNER).unwrap().stockpile.gold;
    let opening_gold = gold(&world);
    world.issue_pickup_relic(OWNER, monk, relic).unwrap();
    until(&mut world, |w| w.carried_relic(monk) == Some(relic));
    assert_eq!(gold(&world), opening_gold);
    assert!(world.issue_deposit_relic(RIVAL, monk, monastery).is_err());
    world.issue_deposit_relic(OWNER, monk, monastery).unwrap();
    until(&mut world, |w| w.monastery_relic_count(monastery) == 1);
    assert_eq!(world.carried_relic(monk), None);
    let interval = 2 * world.config().tick_hz;
    for _ in 1..interval {
        world.advance();
    }
    assert_eq!(gold(&world), opening_gold);
    world.advance();
    assert_eq!(gold(&world), opening_gold + 1);

    assert!(!crate::convertible_building(BuildingKind::Monastery));
    world.issue_delete(OWNER, &[monastery]).unwrap();
    assert_eq!(world.monastery_relic_count(monastery), 0);
    let (found, ground) = world.ground_relics().next().unwrap();
    assert_eq!(found, relic);
    assert!(world.tile_passable(ground.tile_floor()));
    for _ in 0..interval {
        world.advance();
    }
    assert_eq!(gold(&world), opening_gold + 1);
}

#[test]
fn relic_carry_reduces_monk_sight_and_disables_conversion() {
    let (mut world, monk, relic, _) = setup();
    let rival = world
        .spawn_kind(RIVAL, UnitKind::Militia, at(58, 51))
        .unwrap();
    let before = world.unit_observer(&world.unit(monk).unwrap()).unwrap().2;
    let pace = world.movement_speed(UnitKind::Monk, OWNER);
    world.issue_pickup_relic(OWNER, monk, relic).unwrap();
    until(&mut world, |w| w.carried_relic(monk) == Some(relic));
    assert_eq!(
        world.unit_observer(&world.unit(monk).unwrap()).unwrap().2,
        before - 2
    );
    assert_eq!(world.movement_speed(UnitKind::Monk, OWNER), pace);
    assert!(world.issue_attack(OWNER, monk, rival).is_err());
    world.issue_drop_relic(OWNER, monk).unwrap();
    assert_eq!(
        world.unit_observer(&world.unit(monk).unwrap()).unwrap().2,
        before
    );
    assert_eq!(world.ground_relics().next().unwrap().0, relic);
}

#[test]
fn carried_and_stored_relics_continue_after_serialized_restore() {
    let (mut world, monk, relic, monastery) = setup();
    world.issue_pickup_relic(OWNER, monk, relic).unwrap();
    until(&mut world, |w| w.carried_relic(monk) == Some(relic));
    world.issue_deposit_relic(OWNER, monk, monastery).unwrap();

    let restore = |world: &GameWorld| {
        let bytes = postcard::to_allocvec(&world.snapshot([0; 32]).unwrap()).unwrap();
        let snapshot = postcard::from_bytes(&bytes).unwrap();
        let mut restored = GameWorld::new(world.config()).unwrap();
        restored
            .restore_snapshot(&snapshot, [0; 32], &|| false)
            .unwrap();
        restored
    };
    let mut world = restore(&world);
    assert_eq!(world.carried_relic(monk), Some(relic));
    until(&mut world, |w| w.monastery_relic_count(monastery) == 1);
    let gold = |w: &GameWorld| w.player_economy(OWNER).unwrap().stockpile.gold;
    let opening = gold(&world);
    let interval = 2 * world.config().tick_hz;
    for _ in 0..interval / 2 {
        world.advance();
    }
    let mut world = restore(&world);
    assert_eq!(world.monastery_relic_count(monastery), 1);
    for _ in 1..interval / 2 {
        world.advance();
    }
    assert_eq!(gold(&world), opening);
    world.advance();
    assert_eq!(gold(&world), opening + 1);
}

#[test]
fn relic_collection_and_production_replay_across_worker_counts() {
    let run = |threads| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap()
            .install(|| {
                let (mut world, monk, relic, monastery) = setup();
                world.issue_pickup_relic(OWNER, monk, relic).unwrap();
                until(&mut world, |w| w.carried_relic(monk) == Some(relic));
                world.issue_deposit_relic(OWNER, monk, monastery).unwrap();
                for _ in 0..200 {
                    world.advance();
                }
                assert_eq!(world.monastery_relic_count(monastery), 1);
                world.canonical_hash()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn a_monastery_can_hold_ten_relics_and_ejecting_one_frees_a_place() {
    let (mut world, monk, first_relic, monastery) = setup();
    world.relics.ground.remove(&first_relic);
    for n in 1..=11 {
        let relic = world.allocate_entity_id().unwrap();
        world.relics.ground.insert(relic, at(59, 51));
        world.issue_pickup_relic(OWNER, monk, relic).unwrap();
        until(&mut world, |w| w.carried_relic(monk) == Some(relic));
        if n <= 10 {
            world.issue_garrison(OWNER, monk, monastery).unwrap();
            until(&mut world, |w| w.monastery_relic_count(monastery) == n);
        } else {
            assert_eq!(
                world.issue_garrison(OWNER, monk, monastery),
                Err(crate::EconomyError::GarrisonFull)
            );
            assert_eq!(world.carried_relic(monk), Some(relic));
        }
    }
    world.issue_eject_relic(OWNER, monastery).unwrap();
    assert_eq!(world.monastery_relic_count(monastery), 9);
    assert_eq!(world.ground_relics().count(), 1);
    world.issue_garrison(OWNER, monk, monastery).unwrap();
    until(&mut world, |w| w.monastery_relic_count(monastery) == 10);
}
