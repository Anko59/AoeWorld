use super::manager::{active, pass};
use super::*;
use crate::{StewardSettings, StewardWait};
use aoe_map::ResourceKind;

fn town_center(w: &mut GameWorld) {
    w.insert_building(
        PlayerId(0),
        BuildingKind::TownCenter,
        aoe_core::TileRect::from_xywh(110, 70, 4, 4),
        true,
    )
    .unwrap();
}
fn houses(w: &mut GameWorld, count: i32) {
    for i in 0..count {
        w.insert_building(
            PlayerId(0),
            BuildingKind::House,
            aoe_core::TileRect::from_xywh(3 + (i % 70) * 3, 180 + (i / 70) * 3, 2, 2),
            true,
        )
        .unwrap();
    }
}
fn villager_orders(w: &GameWorld) -> usize {
    w.buildings()
        .flat_map(|b| b.training_queue.iter())
        .filter(|e| e.kind == UnitKind::Villager)
        .count()
}
fn managed(w: &GameWorld, steward: EntityId) -> usize {
    w.units()
        .filter(|u| w.steward_controller(u.id) == Some(steward))
        .count()
}
fn two_stewards() -> (GameWorld, EntityId, EntityId) {
    let (mut w, a_town_center) = setup();
    let b_town_center = w
        .insert_building(
            PlayerId(0),
            BuildingKind::TownCenter,
            aoe_core::TileRect::from_xywh(150, 90, 4, 4),
            true,
        )
        .unwrap();
    let a = trained(&mut w, a_town_center);
    w.issue_garrison(PlayerId(0), a, a_town_center).unwrap();
    advance(&mut w, 2);
    let b = trained(&mut w, b_town_center);
    w.issue_garrison(PlayerId(0), b, b_town_center).unwrap();
    advance(&mut w, 2);
    (w, a, b)
}
fn settings(weights: [u8; 4], cap: u8, training: bool) -> StewardSettings {
    StewardSettings {
        weights,
        cap,
        training,
    }
}

#[test]
fn settings_default_to_kingdom_mix_full_cap_and_training_off() {
    let (w, _, steward) = active();
    assert_eq!(
        w.steward_settings(steward),
        Some(settings([45, 30, 15, 10], 100, false))
    );
    assert_eq!(
        w.steward_favoured_resource(steward),
        Some(ResourceKind::Food)
    );
}

#[test]
fn settings_command_is_owner_only_and_refuses_bad_values_without_change() {
    let (mut w, _, steward) = active();
    let kept = w.steward_settings(steward).unwrap();
    let hash = w.canonical_hash();
    let good = settings([10, 20, 30, 40], 25, true);
    assert_eq!(
        w.issue_steward_settings(PlayerId(1), steward, good),
        Err(EconomyError::NotOwner)
    );
    assert_eq!(
        w.issue_steward_settings(PlayerId(0), EntityId(999_999), good),
        Err(EconomyError::UnknownEntity)
    );
    for bad in [
        settings([10, 20, 30, 40], 101, true),
        settings([0, 0, 0, 0], 25, true),
        settings([101, 0, 0, 0], 25, true),
    ] {
        assert_eq!(
            w.issue_steward_settings(PlayerId(0), steward, bad),
            Err(EconomyError::InvalidStewardSettings)
        );
    }
    assert_eq!(w.steward_settings(steward), Some(kept));
    assert_eq!(w.canonical_hash(), hash, "refusals must not mutate state");
    w.issue_steward_settings(PlayerId(0), steward, good)
        .unwrap();
    assert_eq!(w.steward_settings(steward), Some(good));
    assert_ne!(w.canonical_hash(), hash, "settings are canonical state");
}

#[test]
fn mix_weights_change_which_resource_the_steward_favours() {
    let (mut w, _, steward) = active();
    w.issue_steward_settings(PlayerId(0), steward, settings([1, 1, 100, 1], 100, false))
        .unwrap();
    assert_eq!(
        w.steward_favoured_resource(steward),
        Some(ResourceKind::Gold)
    );
    w.issue_steward_settings(PlayerId(0), steward, settings([1, 100, 1, 1], 100, false))
        .unwrap();
    assert_eq!(
        w.steward_favoured_resource(steward),
        Some(ResourceKind::Wood)
    );
}

#[test]
fn training_is_off_by_default_and_the_toggle_starts_villager_orders() {
    let (mut w, _, steward) = active();
    houses(&mut w, 14);
    town_center(&mut w);
    pass(&mut w);
    assert_eq!(villager_orders(&w), 0);
    assert_eq!(w.steward_status(steward).unwrap().wait, StewardWait::Paused);
    w.issue_steward_settings(PlayerId(0), steward, settings([45, 30, 15, 10], 100, true))
        .unwrap();
    pass(&mut w);
    assert!(villager_orders(&w) > 0);
}

#[test]
fn a_lowered_cap_sends_the_excess_to_the_nearest_steward_with_room() {
    let (mut w, a, b) = two_stewards();
    // Four farms inside B's circle, empty, give B work for the villagers it receives.
    for i in 0..4 {
        w.insert_building(
            PlayerId(0),
            BuildingKind::Farm,
            aoe_core::TileRect::from_xywh(140 + 4 * i, 70, 3, 3),
            true,
        )
        .unwrap();
    }
    houses(&mut w, 14);
    let mut farmers = Vec::new();
    for i in 0..40 {
        // Columns x<=116 keep all forty inside A's side of the split at x=122.
        let at = TileCoord::new(96 + 4 * (i % 6), 100 + 4 * (i / 6));
        let farm = w
            .insert_building(
                PlayerId(0),
                BuildingKind::Farm,
                aoe_core::TileRect::from_xywh(at.x, at.y, 3, 3),
                true,
            )
            .unwrap();
        let worker = w
            .spawn_villager(
                PlayerId(0),
                WorldPosition::from_tile_center(TileCoord::new(at.x + 3, at.y)).unwrap(),
            )
            .unwrap();
        w.issue_farm(PlayerId(0), worker, farm).unwrap();
        farmers.push(worker);
    }
    advance(&mut w, 40);
    pass(&mut w);
    let before = managed(&w, a);
    assert!(
        before > 25,
        "fixture: A must first manage more than the cap"
    );
    w.issue_steward_settings(PlayerId(0), a, settings([45, 30, 15, 10], 25, false))
        .unwrap();
    // Each domain may issue two orders per pass, so the excess leaves over a few passes.
    for _ in 0..20 {
        if managed(&w, a) <= 25 {
            break;
        }
        pass(&mut w);
    }
    assert_eq!(
        managed(&w, a),
        25,
        "over-cap villagers leave down to the cap"
    );
    advance(&mut w, 3_000);
    // Later departures from A's domain come from the existing territory and
    // release rules, so only the upper bound is asserted after the long run.
    assert!(managed(&w, a) <= 25);
    assert!(managed(&w, b) >= 1, "excess must reach the nearby Steward");
}

#[test]
fn dismiss_kills_only_idle_unmanaged_own_villagers_in_the_active_plot() {
    let (mut w, town_center, steward) = active();
    let spawn = |w: &mut GameWorld, player, x| {
        w.spawn_villager(
            player,
            WorldPosition::from_tile_center(TileCoord::new(x, 100)).unwrap(),
        )
        .unwrap()
    };
    let managed = spawn(&mut w, PlayerId(0), 100);
    let farm = w
        .insert_building(
            PlayerId(0),
            BuildingKind::Farm,
            aoe_core::TileRect::from_xywh(100, 104, 3, 3),
            true,
        )
        .unwrap();
    w.issue_farm(PlayerId(0), managed, farm).unwrap();
    pass(&mut w);
    assert_eq!(w.steward_controller(managed), Some(steward));
    let idle = spawn(&mut w, PlayerId(0), 102);
    let busy = spawn(&mut w, PlayerId(0), 104);
    let outside = spawn(&mut w, PlayerId(0), 220);
    let foreign = spawn(&mut w, PlayerId(1), 106);
    w.issue_move(
        busy,
        WorldPosition::from_tile_center(TileCoord::new(110, 100)).unwrap(),
    )
    .unwrap();
    let hash = w.canonical_hash();
    assert_eq!(
        w.issue_steward_dismiss(PlayerId(1), steward),
        Err(EconomyError::NotOwner)
    );
    assert_eq!(w.canonical_hash(), hash);
    w.issue_steward_dismiss(PlayerId(0), steward).unwrap();
    assert_eq!(w.unit(idle).unwrap().hp, 0);
    for id in [managed, busy, outside, foreign] {
        assert!(w.unit(id).unwrap().hp > 0);
    }
    w.issue_ungarrison(PlayerId(0), town_center, Some(steward))
        .unwrap();
    let idle = spawn(&mut w, PlayerId(0), 108);
    assert!(w.issue_steward_dismiss(PlayerId(0), steward).is_err());
    assert!(w.unit(idle).unwrap().hp > 0);
}

#[test]
fn dismiss_includes_stopped_unmanaged_workers_carrying_a_load() {
    let (mut w, _, steward) = active();
    let others: Vec<_> = w
        .units()
        .filter(|u| u.kind == UnitKind::Villager)
        .map(|u| u.id)
        .collect();
    for id in others {
        w.issue_move(
            id,
            WorldPosition::from_tile_center(TileCoord::new(220, 100)).unwrap(),
        )
        .unwrap();
        assert_eq!(w.unit(id).unwrap().task, UnitTask::Moving);
    }
    let worker = w
        .spawn_villager(
            PlayerId(0),
            WorldPosition::from_tile_center(TileCoord::new(100, 100)).unwrap(),
        )
        .unwrap();
    w.units[w.lookup[&worker]].state.carried = Some(crate::CarriedResource {
        kind: ResourceKind::Wood,
        amount: 10,
    });
    w.issue_stop(PlayerId(0), worker).unwrap();
    assert_eq!(w.unit(worker).unwrap().task, UnitTask::Carrying);
    assert_eq!(w.steward_controller(worker), None);
    let economy = w.player_economy(PlayerId(0)).unwrap();
    w.issue_steward_dismiss(PlayerId(0), steward).unwrap();
    assert_eq!(w.unit(worker).unwrap().hp, 0);
    let after = w.player_economy(PlayerId(0)).unwrap();
    assert_eq!(after.population_used, economy.population_used - 1);
    assert_eq!(
        after.stockpile, economy.stockpile,
        "dismissal does not deposit its load"
    );
}

fn transfer_fixture() -> (GameWorld, EntityId, EntityId, usize) {
    let (mut w, a, b) = two_stewards();
    let mut workers = Vec::new();
    for i in 0..4 {
        let farm = w
            .insert_building(
                PlayerId(0),
                BuildingKind::Farm,
                aoe_core::TileRect::from_xywh(96 + 4 * i, 110, 3, 3),
                true,
            )
            .unwrap();
        let worker = w
            .spawn_villager(
                PlayerId(0),
                WorldPosition::from_tile_center(TileCoord::new(99 + 4 * i, 110)).unwrap(),
            )
            .unwrap();
        w.issue_farm(PlayerId(0), worker, farm).unwrap();
        workers.push(worker);
    }
    pass(&mut w);
    assert!(
        workers
            .iter()
            .all(|id| w.steward_controller(*id) == Some(a))
    );
    let before = managed(&w, a);
    w.issue_steward_settings(PlayerId(0), b, settings([45, 30, 15, 10], 1, false))
        .unwrap();
    w.issue_steward_settings(PlayerId(0), a, settings([45, 30, 15, 10], 0, false))
        .unwrap();
    (w, a, b, before)
}

#[test]
fn a_destination_slot_stays_reserved_while_the_first_transfer_is_travelling() {
    let (mut w, a, b, before) = transfer_fixture();
    for _ in 0..3 {
        pass(&mut w);
        assert_eq!(managed(&w, b), 0, "fixture: first arrival still travelling");
        assert_eq!(
            managed(&w, a),
            before - 1,
            "one free slot permits only one departure across passes"
        );
    }
}

#[test]
fn a_transfer_slot_is_reserved_until_arrival_but_released_on_owner_redirect_or_death() {
    let (mut w, a, b, before) = transfer_fixture();
    let original: Vec<_> = w
        .units()
        .filter(|u| w.steward_controller(u.id) == Some(a))
        .map(|u| u.id)
        .collect();
    pass(&mut w);
    let first = *original
        .iter()
        .find(|id| w.steward_controller(**id).is_none())
        .unwrap();
    pass(&mut w);
    assert_eq!(
        managed(&w, a),
        before - 1,
        "F3: the travelling arrival reserves the only slot"
    );
    assert_eq!(managed(&w, b), 0);
    w.issue_move(
        first,
        WorldPosition::from_tile_center(TileCoord::new(20, 20)).unwrap(),
    )
    .unwrap();
    pass(&mut w);
    assert_eq!(
        managed(&w, a),
        before - 2,
        "an owner redirect cancels the reservation even while moving"
    );
    let second = *original
        .iter()
        .find(|id| **id != first && w.steward_controller(**id).is_none())
        .unwrap();
    w.kill_unit(w.lookup[&second]);
    pass(&mut w);
    assert_eq!(managed(&w, a), before - 3, "a dead arrival frees the slot");
}

#[test]
fn arrival_and_adoption_do_not_open_a_second_transfer_into_the_same_slot() {
    let (mut w, a, b, before) = transfer_fixture();
    w.insert_building(
        PlayerId(0),
        BuildingKind::Farm,
        aoe_core::TileRect::from_xywh(140, 70, 3, 3),
        true,
    )
    .unwrap();
    for _ in 0..150 {
        pass(&mut w);
        assert_eq!(
            managed(&w, a),
            before - 1,
            "arrival must not release the slot before adoption"
        );
        if managed(&w, b) == 1 {
            pass(&mut w);
            assert_eq!(managed(&w, a), before - 1);
            return;
        }
    }
    panic!("the reserved worker must arrive and be adopted");
}
