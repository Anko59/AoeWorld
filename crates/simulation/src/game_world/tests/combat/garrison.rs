use super::*;
use crate::{BuildingKind, CarriedResource, UnitKind};

fn shelter(kind: BuildingKind) -> (GameWorld, EntityId) {
    let mut world = world();
    world.set_fog_disabled(true);
    let side = kind.rules().side;
    let building = world
        .insert_building(
            PlayerId(0),
            kind,
            TileRect::from_xywh(100, 100, side, side),
            true,
        )
        .unwrap();
    (world, building)
}

fn enter(world: &mut GameWorld, building: EntityId, kind: UnitKind) -> EntityId {
    let unit = world.spawn_kind(PlayerId(0), kind, at(99, 100)).unwrap();
    world.issue_garrison(PlayerId(0), unit, building).unwrap();
    world.advance();
    assert!(
        world.unit(unit).is_none(),
        "garrisoned unit leaves map and lookup"
    );
    assert!(
        world
            .query(TileRect::from_xywh(95, 95, 15, 15))
            .0
            .iter()
            .all(|u| u.id != unit)
    );
    unit
}

#[test]
fn garrison_capacity_rules_and_population_are_authoritative() {
    for (kind, capacity) in [
        (BuildingKind::TownCenter, 15),
        (BuildingKind::WatchTower, 5),
        (BuildingKind::Castle, 20),
    ] {
        let (mut world, building) = shelter(kind);
        assert_eq!(kind.garrison_capacity(), capacity);
        assert!(!kind.admits(UnitKind::FishingShip));
        let foreign = world.spawn_villager(PlayerId(1), at(99, 100)).unwrap();
        assert_eq!(
            world.issue_garrison(PlayerId(0), foreign, building),
            Err(EconomyError::NotOwner)
        );
        let scout = world.spawn_unit(PlayerId(0), at(99, 100)).unwrap();
        if kind == BuildingKind::Castle {
            world.issue_garrison(PlayerId(0), scout, building).unwrap();
            world.advance();
        } else {
            assert_eq!(
                world.issue_garrison(PlayerId(0), scout, building),
                Err(EconomyError::WrongUnitKind)
            );
        }
        let occupied = world.building(building).unwrap().garrison.len();
        for i in occupied..usize::from(capacity) {
            enter(
                &mut world,
                building,
                [
                    UnitKind::Villager,
                    UnitKind::Militia,
                    UnitKind::Archer,
                    UnitKind::Skirmisher,
                    UnitKind::Spearman,
                    UnitKind::ManAtArms,
                ][i % 6],
            );
        }
        let extra = world.spawn_villager(PlayerId(0), at(99, 100)).unwrap();
        let before = world.canonical_hash();
        assert_eq!(
            world.issue_garrison(PlayerId(0), extra, building),
            Err(EconomyError::GarrisonFull)
        );
        assert_eq!(world.canonical_hash(), before, "rejected order is atomic");
        assert_eq!(
            world.building(building).unwrap().garrison.len(),
            usize::from(capacity)
        );
        assert_eq!(
            world.player_economy(PlayerId(0)).unwrap().population_used,
            capacity as u16 + if kind == BuildingKind::Castle { 1 } else { 2 }
        );
        assert_eq!(
            world.issue_ungarrison(PlayerId(1), building, None),
            Err(EconomyError::NotOwner)
        );
    }
}

#[test]
fn garrison_walks_to_the_building_cancels_jobs_and_reserves_space() {
    let (mut world, building) = shelter(BuildingKind::WatchTower);
    let unit = world.spawn_villager(PlayerId(0), at(94, 100)).unwrap();
    world.issue_garrison(PlayerId(0), unit, building).unwrap();
    assert!(world.unit(unit).is_some(), "no teleport on command");
    world.issue_stop(PlayerId(0), unit).unwrap();
    advance(&mut world, 200);
    assert!(world.unit(unit).is_some(), "stop cancels entry");
    world.issue_garrison(PlayerId(0), unit, building).unwrap();
    ticks_until(&mut world, 300, |w| w.unit(unit).is_none());
    assert_eq!(world.building(building).unwrap().garrison.len(), 1);
    let target = world.spawn_villager(PlayerId(1), at(110, 100)).unwrap();
    assert_eq!(
        world.issue_attack(PlayerId(1), target, unit),
        Err(EconomyError::InvalidTarget)
    );
}

#[test]
fn garrison_pending_entries_reserve_capacity_and_stop_releases_it() {
    let (mut world, building) = shelter(BuildingKind::WatchTower);
    let units: Vec<_> = (0..6)
        .map(|i| world.spawn_villager(PlayerId(0), at(90, 97 + i)).unwrap())
        .collect();
    for id in &units[..5] {
        world.issue_garrison(PlayerId(0), *id, building).unwrap();
    }
    assert_eq!(
        world.issue_garrison(PlayerId(0), units[5], building),
        Err(EconomyError::GarrisonFull)
    );
    world.issue_stop(PlayerId(0), units[0]).unwrap();
    world
        .issue_garrison(PlayerId(0), units[5], building)
        .unwrap();
    advance(&mut world, 500);
    assert_eq!(world.building(building).unwrap().garrison.len(), 5);
    assert!(world.unit(units[0]).is_some());
}

#[test]
fn garrison_replaced_by_farming_releases_the_reserved_space_immediately() {
    let (mut world, building) = shelter(BuildingKind::WatchTower);
    let farm = world
        .insert_building(
            PlayerId(0),
            BuildingKind::Farm,
            TileRect::from_xywh(88, 100, 3, 3),
            true,
        )
        .unwrap();
    let units: Vec<_> = (0..6)
        .map(|i| world.spawn_villager(PlayerId(0), at(94, 100 + i)).unwrap())
        .collect();
    for id in &units[..5] {
        world.issue_garrison(PlayerId(0), *id, building).unwrap();
    }
    assert_eq!(
        world.issue_garrison(PlayerId(0), units[5], building),
        Err(EconomyError::GarrisonFull)
    );
    world.issue_farm(PlayerId(0), units[0], farm).unwrap();
    assert_eq!(
        world.issue_garrison(PlayerId(0), units[5], building),
        Ok(()),
        "an accepted replacement job frees the old shelter reservation"
    );
    advance(&mut world, 500);
    assert!(world.unit(units[0]).is_some());
    assert_eq!(world.building(building).unwrap().garrison.len(), 5);
}

#[test]
fn garrison_heals_slowly_and_ejection_preserves_identity_inventory_and_population() {
    for (kind, interval) in [
        (BuildingKind::TownCenter, 200),
        (BuildingKind::Castle, 100),
        (BuildingKind::WatchTower, 200),
    ] {
        let (mut world, building) = shelter(kind);
        let unit = world.spawn_villager(PlayerId(0), at(99, 100)).unwrap();
        let index = world.lookup[&unit];
        world.units[index].state.hp = 10;
        world.units[index].state.carried = Some(CarriedResource {
            kind: aoe_map::ResourceKind::Wood,
            amount: 7,
        });
        world.issue_garrison(PlayerId(0), unit, building).unwrap();
        world.issue_garrison(PlayerId(0), unit, building).unwrap();
        world.advance();
        advance(&mut world, interval - 1);
        assert_eq!(world.building(building).unwrap().garrison[0].unit.hp, 10);
        world.advance();
        assert_eq!(world.building(building).unwrap().garrison[0].unit.hp, 11);
        advance(&mut world, interval * 30);
        assert_eq!(world.building(building).unwrap().garrison[0].unit.hp, 25);
        world
            .issue_ungarrison(PlayerId(0), building, Some(unit))
            .unwrap();
        let ejected = world.unit(unit).unwrap();
        assert_eq!((ejected.hp, ejected.max_hp), (25, 25));
        if kind == BuildingKind::TownCenter {
            assert!(ejected.carried.is_none());
            assert_eq!(
                world.player_economy(PlayerId(0)).unwrap().stockpile.wood,
                7,
                "Town Center entry deposits the load exactly once"
            );
        } else {
            assert_eq!(ejected.carried.unwrap().amount, 7);
        }
        assert!(world.tile_passable(ejected.position.tile_floor()));
        assert_eq!(
            world.player_economy(PlayerId(0)).unwrap().population_used,
            1
        );
        assert!(world.building(building).unwrap().garrison.is_empty());
    }
}

#[test]
fn garrison_destruction_ejects_every_occupant_and_replay_is_deterministic() {
    fn run() -> [u8; 32] {
        let (mut world, building) = shelter(BuildingKind::Castle);
        let ids: Vec<_> = [
            UnitKind::Villager,
            UnitKind::Scout,
            UnitKind::Militia,
            UnitKind::Archer,
        ]
        .into_iter()
        .map(|kind| enter(&mut world, building, kind))
        .collect();
        world.economy.buildings.get_mut(&building).unwrap().hp = 1;
        let enemy = world.spawn_militia(PlayerId(1), at(104, 100)).unwrap();
        world.issue_attack(PlayerId(1), enemy, building).unwrap();
        let mut destruction_changes = Vec::new();
        for _ in 0..100 {
            let changes = world.advance();
            if world.building(building).is_none() {
                destruction_changes = changes;
                break;
            }
        }
        assert!(world.building(building).is_none());
        for id in ids {
            let unit = world
                .unit(id)
                .expect("destruction releases the original unit");
            assert!(world.tile_passable(unit.position.tile_floor()));
            assert!(
                destruction_changes.iter().any(|unit| unit.id == id),
                "released identities must reach simulation tick consumers"
            );
        }
        assert_eq!(
            world.player_economy(PlayerId(0)).unwrap().population_used,
            4
        );
        world.canonical_hash()
    }
    assert_eq!(run(), run());
}

#[test]
fn garrison_destruction_releases_every_unit_even_with_only_one_open_ground_tile() {
    let (mut world, building) = shelter(BuildingKind::WatchTower);
    let units: Vec<_> = (0..5)
        .map(|_| enter(&mut world, building, UnitKind::Villager))
        .collect();
    for y in 96..105 {
        for x in 96..105 {
            world.economy.blocked_tiles.insert(TileCoord::new(x, y));
        }
    }
    world.destroy_building(building);
    for id in units {
        let unit = world
            .unit(id)
            .expect("destroyed shelter ejects even when exits are crowded");
        assert_eq!(unit.position.tile_floor(), TileCoord::new(100, 100));
    }
    assert!(world.economy.ejections.is_empty());
}

#[test]
fn garrison_arrows_use_projectiles_and_only_villagers_and_foot_archers_add_firepower() {
    for (kind, base) in [
        (BuildingKind::TownCenter, 0),
        (BuildingKind::Castle, 5),
        (BuildingKind::WatchTower, 1),
    ] {
        let (mut world, building) = shelter(kind);
        let target = world.spawn_militia(PlayerId(1), at(106, 101)).unwrap();
        // Hold the damage target still while measuring garrison firepower.
        world
            .issue_stance(PlayerId(1), &[target], aoe_core::CombatStance::NoAttack)
            .unwrap();
        world.advance();
        assert_eq!(
            world
                .projectiles()
                .filter(|p| p.attacker == building)
                .count(),
            base
        );
        advance(&mut world, 40);
        enter(&mut world, building, UnitKind::Militia);
        assert_eq!(world.building_arrow_count(building), base as u8);
        enter(&mut world, building, UnitKind::Villager);
        if kind == BuildingKind::Castle {
            enter(&mut world, building, UnitKind::Villager);
            enter(&mut world, building, UnitKind::Villager);
        }
        assert!(world.building_arrow_count(building) > base as u8);
        let count = world.building_arrow_count(building);
        enter(&mut world, building, UnitKind::Archer);
        assert!(world.building_arrow_count(building) >= count);
        if let Some(unit) = world.unit(target) {
            assert!(
                unit.hp < unit.max_hp || base == 0,
                "existing projectile impacts deal damage"
            );
        }
    }
}

#[test]
fn garrison_firepower_sums_before_flooring_and_respects_original_arrow_limits() {
    for (kind, expected) in [
        (BuildingKind::TownCenter, 10),
        (BuildingKind::WatchTower, 5),
        (BuildingKind::Castle, 14),
    ] {
        let (mut world, building) = shelter(kind);
        for _ in 0..kind.garrison_capacity() {
            enter(&mut world, building, UnitKind::Villager);
        }
        assert_eq!(world.building_arrow_count(building), expected);
    }
    let (mut world, building) = shelter(BuildingKind::Castle);
    for expected in [5, 5, 6, 6, 6, 7] {
        enter(&mut world, building, UnitKind::Archer);
        assert_eq!(
            world.building_arrow_count(building),
            expected,
            "floor(sum(archer DPS) / single Castle arrow DPS)"
        );
    }
}

#[test]
fn garrison_volley_launches_the_scaled_arrow_count_and_ejection_removes_the_bonus() {
    for (kind, full, empty) in [
        (BuildingKind::TownCenter, 10, 0),
        (BuildingKind::WatchTower, 5, 1),
        (BuildingKind::Castle, 14, 5),
    ] {
        let (mut world, building) = shelter(kind);
        for _ in 0..kind.garrison_capacity() {
            enter(&mut world, building, UnitKind::Villager);
        }
        world.spawn_militia(PlayerId(1), at(106, 101)).unwrap();
        let before = world.combat.next_projectile;
        world.advance();
        assert_eq!(world.combat.next_projectile - before, full);
        world.issue_ungarrison(PlayerId(0), building, None).unwrap();
        advance(&mut world, 40);
        world.spawn_militia(PlayerId(1), at(106, 101)).unwrap();
        let before = world.combat.next_projectile;
        advance(&mut world, 40);
        assert_eq!(world.combat.next_projectile - before, empty);
    }
}

#[test]
fn garrison_cycles_cannot_reset_town_center_reload() {
    let (mut world, building) = shelter(BuildingKind::TownCenter);
    world.spawn_militia(PlayerId(1), at(106, 101)).unwrap();
    let unit = enter(&mut world, building, UnitKind::Villager);
    let launched = world.combat.next_projectile;
    assert_eq!(launched, 1);
    world.issue_ungarrison(PlayerId(0), building, None).unwrap();
    world.issue_garrison(PlayerId(0), unit, building).unwrap();
    world.advance();
    assert_eq!(world.combat.next_projectile, launched);
    advance(&mut world, 39);
    assert_eq!(world.combat.next_projectile, launched + 1);
}

#[test]
fn garrison_research_upgrades_occupants_and_blocked_exits_never_drop_them() {
    let (mut world, building) = shelter(BuildingKind::TownCenter);
    let villager = enter(&mut world, building, UnitKind::Villager);
    let militia = enter(&mut world, building, UnitKind::Militia);
    world.complete_research(PlayerId(0), crate::Tech::FeudalAge, &mut Vec::new());
    world.complete_research(PlayerId(0), crate::Tech::Loom, &mut Vec::new());
    world.complete_research(PlayerId(0), crate::Tech::ManAtArms, &mut Vec::new());
    let footprint = world.building(building).unwrap().footprint;
    for y in 96..108 {
        for x in 96..108 {
            world.economy.blocked_tiles.insert(TileCoord::new(x, y));
        }
    }
    world.issue_ungarrison(PlayerId(0), building, None).unwrap();
    assert_eq!(world.building(building).unwrap().garrison.len(), 2);
    for y in 96..108 {
        for x in 96..108 {
            if !footprint.contains(TileCoord::new(x, y)) {
                world.economy.blocked_tiles.remove(&TileCoord::new(x, y));
            }
        }
    }
    world.issue_ungarrison(PlayerId(0), building, None).unwrap();
    assert_eq!(world.unit(villager).unwrap().max_hp, 40);
    assert_eq!(world.unit(militia).unwrap().kind, UnitKind::ManAtArms);
    assert_eq!(world.unit(militia).unwrap().max_hp, 45);
}

#[test]
fn castle_research_upgrades_sheltered_units_and_preserves_their_wounds() {
    let (mut world, building) = shelter(BuildingKind::TownCenter);
    let id = enter(&mut world, building, UnitKind::Militia);
    world.economy.buildings.get_mut(&building).unwrap().garrison[0]
        .unit
        .hp -= 7;
    for tech in [
        crate::Tech::FeudalAge,
        crate::Tech::ManAtArms,
        crate::Tech::CastleAge,
        crate::Tech::LongSwordsman,
    ] {
        world.complete_research(PlayerId(0), tech, &mut Vec::new());
    }
    world
        .issue_ungarrison(PlayerId(0), building, Some(id))
        .unwrap();
    let unit = world.unit(id).unwrap();
    assert_eq!(unit.kind, UnitKind::LongSwordsman);
    assert_eq!((unit.hp, unit.max_hp), (53, 60));
    assert!(BuildingKind::TownCenter.admits(UnitKind::LongSwordsman));
    assert!(BuildingKind::WatchTower.admits(UnitKind::Crossbowman));
}

#[test]
fn castle_shelters_never_admit_naval_units() {
    for kind in UnitKind::ALL.into_iter().filter(|k| k.is_naval()) {
        assert!(!BuildingKind::Castle.admits(kind), "{kind:?}");
    }
}
