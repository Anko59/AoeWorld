//! Building effects: hit points, armour, towers, construction, training,
//! garrison healing and the Market.
use super::*;
use crate::{MARKET_RULES, building_max_hp};

fn hp(w: &GameWorld, id: EntityId) -> (u16, u16) {
    let b = w.building(id).unwrap();
    (b.hp, b.max_hp)
}

#[test]
fn architecture_adds_a_tenth_of_hp_to_existing_and_new_buildings() {
    let (mut w, _) = castle_player();
    let barracks = place(&mut w, BuildingKind::Barracks, 0);
    let base = hp(&w, barracks).1;
    learn(&mut w, Tech::Masonry);
    learn(&mut w, Tech::Architecture);
    assert_eq!(hp(&w, barracks), (base + base / 5, base + base / 5));
    let fresh = place(&mut w, BuildingKind::Barracks, 1);
    assert_eq!(hp(&w, fresh).1, base + base / 5);
    let wall = w
        .insert_building(
            PlayerId(0),
            BuildingKind::StoneWall,
            aoe_core::TileRect::from_xywh(60, 60, 1, 1),
            true,
        )
        .unwrap();
    assert_eq!(
        hp(&w, wall).1,
        BuildingKind::StoneWall.age_stats(Age::Castle).0,
        "walls are outside Masonry and Architecture"
    );
}

#[test]
fn architecture_adds_one_armour_so_a_blow_does_less() {
    let (mut w, _) = castle_player();
    let barracks = place(&mut w, BuildingKind::Barracks, 0);
    let attacker = w
        .spawn_kind(PlayerId(1), UnitKind::Militia, tile(110, 110))
        .unwrap();
    let max = hp(&w, barracks).1;
    let dealt = |w: &mut GameWorld| {
        let before = w.building(barracks).unwrap().hp;
        w.resolve_hit(
            (attacker, PlayerId(1)),
            barracks,
            60,
            0,
            crate::DamageKind::Melee,
            &mut Vec::new(),
        );
        before - w.building(barracks).unwrap().hp
    };
    let plain = dealt(&mut w);
    let (mut w2, _) = castle_player();
    let barracks2 = place(&mut w2, BuildingKind::Barracks, 0);
    learn(&mut w2, Tech::Architecture);
    let attacker2 = w2
        .spawn_kind(PlayerId(1), UnitKind::Militia, tile(110, 110))
        .unwrap();
    let before = w2.building(barracks2).unwrap().hp;
    w2.resolve_hit(
        (attacker2, PlayerId(1)),
        barracks2,
        60,
        0,
        crate::DamageKind::Melee,
        &mut Vec::new(),
    );
    let reduced = before - w2.building(barracks2).unwrap().hp;
    assert!(max > 0 && reduced < plain, "{reduced} < {plain}");
}

#[test]
fn guard_tower_and_keep_upgrade_towers_in_place() {
    let (mut w, _) = castle_player();
    let tower = place(&mut w, BuildingKind::WatchTower, 0);
    let (hp0, base_attack) = (
        hp(&w, tower).1,
        w.building_attack(PlayerId(0), BuildingKind::WatchTower)
            .unwrap()
            .attack,
    );
    learn(&mut w, Tech::GuardTower);
    assert_eq!(hp(&w, tower), (hp0 + 650, hp0 + 650));
    assert_eq!(
        w.building_attack(PlayerId(0), BuildingKind::WatchTower)
            .unwrap()
            .attack,
        base_attack + 2
    );
    learn(&mut w, Tech::Keep);
    assert_eq!(hp(&w, tower).1, hp0 + 1400);
    assert_eq!(
        w.building_attack(PlayerId(0), BuildingKind::WatchTower)
            .unwrap()
            .attack,
        base_attack + 3
    );
    let second = place(&mut w, BuildingKind::WatchTower, 1);
    assert_eq!(hp(&w, second).1, hp0 + 1400);
    // Other players' towers are unchanged.
    assert_eq!(
        w.building_attack(PlayerId(1), BuildingKind::WatchTower)
            .unwrap()
            .attack,
        base_attack
    );
}

#[test]
fn arrowslits_add_attack_by_tower_level() {
    let (mut w, _) = castle_player();
    let base = w
        .building_attack(PlayerId(0), BuildingKind::WatchTower)
        .unwrap()
        .attack;
    learn(&mut w, Tech::Arrowslits);
    assert_eq!(
        w.building_attack(PlayerId(0), BuildingKind::WatchTower)
            .unwrap()
            .attack,
        base + 1
    );
    learn(&mut w, Tech::GuardTower);
    assert_eq!(
        w.building_attack(PlayerId(0), BuildingKind::WatchTower)
            .unwrap()
            .attack,
        base + 2 + 2
    );
    learn(&mut w, Tech::Keep);
    assert_eq!(
        w.building_attack(PlayerId(0), BuildingKind::WatchTower)
            .unwrap()
            .attack,
        base + 3 + 3
    );
    let castle = w
        .building_attack(PlayerId(0), BuildingKind::Castle)
        .unwrap()
        .attack;
    assert_eq!(
        castle,
        crate::building_combat(BuildingKind::Castle)
            .attack
            .unwrap()
            .attack
    );
}

#[test]
fn heated_shot_adds_attack_against_ships_only() {
    let (mut w, _) = castle_player();
    // The flat fixture has no water: relabel a scout as a ship.
    let target = w
        .spawn_kind(PlayerId(1), UnitKind::Scout, tile(20, 20))
        .unwrap();
    let at = w.lookup[&target];
    w.units[at].state.kind = UnitKind::Galley;
    let villager = w
        .spawn_kind(PlayerId(1), UnitKind::Villager, tile(110, 110))
        .unwrap();
    assert_eq!(
        w.heated_shot_bonus(PlayerId(0), BuildingKind::WatchTower, 8, target),
        0
    );
    learn(&mut w, Tech::HeatedShot);
    assert_eq!(
        w.heated_shot_bonus(PlayerId(0), BuildingKind::WatchTower, 8, target),
        10
    );
    assert_eq!(
        w.heated_shot_bonus(PlayerId(0), BuildingKind::Castle, 8, target),
        4
    );
    assert_eq!(
        w.heated_shot_bonus(PlayerId(0), BuildingKind::Castle, 8, villager),
        0
    );
}

#[test]
fn fortified_wall_and_hoardings_add_hit_points_and_armour() {
    let (mut w, _) = castle_player();
    let wall = w
        .insert_building(
            PlayerId(0),
            BuildingKind::StoneWall,
            aoe_core::TileRect::from_xywh(60, 60, 1, 1),
            true,
        )
        .unwrap();
    let castle = place(&mut w, BuildingKind::Castle, 0);
    let (wall0, castle0) = (hp(&w, wall).1, hp(&w, castle).1);
    learn(&mut w, Tech::FortifiedWall);
    learn(&mut w, Tech::Hoardings);
    assert_eq!(hp(&w, wall).1, wall0 + 1920);
    assert_eq!(hp(&w, castle).1, castle0 + 1000);
    assert_eq!(
        crate::game_world::research::building_armour_bonus(BuildingKind::StoneWall, researched(&w)),
        (4, 2)
    );
    assert_eq!(
        building_max_hp(BuildingKind::StoneWall, Age::Castle, 0),
        wall0
    );
}

#[test]
fn treadmill_crane_builds_a_fifth_faster() {
    let (mut w, _) = castle_player();
    let plain = place_foundation(&mut w, 0);
    let total = w.building(plain).unwrap().work_total;
    learn(&mut w, Tech::TreadmillCrane);
    assert_eq!(w.building(plain).unwrap().work_total, total * 100 / 120);
    let fresh = place_foundation(&mut w, 1);
    assert_eq!(w.building(fresh).unwrap().work_total, total * 100 / 120);
}

fn place_foundation(w: &mut GameWorld, slot: i32) -> EntityId {
    w.insert_building(
        PlayerId(0),
        BuildingKind::Barracks,
        aoe_core::TileRect::from_xywh(120 + slot * 8, 150, 3, 3),
        false,
    )
    .unwrap()
}

#[test]
fn conscription_shortens_military_training_but_not_siege_or_villagers() {
    let (mut w, tc) = castle_player();
    let barracks = place(&mut w, BuildingKind::Barracks, 0);
    let workshop = place(&mut w, BuildingKind::SiegeWorkshop, 1);
    w.issue_train(PlayerId(0), barracks, UnitKind::Militia)
        .unwrap();
    let queued = w.building(barracks).unwrap().training_queue[0].total_ticks;
    learn(&mut w, Tech::Conscription);
    assert_eq!(
        w.building(barracks).unwrap().training_queue[0].total_ticks,
        queued * 100 / 133
    );
    w.issue_train(PlayerId(0), barracks, UnitKind::Militia)
        .unwrap();
    assert_eq!(
        w.building(barracks).unwrap().training_queue[1].total_ticks,
        queued * 100 / 133
    );
    w.issue_train(PlayerId(0), workshop, UnitKind::BatteringRam)
        .unwrap();
    let ram = crate::unit_combat(UnitKind::BatteringRam)
        .training
        .unwrap()
        .seconds;
    assert_eq!(
        w.building(workshop).unwrap().training_queue[0].total_ticks,
        u64::from(ram) * u64::from(w.config().tick_hz)
    );
    w.issue_train(PlayerId(0), tc, UnitKind::Villager).unwrap();
    assert_eq!(
        w.building(tc).unwrap().training_queue[0].total_ticks,
        u64::from(crate::VILLAGE_RULES.villager_training_seconds) * u64::from(w.config().tick_hz)
    );
}

#[test]
fn herbal_medicine_heals_garrisoned_units_six_times_faster() {
    let (mut w, _) = castle_player();
    let ticks = |w: &GameWorld| w.garrison_heal_ticks(PlayerId(0), BuildingKind::Barracks);
    let base = ticks(&w);
    learn(&mut w, Tech::HerbalMedicine);
    assert_eq!(ticks(&w), base / 6);
}

#[test]
fn guilds_halve_the_market_commission() {
    let (mut w, _) = castle_player();
    let market = place(&mut w, BuildingKind::Market, 0);
    let price = w.player_economy(PlayerId(0)).unwrap().market_prices.food;
    let spend = |w: &mut GameWorld| {
        let before = w.player_economy(PlayerId(0)).unwrap().stockpile.gold;
        w.issue_market_buy(PlayerId(0), market, aoe_map::ResourceKind::Food)
            .unwrap();
        before - w.player_economy(PlayerId(0)).unwrap().stockpile.gold
    };
    assert_eq!(spend(&mut w), MARKET_RULES.buy_cost(price));
    learn(&mut w, Tech::Guilds);
    let price = w.player_economy(PlayerId(0)).unwrap().market_prices.food;
    assert_eq!(spend(&mut w), u32::from(price) * 115 / 100);
}

#[test]
fn banking_and_coinage_set_the_tribute_fee() {
    let (mut w, _) = castle_player();
    assert_eq!(crate::tribute_fee_permyriad(researched(&w)), 3000);
    learn(&mut w, Tech::Coinage);
    assert_eq!(crate::tribute_fee_permyriad(researched(&w)), 2000);
    learn(&mut w, Tech::Banking);
    assert_eq!(crate::tribute_fee_permyriad(researched(&w)), 0);
}
