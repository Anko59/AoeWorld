//! Castle progression is pinned to the licensed DAT; no source pixels live here.
use super::*;
use crate::{Age, Tech};

fn rich_feudal() -> (GameWorld, EntityId) {
    let (mut w, _, tc) = village();
    let p = w.economy.players.get_mut(&PlayerId(0)).unwrap();
    p.age = Age::Feudal;
    p.researched = Tech::FeudalAge.bit();
    p.stockpile = crate::Stockpile {
        food: 10000,
        wood: 10000,
        gold: 10000,
        stone: 10000,
    };
    p.population_cap = 200;
    (w, tc)
}

#[test]
fn castle_age_requires_feudal_and_two_distinct_finished_feudal_buildings() {
    let (mut w, tc) = rich_feudal();
    assert_eq!(
        w.issue_research(PlayerId(0), tc, Tech::CastleAge),
        Err(EconomyError::TechRequirementMissing)
    );
    let at = w.building(tc).unwrap().footprint.min;
    for (i, kind) in [BuildingKind::Blacksmith, BuildingKind::Market]
        .into_iter()
        .enumerate()
    {
        w.insert_building(
            PlayerId(0),
            kind,
            aoe_core::TileRect::from_xywh(
                at.x + 10 + i as i32 * 6,
                at.y + 10,
                kind.rules().side,
                kind.rules().depth,
            ),
            true,
        )
        .unwrap();
    }
    let before = w.player_economy(PlayerId(0)).unwrap().stockpile;
    w.issue_research(PlayerId(0), tc, Tech::CastleAge).unwrap();
    let after = w.player_economy(PlayerId(0)).unwrap().stockpile;
    assert_eq!(
        (before.food - after.food, before.gold - after.gold),
        (800, 200)
    );
    let ticks = 160 * w.config().tick_hz;
    advance(&mut w, ticks - 1);
    assert_eq!(w.player_economy(PlayerId(0)).unwrap().age, Age::Feudal);
    advance(&mut w, 1);
    assert_eq!(w.player_economy(PlayerId(0)).unwrap().age, Age::Castle);
    assert!(
        w.is_feudal(PlayerId(0)),
        "Feudal unlocks remain available in Castle Age"
    );
}

#[test]
fn castle_roster_and_tech_prerequisites_are_authoritative() {
    let (mut w, _) = rich_feudal();
    let at = TileCoord::new(90, 90);
    let stable = w
        .insert_building(
            PlayerId(0),
            BuildingKind::Stable,
            aoe_core::TileRect::from_xywh(at.x, at.y, 3, 3),
            true,
        )
        .unwrap();
    assert_eq!(
        w.issue_train(PlayerId(0), stable, UnitKind::Knight),
        Err(EconomyError::RequiresCastleAge)
    );
    w.complete_research(PlayerId(0), Tech::CastleAge, &mut Vec::new());
    w.issue_train(PlayerId(0), stable, UnitKind::Knight)
        .unwrap();
    assert_eq!(
        w.building(stable).unwrap().training_queue[0].total_ticks,
        30 * u64::from(w.config().tick_hz)
    );
    assert_eq!(
        w.issue_research(PlayerId(0), stable, Tech::LightCavalry),
        Err(EconomyError::ResearchBusy)
    );
    w.issue_cancel_training(PlayerId(0), stable).unwrap();
    w.issue_research(PlayerId(0), stable, Tech::LightCavalry)
        .unwrap();
}

#[test]
fn castle_upgrades_existing_units_and_retains_damage() {
    let (mut w, _) = rich_feudal();
    w.complete_research(PlayerId(0), Tech::ManAtArms, &mut Vec::new());
    let id = w
        .spawn_kind(
            PlayerId(0),
            UnitKind::ManAtArms,
            WorldPosition::from_tile_center(TileCoord::new(90, 90)).unwrap(),
        )
        .unwrap();
    let index = w.lookup[&id];
    w.units[index].state.hp -= 7;
    w.complete_research(PlayerId(0), Tech::CastleAge, &mut Vec::new());
    w.complete_research(PlayerId(0), Tech::LongSwordsman, &mut Vec::new());
    let u = w.unit(id).unwrap();
    assert_eq!((u.kind, u.hp, u.max_hp), (UnitKind::LongSwordsman, 53, 60));
    assert_eq!(w.unit_stats(id).unwrap().attack, 9);
}

#[test]
fn castle_building_orders_are_age_gated_and_charge_exact_stone() {
    let (mut w, _) = rich_feudal();
    let worker = w.units().find(|u| u.kind == UnitKind::Villager).unwrap().id;
    let tile = TileCoord::new(100, 100);
    for kind in [
        BuildingKind::Castle,
        BuildingKind::Monastery,
        BuildingKind::University,
        BuildingKind::SiegeWorkshop,
    ] {
        let before = w.canonical_hash();
        assert_eq!(
            w.issue_build(PlayerId(0), &[worker], kind, tile, false),
            Err(EconomyError::RequiresCastleAge)
        );
        assert_eq!(w.canonical_hash(), before);
    }
    w.complete_research(PlayerId(0), Tech::CastleAge, &mut Vec::new());
    fixture::survey(&mut w, PlayerId(0), TileCoord::new(102, 102), 5);
    let stone = w.player_economy(PlayerId(0)).unwrap().stockpile.stone;
    let castle = w
        .issue_build(PlayerId(0), &[worker], BuildingKind::Castle, tile, false)
        .unwrap();
    assert_eq!(
        w.player_economy(PlayerId(0)).unwrap().stockpile.stone,
        stone - 650
    );
    assert_eq!(w.building(castle).unwrap().footprint.width(), 4);
    assert_eq!(w.building(castle).unwrap().max_hp, 4800);
}

#[test]
fn castle_economic_techs_stack_and_heavy_plow_adds_farmer_capacity() {
    let (mut w, _) = rich_feudal();
    for tech in [
        Tech::CastleAge,
        Tech::Wheelbarrow,
        Tech::HandCart,
        Tech::HorseCollar,
        Tech::HeavyPlow,
        Tech::DoubleBitAxe,
        Tech::BowSaw,
        Tech::GoldMining,
        Tech::GoldShaftMining,
    ] {
        w.complete_research(PlayerId(0), tech, &mut Vec::new());
    }
    assert_eq!(w.villager_capacity(PlayerId(0)), 20);
    assert_eq!(
        w.gather_rate(PlayerId(0), ResourceKind::Wood),
        (39 * 144, 100)
    );
    assert_eq!(
        w.gather_rate(PlayerId(0), ResourceKind::Gold),
        (38 * 13225, 10000)
    );
    let farm = w
        .insert_building(
            PlayerId(0),
            BuildingKind::Farm,
            aoe_core::TileRect::from_xywh(100, 100, 3, 3),
            true,
        )
        .unwrap();
    assert_eq!(w.building(farm).unwrap().food_remaining, Some(375));
    let worker = w.units().find(|u| u.kind == UnitKind::Villager).unwrap().id;
    w.issue_farm(PlayerId(0), worker, farm).unwrap();
    assert_eq!(w.gather_capacity(&w.unit(worker).unwrap(), true), 21);
}

#[test]
fn heavy_plow_farmer_gathers_and_deposits_the_full_extra_slot() {
    for (upgrades, capacity) in [
        (vec![], 10),
        (vec![Tech::HorseCollar, Tech::HeavyPlow], 11),
        (
            vec![
                Tech::HorseCollar,
                Tech::HeavyPlow,
                Tech::Wheelbarrow,
                Tech::HandCart,
            ],
            21,
        ),
    ] {
        let (mut w, _) = rich_feudal();
        w.complete_research(PlayerId(0), Tech::CastleAge, &mut Vec::new());
        for tech in upgrades {
            w.complete_research(PlayerId(0), tech, &mut Vec::new());
        }
        let farm = w
            .insert_building(
                PlayerId(0),
                BuildingKind::Farm,
                aoe_core::TileRect::from_xywh(70, 64, 3, 3),
                true,
            )
            .unwrap();
        let worker = w
            .spawn_villager(
                PlayerId(0),
                WorldPosition::from_tile_center(TileCoord::new(71, 65)).unwrap(),
            )
            .unwrap();
        w.issue_farm(PlayerId(0), worker, farm).unwrap();
        let food = w.player_economy(PlayerId(0)).unwrap().stockpile.food;
        let mut returning = false;
        let mut deposited = false;
        for _ in 0..2000 {
            w.advance();
            if w.unit(worker).unwrap().task == UnitTask::Returning {
                assert_eq!(w.unit(worker).unwrap().carried.unwrap().amount, capacity);
                returning = true;
            }
            if w.player_economy(PlayerId(0)).unwrap().stockpile.food > food {
                assert_eq!(
                    w.player_economy(PlayerId(0)).unwrap().stockpile.food - food,
                    u32::from(capacity)
                );
                deposited = true;
                break;
            }
        }
        assert!(
            returning && deposited,
            "farmer completes a full load of {capacity}"
        );
    }
}

#[test]
fn crossbow_upgrade_rewrites_queues_preserves_progress_and_changes_future_training() {
    let (mut w, _) = rich_feudal();
    let range = w
        .insert_building(
            PlayerId(0),
            BuildingKind::ArcheryRange,
            aoe_core::TileRect::from_xywh(90, 90, 3, 3),
            true,
        )
        .unwrap();
    w.insert_building(
        PlayerId(0),
        BuildingKind::House,
        aoe_core::TileRect::from_xywh(96, 90, 2, 2),
        true,
    )
    .unwrap();
    w.issue_train(PlayerId(0), range, UnitKind::Archer).unwrap();
    let before = w.building(range).unwrap().training_queue[0].total_ticks / 2;
    w.economy.buildings.get_mut(&range).unwrap().training_queue[0].progress_ticks = before;
    w.complete_research(PlayerId(0), Tech::CastleAge, &mut Vec::new());
    w.complete_research(PlayerId(0), Tech::Crossbowman, &mut Vec::new());
    let entry = &w.building(range).unwrap().training_queue[0];
    assert_eq!(entry.kind, UnitKind::Crossbowman);
    assert_eq!(entry.total_ticks, 27 * u64::from(w.config().tick_hz));
    assert_eq!(entry.progress_ticks, entry.total_ticks / 2);
    w.issue_train(PlayerId(0), range, UnitKind::Archer).unwrap();
    assert_eq!(
        w.building(range).unwrap().training_queue[1].kind,
        UnitKind::Crossbowman
    );
}

#[test]
fn duplicate_and_unfinished_feudal_sites_do_not_satisfy_castle_requirements() {
    let (mut w, tc) = rich_feudal();
    for (x, kind, complete) in [
        (90, BuildingKind::Blacksmith, true),
        (95, BuildingKind::Blacksmith, true),
        (100, BuildingKind::Market, false),
    ] {
        w.insert_building(
            PlayerId(0),
            kind,
            aoe_core::TileRect::from_xywh(x, 90, kind.rules().side, kind.rules().depth),
            complete,
        )
        .unwrap();
    }
    assert_eq!(
        w.issue_research(PlayerId(0), tc, Tech::CastleAge),
        Err(EconomyError::TechRequirementMissing)
    );
}

#[test]
fn a_completed_castle_provides_twenty_population_slots_from_the_dat() {
    let (mut w, _, _) = village();
    w.complete_research(PlayerId(0), Tech::FeudalAge, &mut Vec::new());
    w.complete_research(PlayerId(0), Tech::CastleAge, &mut Vec::new());
    let before = w.player_economy(PlayerId(0)).unwrap().population_cap;
    w.insert_building(
        PlayerId(0),
        BuildingKind::Castle,
        aoe_core::TileRect::from_xywh(100, 100, 4, 4),
        true,
    )
    .unwrap();
    assert_eq!(
        w.player_economy(PlayerId(0)).unwrap().population_cap,
        before + 20
    );
}

#[test]
fn castle_villagers_build_additional_town_centers_with_normal_placement_rules() {
    let (mut w, _) = rich_feudal();
    // Ordinary sight covers the whole site; construction still checks exploration.
    let worker = w
        .spawn_villager(
            PlayerId(0),
            WorldPosition::from_tile_center(TileCoord::new(99, 102)).unwrap(),
        )
        .unwrap();
    w.spawn_kind(
        PlayerId(0),
        UnitKind::Scout,
        WorldPosition::from_tile_center(TileCoord::new(104, 101)).unwrap(),
    )
    .unwrap();
    let at = TileCoord::new(100, 100);
    let before = w.canonical_hash();
    assert_eq!(
        w.issue_build(PlayerId(0), &[worker], BuildingKind::TownCenter, at, false),
        Err(EconomyError::RequiresCastleAge)
    );
    assert_eq!(w.canonical_hash(), before);
    w.complete_research(PlayerId(0), Tech::CastleAge, &mut Vec::new());
    let before = w.player_economy(PlayerId(0)).unwrap().stockpile;
    let id = w
        .issue_build(PlayerId(0), &[worker], BuildingKind::TownCenter, at, false)
        .unwrap();
    let after = w.player_economy(PlayerId(0)).unwrap().stockpile;
    assert_eq!(
        (before.wood - after.wood, before.stone - after.stone),
        (275, 100)
    );
    assert_eq!(
        w.building(id).unwrap().footprint,
        aoe_core::TileRect::from_xywh(100, 100, 4, 4)
    );
    let accepted = w.canonical_hash();
    assert!(
        w.issue_build(PlayerId(0), &[worker], BuildingKind::TownCenter, at, false)
            .is_err()
    );
    assert_eq!(w.canonical_hash(), accepted);
    advance(&mut w, 5000);
    assert!(w.building(id).unwrap().is_complete());
    assert_eq!(
        w.player_buildings_of(PlayerId(0), BuildingKind::TownCenter)
            .count(),
        2
    );
}
