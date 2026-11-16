//! Track A mechanics: one test per mechanic, values from aoe2techtree data.json.
use crate::{
    Age, BuildingKind, EconomyError, GameWorld, MarketPrices, PlayerEconomy, Stockpile, Tech,
    UnitKind, researched_stats, unit_combat,
};
use aoe_core::{EntityId, PlayerId, Seed, TileCoord, TileRect, WorldConfig, WorldPosition};

fn at(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}

fn advance(w: &mut GameWorld, ticks: u32) {
    for _ in 0..ticks {
        w.advance();
    }
}

fn imperial(researched: &[Tech]) -> GameWorld {
    let mut w = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    for player in [PlayerId(0), PlayerId(1)] {
        w.economy.players.insert(
            player,
            PlayerEconomy {
                player,
                stockpile: Stockpile {
                    food: 10_000,
                    wood: 10_000,
                    gold: 10_000,
                    stone: 10_000,
                },
                population_used: 0,
                population_cap: 200,
                age: Age::Imperial,
                researched: [Tech::FeudalAge, Tech::CastleAge, Tech::ImperialAge]
                    .iter()
                    .chain(researched)
                    .fold(0, |bits, t| bits | t.bit()),
                market_prices: MarketPrices::default(),
            },
        );
    }
    w
}

fn hp(w: &GameWorld, id: EntityId) -> u16 {
    w.unit(id).map_or(0, |u| u.hp)
}

#[test]
fn new_units_have_data_json_stats_costs_and_ages() {
    for (kind, hp, attack, melee, pierce, range, min, cost, seconds, age) in [
        (
            UnitKind::CavalryArcher,
            50,
            6,
            0,
            0,
            4,
            0,
            (0, 40, 60),
            37,
            Age::Castle,
        ),
        (
            UnitKind::HeavyCavalryArcher,
            60,
            7,
            1,
            0,
            4,
            0,
            (0, 40, 60),
            30,
            Age::Imperial,
        ),
        (
            UnitKind::CamelRider,
            100,
            6,
            0,
            0,
            1,
            0,
            (55, 0, 60),
            22,
            Age::Castle,
        ),
        (
            UnitKind::HeavyCamelRider,
            120,
            7,
            0,
            0,
            1,
            0,
            (55, 0, 60),
            22,
            Age::Imperial,
        ),
        (
            UnitKind::Scorpion,
            40,
            11,
            0,
            7,
            7,
            2,
            (0, 75, 75),
            30,
            Age::Castle,
        ),
        (
            UnitKind::HeavyScorpion,
            60,
            14,
            1,
            8,
            7,
            2,
            (0, 75, 75),
            30,
            Age::Imperial,
        ),
        (
            UnitKind::HandCannoneer,
            40,
            17,
            1,
            0,
            7,
            0,
            (45, 0, 50),
            34,
            Age::Imperial,
        ),
        (
            UnitKind::BombardCannon,
            80,
            40,
            2,
            5,
            12,
            5,
            (0, 225, 225),
            56,
            Age::Imperial,
        ),
        (
            UnitKind::Trebuchet,
            150,
            200,
            2,
            8,
            16,
            4,
            (0, 200, 200),
            50,
            Age::Castle,
        ),
        (
            UnitKind::TrebuchetUnpacked,
            150,
            200,
            1,
            150,
            16,
            4,
            (0, 200, 200),
            50,
            Age::Castle,
        ),
        (
            UnitKind::Petard,
            50,
            25,
            0,
            2,
            1,
            0,
            (65, 0, 20),
            25,
            Age::Castle,
        ),
        (
            UnitKind::SiegeOnager,
            70,
            75,
            0,
            8,
            8,
            3,
            (0, 160, 135),
            46,
            Age::Imperial,
        ),
    ] {
        let s = unit_combat(kind);
        let t = s.training.unwrap();
        assert_eq!(s.kind, kind);
        assert_eq!(
            (
                s.hp,
                s.attack,
                s.melee_armour,
                s.pierce_armour,
                s.range,
                s.min_range
            ),
            (hp, attack, melee, pierce, range, min),
            "{kind:?}"
        );
        assert_eq!((t.cost.food, t.cost.wood, t.cost.gold), cost, "{kind:?}");
        assert_eq!((t.seconds, kind.required_age()), (seconds, age), "{kind:?}");
    }
}

#[test]
fn production_buildings_train_the_new_lines_behind_their_requirements() {
    let mut w = imperial(&[]);
    let p = PlayerId(0);
    let range = w
        .insert_building(
            p,
            BuildingKind::ArcheryRange,
            TileRect::from_xywh(40, 40, 3, 3),
            true,
        )
        .unwrap();
    let castle = w
        .insert_building(
            p,
            BuildingKind::Castle,
            TileRect::from_xywh(50, 40, 4, 4),
            true,
        )
        .unwrap();
    let stable = w
        .insert_building(
            p,
            BuildingKind::Stable,
            TileRect::from_xywh(60, 40, 3, 3),
            true,
        )
        .unwrap();
    assert_eq!(
        w.issue_train(p, range, UnitKind::HandCannoneer),
        Err(EconomyError::TechRequirementMissing)
    );
    w.complete_research(p, Tech::Chemistry, &mut Vec::new());
    w.issue_train(p, range, UnitKind::HandCannoneer).unwrap();
    w.issue_train(p, range, UnitKind::CavalryArcher).unwrap();
    w.issue_train(p, castle, UnitKind::Trebuchet).unwrap();
    w.issue_train(p, castle, UnitKind::Petard).unwrap();
    assert_eq!(
        w.issue_train(p, stable, UnitKind::HandCannoneer),
        Err(EconomyError::WrongUnitKind)
    );
    // The upgrade replaces the line identity, queued and future alike.
    w.issue_train(p, stable, UnitKind::CamelRider).unwrap();
    w.complete_research(p, Tech::HeavyCamelRider, &mut Vec::new());
    let queued = w.building(stable).unwrap().training_queue[0].kind;
    assert_eq!(queued, UnitKind::HeavyCamelRider);
    assert_eq!(
        UnitKind::Onager.upgraded(Tech::Onager.bit() | Tech::SiegeOnager.bit()),
        UnitKind::SiegeOnager
    );
}

#[test]
fn camels_carry_their_anti_cavalry_bonus_and_chemistry_spares_gunpowder() {
    // 6 attack - 2 Knight melee armour + 9 vs class 8 (Cavalry).
    let mut w = imperial(&[]);
    let camel = w
        .spawn_kind(PlayerId(0), UnitKind::CamelRider, at(100, 100))
        .unwrap();
    let knight = w
        .spawn_kind(PlayerId(1), UnitKind::Knight, at(101, 100))
        .unwrap();
    let before = hp(&w, knight);
    w.issue_attack(PlayerId(0), camel, knight).unwrap();
    advance(&mut w, 2);
    assert_eq!(before - hp(&w, knight), 13);
    let chem = Tech::Chemistry.bit();
    for (kind, delta) in [
        (UnitKind::CavalryArcher, 1),
        (UnitKind::Archer, 1),
        (UnitKind::HandCannoneer, 0),
        (UnitKind::Scorpion, 0),
    ] {
        let base = researched_stats(kind, true, 0).attack;
        assert_eq!(
            researched_stats(kind, true, chem).attack - base,
            delta,
            "{kind:?}"
        );
    }
}

#[test]
fn scorpion_bolts_pass_through_every_enemy_on_their_line() {
    let mut w = imperial(&[]);
    let scorpion = w
        .spawn_kind(PlayerId(0), UnitKind::Scorpion, at(100, 100))
        .unwrap();
    let target = w
        .spawn_kind(PlayerId(1), UnitKind::Militia, at(106, 100))
        .unwrap();
    let between = w
        .spawn_kind(PlayerId(1), UnitKind::Militia, at(103, 100))
        .unwrap();
    let beside = w
        .spawn_kind(PlayerId(1), UnitKind::Militia, at(103, 104))
        .unwrap();
    let friend = w
        .spawn_kind(PlayerId(0), UnitKind::Militia, at(104, 100))
        .unwrap();
    // Hold everyone in place so the line geometry stays exact.
    let still = aoe_core::CombatStance::NoAttack;
    w.issue_stance(PlayerId(1), &[target, between, beside], still)
        .unwrap();
    w.issue_stance(PlayerId(0), &[friend], still).unwrap();
    w.issue_attack(PlayerId(0), scorpion, target).unwrap();
    for _ in 0..80 {
        if hp(&w, target) < 40 {
            break;
        }
        w.advance();
    }
    assert!(hp(&w, target) < 40, "target hit");
    assert!(hp(&w, between) < 40, "the unit on the line is hit");
    assert_eq!(hp(&w, beside), 40, "a unit off the line is spared");
    assert_eq!(hp(&w, friend), 40, "bolts pass friendly units");
}

#[test]
fn petards_explode_on_buildings_and_die() {
    let mut w = imperial(&[]);
    let house = w
        .insert_building(
            PlayerId(1),
            BuildingKind::House,
            TileRect::from_xywh(110, 100, 2, 2),
            true,
        )
        .unwrap();
    let petard = w
        .spawn_kind(PlayerId(0), UnitKind::Petard, at(108, 100))
        .unwrap();
    let before = w.building(house).unwrap().hp;
    w.issue_attack(PlayerId(0), petard, house).unwrap();
    advance(&mut w, 120);
    let after = w.building(house).map_or(0, |b| b.hp);
    assert!(before - after >= 500, "{before} -> {after}");
    assert!(w.unit(petard).is_none_or(|u| u.hp == 0));
}

#[test]
fn trebuchets_unpack_before_firing_and_pack_to_move() {
    let mut w = imperial(&[]);
    let castle = w
        .insert_building(
            PlayerId(1),
            BuildingKind::Castle,
            TileRect::from_xywh(120, 100, 4, 4),
            true,
        )
        .unwrap();
    let treb = w
        .spawn_kind(PlayerId(0), UnitKind::Trebuchet, at(108, 101))
        .unwrap();
    let before = w.building(castle).unwrap().hp;
    w.issue_attack(PlayerId(0), treb, castle).unwrap();
    let hz = w.config.tick_hz;
    advance(&mut w, 2);
    assert_eq!(w.unit(treb).unwrap().kind, UnitKind::TrebuchetUnpacked);
    advance(&mut w, hz * crate::TREBUCHET_UNPACK_SECONDS - 4);
    assert_eq!(
        w.building(castle).unwrap().hp,
        before,
        "no shot while unpacking"
    );
    advance(&mut w, hz * 6);
    assert!(w.building(castle).unwrap().hp < before, "the boulder lands");
    w.issue_move(treb, at(100, 101)).unwrap();
    assert_eq!(w.unit(treb).unwrap().kind, UnitKind::Trebuchet);
}

#[test]
fn siege_onager_blast_is_wider_than_the_mangonels() {
    fn splash(kind: UnitKind) -> u16 {
        let mut w = imperial(&[]);
        let m = w.spawn_kind(PlayerId(0), kind, at(100, 100)).unwrap();
        let target = w
            .spawn_kind(PlayerId(1), UnitKind::Knight, at(106, 100))
            .unwrap();
        // 1.25 tiles from the impact: outside a Mangonel's 1.0, inside 1.5.
        let aim = at(106, 101);
        let edge = WorldPosition::new(aim.x + 768, aim.y);
        let edge = w.spawn_kind(PlayerId(1), UnitKind::Knight, edge).unwrap();
        w.issue_attack(PlayerId(0), m, target).unwrap();
        advance(&mut w, 40);
        100 - hp(&w, edge)
    }
    assert_eq!(splash(UnitKind::Mangonel), 0);
    assert!(splash(UnitKind::SiegeOnager) > 0);
}
