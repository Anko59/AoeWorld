//! Imperial Age progression, unit lines and techs are pinned to the licensed
//! DE DAT (docs/economy/imperial.md); no source pixels live here.
use super::*;
use crate::{Age, TECH_RULES, Tech};

fn rich_castle() -> (GameWorld, EntityId) {
    let (mut w, _, tc) = village();
    let p = w.economy.players.get_mut(&PlayerId(0)).unwrap();
    p.age = Age::Castle;
    p.researched = Tech::FeudalAge.bit() | Tech::CastleAge.bit();
    p.stockpile = crate::Stockpile {
        food: 20000,
        wood: 20000,
        gold: 20000,
        stone: 20000,
    };
    for i in 0..5 {
        w.insert_building(
            PlayerId(0),
            BuildingKind::House,
            TileRect::from_xywh(10 + 3 * (i % 10), 10 + 3 * (i / 10), 2, 2),
            true,
        )
        .unwrap();
    }
    (w, tc)
}

fn imperial(w: &mut GameWorld) {
    w.complete_research(PlayerId(0), Tech::ImperialAge, &mut Vec::new());
}

fn place(w: &mut GameWorld, kind: BuildingKind, slot: i32) -> EntityId {
    w.insert_building(
        PlayerId(0),
        kind,
        aoe_core::TileRect::from_xywh(
            120 + slot % 4 * 8,
            120 + slot / 4 * 8,
            kind.rules().side,
            kind.rules().depth,
        ),
        true,
    )
    .unwrap()
}

fn spawn(w: &mut GameWorld, kind: UnitKind) -> EntityId {
    w.spawn_kind(
        PlayerId(0),
        kind,
        WorldPosition::from_tile_center(TileCoord::new(90, 90)).unwrap(),
    )
    .unwrap()
}

#[test]
fn imperial_age_needs_castle_age_and_two_distinct_castle_age_buildings() {
    let (mut w, tc) = rich_castle();
    assert_eq!(
        w.issue_research(PlayerId(0), tc, Tech::ImperialAge),
        Err(EconomyError::TechRequirementMissing)
    );
    // Duplicates and Towers do not count; neither does a second University.
    place(&mut w, BuildingKind::University, 0);
    place(&mut w, BuildingKind::University, 1);
    place(&mut w, BuildingKind::WatchTower, 2);
    assert_eq!(
        w.issue_research(PlayerId(0), tc, Tech::ImperialAge),
        Err(EconomyError::TechRequirementMissing)
    );
    place(&mut w, BuildingKind::SiegeWorkshop, 3);
    let before = w.player_economy(PlayerId(0)).unwrap().stockpile;
    w.issue_research(PlayerId(0), tc, Tech::ImperialAge)
        .unwrap();
    let after = w.player_economy(PlayerId(0)).unwrap().stockpile;
    assert_eq!(
        (before.food - after.food, before.gold - after.gold),
        (1000, 800)
    );
    let ticks = 190 * w.config().tick_hz;
    advance(&mut w, ticks - 1);
    assert_eq!(w.player_economy(PlayerId(0)).unwrap().age, Age::Castle);
    advance(&mut w, 1);
    assert_eq!(w.player_economy(PlayerId(0)).unwrap().age, Age::Imperial);
    assert!(w.is_feudal(PlayerId(0)), "earlier unlocks remain available");
    assert!(w.has_researched(PlayerId(0), Tech::ImperialAge));
}

#[test]
fn imperial_age_is_refused_before_castle_age_and_cancelling_refunds_it() {
    let (mut w, _, tc) = village();
    let p = w.economy.players.get_mut(&PlayerId(0)).unwrap();
    p.age = Age::Feudal;
    p.researched = Tech::FeudalAge.bit();
    p.stockpile = crate::Stockpile {
        food: 5000,
        wood: 5000,
        gold: 5000,
        stone: 5000,
    };
    assert_eq!(
        w.issue_research(PlayerId(0), tc, Tech::ImperialAge),
        Err(EconomyError::RequiresCastleAge)
    );
    let (mut w, tc) = rich_castle();
    place(&mut w, BuildingKind::University, 0);
    place(&mut w, BuildingKind::Monastery, 1);
    let before = w.player_economy(PlayerId(0)).unwrap().stockpile;
    w.issue_research(PlayerId(0), tc, Tech::ImperialAge)
        .unwrap();
    w.issue_cancel_research(PlayerId(0), tc).unwrap();
    assert_eq!(w.player_economy(PlayerId(0)).unwrap().stockpile, before);
}

#[test]
fn imperial_techs_need_imperial_age_and_their_castle_age_predecessor() {
    let (mut w, _) = rich_castle();
    let barracks = place(&mut w, BuildingKind::Barracks, 0);
    assert_eq!(
        w.issue_research(PlayerId(0), barracks, Tech::TwoHandedSwordsman),
        Err(EconomyError::RequiresImperialAge)
    );
    imperial(&mut w);
    assert_eq!(
        w.issue_research(PlayerId(0), barracks, Tech::TwoHandedSwordsman),
        Err(EconomyError::TechRequirementMissing),
        "Long Swordsman comes first"
    );
    w.complete_research(PlayerId(0), Tech::ManAtArms, &mut Vec::new());
    w.complete_research(PlayerId(0), Tech::LongSwordsman, &mut Vec::new());
    assert_eq!(
        w.issue_research(PlayerId(0), barracks, Tech::Champion),
        Err(EconomyError::TechRequirementMissing)
    );
    w.issue_research(PlayerId(0), barracks, Tech::TwoHandedSwordsman)
        .unwrap();
}

#[test]
fn imperial_rules_match_the_licensed_dat_costs_and_times() {
    let paid = |t: Tech| {
        let r = t.rules();
        (
            r.cost.food,
            r.cost.wood,
            r.cost.gold,
            r.cost.stone,
            r.seconds,
        )
    };
    for (tech, expected) in [
        (Tech::ImperialAge, (1000, 0, 800, 0, 190)),
        (Tech::TwoHandedSwordsman, (200, 0, 100, 0, 45)),
        (Tech::Champion, (650, 0, 350, 0, 70)),
        (Tech::Halberdier, (300, 0, 600, 0, 50)),
        (Tech::Arbalester, (450, 0, 350, 0, 50)),
        (Tech::Cavalier, (300, 0, 300, 0, 80)),
        (Tech::Paladin, (1300, 0, 750, 0, 170)),
        (Tech::Hussar, (500, 0, 600, 0, 50)),
        (Tech::CappedRam, (300, 0, 0, 0, 50)),
        (Tech::SiegeRam, (1000, 0, 0, 0, 75)),
        (Tech::Onager, (800, 0, 500, 0, 75)),
        (Tech::BlastFurnace, (275, 0, 225, 0, 100)),
        (Tech::PlateMailArmor, (300, 0, 150, 0, 70)),
        (Tech::PlateBardingArmor, (350, 0, 200, 0, 75)),
        (Tech::Bracer, (300, 0, 200, 0, 40)),
        (Tech::RingArcherArmor, (250, 0, 250, 0, 70)),
        (Tech::CropRotation, (250, 250, 0, 0, 70)),
        (Tech::TwoManSaw, (300, 200, 0, 0, 100)),
        (Tech::StoneShaftMining, (175, 75, 0, 0, 75)),
    ] {
        assert_eq!(paid(tech), expected, "{tech:?}");
        assert_eq!(TECH_RULES[tech as usize].tech, tech);
    }
}

#[test]
fn each_line_trains_its_newest_identity_and_upgrades_living_units() {
    let (mut w, _) = rich_castle();
    imperial(&mut w);
    for tech in [
        Tech::ManAtArms,
        Tech::LongSwordsman,
        Tech::TwoHandedSwordsman,
    ] {
        w.complete_research(PlayerId(0), tech, &mut Vec::new());
    }
    let barracks = place(&mut w, BuildingKind::Barracks, 0);
    w.issue_train(PlayerId(0), barracks, UnitKind::Militia)
        .unwrap();
    assert_eq!(
        w.building(barracks).unwrap().training_queue[0].kind,
        UnitKind::TwoHandedSwordsman
    );
    let id = spawn(&mut w, UnitKind::TwoHandedSwordsman);
    let index = w.lookup[&id];
    w.units[index].state.hp -= 10;
    w.complete_research(PlayerId(0), Tech::Champion, &mut Vec::new());
    let u = w.unit(id).unwrap();
    assert_eq!((u.kind, u.hp, u.max_hp), (UnitKind::Champion, 60, 70));
    let stats = w.unit_stats(id).unwrap();
    assert_eq!(
        (stats.attack, stats.melee_armour, stats.pierce_armour),
        (14, 1, 1)
    );
    w.issue_cancel_training(PlayerId(0), barracks).unwrap();
    w.issue_train(PlayerId(0), barracks, UnitKind::Spearman)
        .unwrap();
    assert_eq!(
        w.building(barracks).unwrap().training_queue[0].kind,
        UnitKind::Spearman,
        "Pikeman is not yet researched"
    );
}

#[test]
fn cavalry_archer_and_pike_lines_follow_the_dat_stats() {
    let (mut w, _) = rich_castle();
    imperial(&mut w);
    for tech in [
        Tech::Pikeman,
        Tech::Halberdier,
        Tech::Crossbowman,
        Tech::Arbalester,
        Tech::Cavalier,
        Tech::Paladin,
        Tech::LightCavalry,
        Tech::Hussar,
    ] {
        w.complete_research(PlayerId(0), tech, &mut Vec::new());
    }
    let mut rows = Vec::new();
    for kind in [
        UnitKind::Halberdier,
        UnitKind::Arbalester,
        UnitKind::Cavalier,
        UnitKind::Paladin,
        UnitKind::Hussar,
    ] {
        let id = spawn(&mut w, kind);
        let s = w.unit_stats(id).unwrap();
        rows.push((s.hp, s.attack, s.melee_armour, s.pierce_armour, s.range));
    }
    assert_eq!(
        rows,
        [
            (60, 6, 0, 0, 1),
            (40, 6, 0, 0, 5),
            (120, 12, 2, 2, 1),
            (160, 14, 2, 3, 1),
            (75, 7, 0, 2, 1)
        ]
    );
    // Queued and requested identities collapse to the newest researched step.
    assert_eq!(
        UnitKind::Spearman.upgraded(w.player_economy(PlayerId(0)).unwrap().researched),
        UnitKind::Halberdier
    );
    assert_eq!(
        UnitKind::Knight.upgraded(w.player_economy(PlayerId(0)).unwrap().researched),
        UnitKind::Paladin
    );
    assert_eq!(
        UnitKind::Scout.upgraded(w.player_economy(PlayerId(0)).unwrap().researched),
        UnitKind::Hussar
    );
}

#[test]
fn imperial_units_cannot_be_trained_before_their_age_or_technology() {
    let (mut w, _) = rich_castle();
    let stable = place(&mut w, BuildingKind::Stable, 0);
    assert_eq!(
        w.issue_train(PlayerId(0), stable, UnitKind::Paladin),
        Err(EconomyError::RequiresImperialAge)
    );
    imperial(&mut w);
    assert_eq!(
        w.issue_train(PlayerId(0), stable, UnitKind::Paladin),
        Err(EconomyError::TechRequirementMissing)
    );
    w.complete_research(PlayerId(0), Tech::Cavalier, &mut Vec::new());
    w.issue_train(PlayerId(0), stable, UnitKind::Knight)
        .unwrap();
    assert_eq!(
        w.building(stable).unwrap().training_queue[0].kind,
        UnitKind::Cavalier
    );
}

#[test]
fn siege_lines_keep_ram_and_artillery_behaviour() {
    let (mut w, _) = rich_castle();
    imperial(&mut w);
    for tech in [Tech::CappedRam, Tech::SiegeRam, Tech::Onager] {
        w.complete_research(PlayerId(0), tech, &mut Vec::new());
    }
    let shop = place(&mut w, BuildingKind::SiegeWorkshop, 0);
    w.issue_train(PlayerId(0), shop, UnitKind::BatteringRam)
        .unwrap();
    w.issue_train(PlayerId(0), shop, UnitKind::Mangonel)
        .unwrap();
    let queue: Vec<_> = w
        .building(shop)
        .unwrap()
        .training_queue
        .iter()
        .map(|e| e.kind)
        .collect();
    assert_eq!(queue, [UnitKind::SiegeRam, UnitKind::Onager]);
    let ram = spawn(&mut w, UnitKind::SiegeRam);
    let s = w.unit_stats(ram).unwrap();
    assert_eq!(
        (s.hp, s.attack, s.melee_armour, s.pierce_armour),
        (270, 4, -1, 195)
    );
    assert!(UnitKind::SiegeRam.is_ram() && UnitKind::CappedRam.is_ram());
    assert!(UnitKind::Onager.is_artillery() && !UnitKind::Onager.is_ram());
    assert_eq!(
        crate::game_world::military::class_bonus(UnitKind::SiegeRam, UnitKind::Mangonel),
        65
    );
    let onager = spawn(&mut w, UnitKind::Onager);
    let s = w.unit_stats(onager).unwrap();
    assert_eq!((s.hp, s.attack, s.range, s.min_range), (60, 55, 8, 3));
}

#[test]
fn blacksmith_tier_three_stacks_on_the_earlier_tiers() {
    let (mut w, _) = rich_castle();
    imperial(&mut w);
    for tech in [
        Tech::Forging,
        Tech::IronCasting,
        Tech::ScaleMailArmor,
        Tech::ChainMailArmor,
        Tech::PaddedArcherArmor,
        Tech::LeatherArcherArmor,
        Tech::ScaleBardingArmor,
        Tech::ChainBardingArmor,
        Tech::Fletching,
        Tech::BodkinArrow,
    ] {
        w.complete_research(PlayerId(0), tech, &mut Vec::new());
    }
    let sword = spawn(&mut w, UnitKind::LongSwordsman);
    let archer = spawn(&mut w, UnitKind::Crossbowman);
    let knight = spawn(&mut w, UnitKind::Knight);
    let before = |id| {
        let s = w.unit_stats(id).unwrap();
        (s.attack, s.range, s.melee_armour, s.pierce_armour)
    };
    let (sw, ar, kn) = (before(sword), before(archer), before(knight));
    for tech in [
        Tech::BlastFurnace,
        Tech::PlateMailArmor,
        Tech::PlateBardingArmor,
        Tech::Bracer,
        Tech::RingArcherArmor,
    ] {
        w.complete_research(PlayerId(0), tech, &mut Vec::new());
    }
    let after = |id| {
        let s = w.unit_stats(id).unwrap();
        (s.attack, s.range, s.melee_armour, s.pierce_armour)
    };
    assert_eq!(after(sword), (sw.0 + 2, sw.1, sw.2 + 1, sw.3 + 2));
    assert_eq!(after(archer), (ar.0 + 1, ar.1 + 1, ar.2 + 1, ar.3 + 2));
    assert_eq!(after(knight), (kn.0 + 2, kn.1, kn.2 + 1, kn.3 + 2));
}

#[test]
fn imperial_economy_techs_stack_on_the_castle_age_ones() {
    let (mut w, _) = rich_castle();
    imperial(&mut w);
    for tech in [
        Tech::HorseCollar,
        Tech::HeavyPlow,
        Tech::DoubleBitAxe,
        Tech::BowSaw,
        Tech::StoneMining,
    ] {
        w.complete_research(PlayerId(0), tech, &mut Vec::new());
    }
    let farm = place(&mut w, BuildingKind::Farm, 0);
    let food = w.building(farm).unwrap().food_remaining.unwrap();
    let wood = w.gather_rate(PlayerId(0), ResourceKind::Wood);
    let stone = w.gather_rate(PlayerId(0), ResourceKind::Stone);
    for tech in [Tech::CropRotation, Tech::TwoManSaw, Tech::StoneShaftMining] {
        w.complete_research(PlayerId(0), tech, &mut Vec::new());
    }
    let farm = place(&mut w, BuildingKind::Farm, 1);
    assert_eq!(w.building(farm).unwrap().food_remaining, Some(food + 175));
    let rate = |(n, d): (u32, u32)| u64::from(n) * 1_000_000 / u64::from(d);
    assert_eq!(
        rate(w.gather_rate(PlayerId(0), ResourceKind::Wood)),
        rate(wood) * 120 / 100
    );
    assert_eq!(
        rate(w.gather_rate(PlayerId(0), ResourceKind::Stone)),
        rate(stone) * 115 / 100
    );
}

#[test]
fn imperial_age_keeps_castle_age_building_strength_and_is_deterministic() {
    let run = || {
        let (mut w, tc) = rich_castle();
        let castle_hp = w.building(tc).unwrap().max_hp;
        imperial(&mut w);
        assert_eq!(w.building(tc).unwrap().max_hp, castle_hp);
        assert_eq!(w.building(tc).unwrap().age, Age::Imperial);
        advance(&mut w, 40);
        w.canonical_hash()
    };
    assert_eq!(run(), run());
}

#[test]
fn the_imperial_showcase_is_ready_to_advance_through_the_ordinary_order() {
    let (mut w, _) = GameWorld::imperial_demo(aoe_core::Seed(1)).unwrap();
    assert_eq!(
        w.config().tick_hz,
        1,
        "twenty times slower than the server ticker"
    );
    let tc = w
        .buildings()
        .find(|b| b.player == PlayerId(0) && b.kind == BuildingKind::TownCenter)
        .unwrap()
        .id;
    assert_eq!(w.player_economy(PlayerId(0)).unwrap().age, Age::Castle);
    w.issue_research(PlayerId(0), tc, Tech::ImperialAge)
        .unwrap();
    let ticks = 190 * w.config().tick_hz;
    advance(&mut w, ticks);
    assert_eq!(w.player_economy(PlayerId(0)).unwrap().age, Age::Imperial);
    for tech in [Tech::TwoHandedSwordsman, Tech::Cavalier, Tech::CappedRam] {
        let site = w
            .buildings()
            .find(|b| b.player == PlayerId(0) && b.kind == tech.rules().building)
            .unwrap()
            .id;
        w.issue_research(PlayerId(0), site, tech).unwrap();
    }
}
