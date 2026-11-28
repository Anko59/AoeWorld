mod conquest;

use super::*;
use aoe_core::{Seed, TileCoord, TileRect, WorldConfig, WorldPosition};

const FIRST: PlayerId = PlayerId(0);
const SECOND: PlayerId = PlayerId(1);

fn at(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}

fn world() -> GameWorld {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    for (player, x) in [(FIRST, 30), (SECOND, 190)] {
        world
            .insert_building(
                player,
                BuildingKind::TownCenter,
                TileRect::from_xywh(x, 30, 4, 4),
                true,
            )
            .unwrap();
    }
    world
}

#[test]
fn stances_default_to_enemy_and_alliance_is_mutual_and_persisted() {
    let mut source = world();
    assert_eq!(
        source.diplomatic_stance(FIRST, SECOND),
        DiplomaticStance::Enemy
    );
    assert_eq!(
        source.diplomatic_stance(SECOND, FIRST),
        DiplomaticStance::Enemy
    );
    assert!(
        source
            .set_diplomatic_stance(FIRST, FIRST, DiplomaticStance::Ally)
            .is_err()
    );
    assert!(
        source
            .set_diplomatic_stance(FIRST, PlayerId(2), DiplomaticStance::Ally)
            .is_err()
    );
    let before = source.canonical_hash();
    source
        .set_diplomatic_stance(FIRST, SECOND, DiplomaticStance::Ally)
        .unwrap();
    assert_ne!(source.canonical_hash(), before);
    assert_eq!(
        source.diplomatic_stance(SECOND, FIRST),
        DiplomaticStance::Ally
    );

    let snapshot = source.snapshot([0; 32]).unwrap();
    let mut restored = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    restored
        .restore_snapshot(&snapshot, [0; 32], &|| false)
        .unwrap();
    assert_eq!(
        restored.diplomatic_stance(FIRST, SECOND),
        DiplomaticStance::Ally
    );
    assert_eq!(
        restored.diplomatic_stance(SECOND, FIRST),
        DiplomaticStance::Ally
    );

    restored
        .set_diplomatic_stance(SECOND, FIRST, DiplomaticStance::Enemy)
        .unwrap();
    assert_eq!(
        restored.diplomatic_stance(FIRST, SECOND),
        DiplomaticStance::Neutral
    );
    assert_eq!(
        restored.diplomatic_stance(SECOND, FIRST),
        DiplomaticStance::Enemy
    );
}

#[test]
fn militia_and_towers_ignore_neutrals_and_allies_but_neutral_can_be_ordered() {
    for stance in [DiplomaticStance::Neutral, DiplomaticStance::Ally] {
        let mut world = world();
        world.set_diplomatic_stance(FIRST, SECOND, stance).unwrap();
        world.set_diplomatic_stance(SECOND, FIRST, stance).unwrap();
        let militia = world.spawn_militia(FIRST, at(100, 100)).unwrap();
        let tower = world
            .insert_building(
                FIRST,
                BuildingKind::WatchTower,
                TileRect::from_xywh(98, 98, 1, 1),
                true,
            )
            .unwrap();
        let visitor = world.spawn_militia(SECOND, at(103, 96)).unwrap();
        world.issue_move(visitor, at(103, 115)).unwrap();
        for _ in 0..220 {
            world.advance();
            assert!(world.unit(militia).unwrap().attack_target.is_none());
            assert!(world.unit(visitor).unwrap().last_hit.is_none());
        }
        assert!(world.unit(visitor).unwrap().position.tile_floor().y > 100);
        let visitor = world.spawn_militia(SECOND, at(100, 98)).unwrap();
        let is_neutral = stance == DiplomaticStance::Neutral;
        assert_eq!(
            world.issue_attack(FIRST, tower, visitor).is_ok(),
            is_neutral
        );
        assert_eq!(
            world.issue_attack(FIRST, militia, visitor).is_ok(),
            is_neutral
        );
    }
}

#[test]
fn tribute_uses_sender_research_and_changes_both_balances_atomically() {
    for (tech, cost) in [
        (0, 130),
        (crate::Tech::Coinage.bit(), 120),
        (crate::Tech::Banking.bit(), 100),
    ] {
        let mut world = world();
        let market = world
            .insert_building(
                FIRST,
                BuildingKind::Market,
                TileRect::from_xywh(40, 40, 4, 4),
                true,
            )
            .unwrap();
        let sender = world.economy.players.get_mut(&FIRST).unwrap();
        sender.stockpile.wood = cost;
        sender.researched = tech;
        let before = world.player_economy(SECOND).unwrap().stockpile.wood;
        assert!(
            world
                .issue_tribute(FIRST, SECOND, market, ResourceKind::Wood, 101)
                .is_err()
        );
        assert_eq!(world.player_economy(FIRST).unwrap().stockpile.wood, cost);
        assert_eq!(world.player_economy(SECOND).unwrap().stockpile.wood, before);
        world
            .issue_tribute(FIRST, SECOND, market, ResourceKind::Wood, 100)
            .unwrap();
        assert_eq!(world.player_economy(FIRST).unwrap().stockpile.wood, 0);
        assert_eq!(
            world.player_economy(SECOND).unwrap().stockpile.wood,
            before + 100
        );
    }
}

#[test]
fn alliance_offers_require_acceptance_and_fail_during_war() {
    let mut world = world();
    world.offer_alliance(FIRST, SECOND).unwrap();
    assert_eq!(
        world.diplomatic_stance(SECOND, FIRST),
        DiplomaticStance::Enemy
    );
    assert_eq!(world.alliance_requests(SECOND).collect::<Vec<_>>(), [FIRST]);
    world.offer_alliance(SECOND, FIRST).unwrap();
    assert_eq!(
        world.diplomatic_stance(SECOND, FIRST),
        DiplomaticStance::Ally
    );
    assert_eq!(world.alliance_requests(SECOND).next(), None);
    world
        .set_diplomatic_stance(SECOND, FIRST, DiplomaticStance::Enemy)
        .unwrap();
    assert!(world.offer_alliance(FIRST, SECOND).is_err());

    world
        .set_diplomatic_stance(SECOND, FIRST, DiplomaticStance::Neutral)
        .unwrap();
    let militia = world.spawn_militia(FIRST, at(100, 100)).unwrap();
    let target = world.spawn_militia(SECOND, at(101, 100)).unwrap();
    world.issue_attack(FIRST, militia, target).unwrap();
    assert!(!world.at_war(FIRST, SECOND));
    for _ in 0..100 {
        world.advance();
        if world.unit(target).unwrap().last_hit.is_some() {
            break;
        }
    }
    assert!(world.unit(target).unwrap().last_hit.is_some());
    assert_eq!(
        world.offer_alliance(FIRST, SECOND),
        Err(EconomyError::DiplomacyAtWar)
    );
}

#[test]
fn only_complete_allied_market_positions_are_shared() {
    let mut world = world();
    for (kind, complete, x) in [
        (BuildingKind::Market, true, 150),
        (BuildingKind::Market, false, 160),
        (BuildingKind::TownCenter, true, 170),
    ] {
        world
            .insert_building(SECOND, kind, TileRect::from_xywh(x, 145, 4, 4), complete)
            .unwrap();
    }
    let tile = TileCoord::new(152, 147);
    assert_eq!(world.allied_market_positions(FIRST).next(), None);
    world
        .set_diplomatic_stance(FIRST, SECOND, DiplomaticStance::Ally)
        .unwrap();
    assert_eq!(
        world.allied_market_positions(FIRST).collect::<Vec<_>>(),
        [(SECOND, tile)]
    );
    assert_eq!(
        world.visibility(FIRST, tile),
        crate::TileVisibility::Unexplored
    );
    assert!(
        world
            .known_buildings(FIRST)
            .iter()
            .all(|building| building.player == FIRST)
    );
    world
        .set_diplomatic_stance(FIRST, SECOND, DiplomaticStance::Enemy)
        .unwrap();
    assert_eq!(world.allied_market_positions(FIRST).next(), None);
}

#[test]
fn automatic_tower_damage_blocks_alliance_offers_until_combat_cools() {
    let mut source = world();
    let tower = source
        .insert_building(
            FIRST,
            BuildingKind::WatchTower,
            TileRect::from_xywh(98, 98, 1, 1),
            true,
        )
        .unwrap();
    let visitor = source.spawn_villager(SECOND, at(103, 98)).unwrap();
    assert!(!source.at_war(FIRST, SECOND));
    source.offer_alliance(FIRST, SECOND).unwrap();
    for _ in 0..100 {
        source.advance();
        if source.unit(visitor).unwrap().last_hit.is_some() {
            break;
        }
    }
    assert!(source.unit(visitor).unwrap().last_hit.is_some());
    assert_eq!(
        source.offer_alliance(SECOND, FIRST),
        Err(EconomyError::DiplomacyAtWar)
    );
    assert_eq!(
        source.offer_alliance(FIRST, SECOND),
        Err(EconomyError::DiplomacyAtWar)
    );

    source.destroy_building(tower);
    source.remove_unit(visitor);
    let snapshot = source.snapshot([0; 32]).unwrap();
    let mut restored = GameWorld::new(source.config).unwrap();
    restored
        .restore_snapshot(&snapshot, [0; 32], &|| false)
        .unwrap();
    let cooldown = source.config.tick_hz * 10;
    for _ in 0..cooldown {
        assert!(restored.at_war(FIRST, SECOND));
        restored.advance();
    }
    assert!(!restored.at_war(FIRST, SECOND));
    restored.offer_alliance(SECOND, FIRST).unwrap();
    assert!(restored.allied_seats(FIRST, SECOND));
}

#[test]
fn allied_requests_are_private_validated_and_answered_once() {
    use aoe_core::DiplomacyMessage;
    let mut world = world();
    let third = PlayerId(2);
    world
        .place_town_center(third, TileCoord::new(120, 30))
        .unwrap();
    let request = DiplomacyMessage::RequestTribute {
        resource: ResourceKind::Wood,
        amount: 100,
    };
    assert!(world.send_diplomacy(FIRST, SECOND, request).is_err());
    world
        .set_diplomatic_stance(FIRST, SECOND, DiplomaticStance::Ally)
        .unwrap();
    world.send_diplomacy(FIRST, SECOND, request).unwrap();
    let sent = *world.diplomacy_log(SECOND).last().unwrap();
    assert_eq!(sent.message, request);
    assert_eq!(world.diplomacy_log(FIRST), world.diplomacy_log(SECOND));
    assert!(world.diplomacy_log(third).is_empty());
    let answer = DiplomacyMessage::Reply {
        request: sent.id,
        accepted: true,
    };
    assert!(world.send_diplomacy(FIRST, SECOND, answer).is_err());
    // A true reply requires a completed Market and a successful transfer.
    assert!(world.send_diplomacy(SECOND, FIRST, answer).is_err());
    world
        .insert_building(
            SECOND,
            BuildingKind::Market,
            TileRect::from_xywh(190, 40, 4, 4),
            true,
        )
        .unwrap();
    world
        .economy
        .players
        .get_mut(&SECOND)
        .unwrap()
        .stockpile
        .wood = 130;
    world.send_diplomacy(SECOND, FIRST, answer).unwrap();
    assert_eq!(world.player_economy(SECOND).unwrap().stockpile.wood, 0);
    assert_eq!(world.player_economy(FIRST).unwrap().stockpile.wood, 100);
    assert!(world.send_diplomacy(SECOND, FIRST, answer).is_err());
}

#[test]
fn losing_an_ai_town_center_transfers_survivors_and_stock_to_its_destroyer() {
    use aoe_core::DiplomacyMessage;
    let mut world = world();
    world.set_diplomacy_ai_seats(&[SECOND]);
    let center = world
        .player_buildings_of(SECOND, BuildingKind::TownCenter)
        .next()
        .unwrap()
        .id;
    let house = world
        .insert_building(
            SECOND,
            BuildingKind::House,
            TileRect::from_xywh(200, 40, 2, 2),
            true,
        )
        .unwrap();
    let survivor = world.spawn_militia(SECOND, at(194, 32)).unwrap();
    let attacker = world.spawn_militia(FIRST, at(189, 32)).unwrap();
    world
        .economy
        .players
        .get_mut(&SECOND)
        .unwrap()
        .stockpile
        .gold = 75;
    world.advance();
    assert_eq!(world.building(house).unwrap().player, SECOND);

    world.resolve_hit(
        (attacker, FIRST),
        center,
        10_000,
        0,
        super::super::combat::DamageKind::Melee,
        &mut Vec::new(),
    );
    world.advance();
    assert_eq!(world.building(house).unwrap().player, FIRST);
    assert_eq!(world.unit(survivor).unwrap().player, FIRST);
    assert_eq!(world.player_economy(FIRST).unwrap().stockpile.gold, 75);
    assert_eq!(
        world.player_economy(SECOND).unwrap().stockpile,
        Stockpile::default()
    );
    assert_eq!(world.surrendered_to(SECOND), Some(FIRST));
    assert_eq!(
        world.diplomacy_log(FIRST).last().unwrap().message,
        DiplomacyMessage::Surrender
    );
}

#[test]
fn conquest_waits_for_a_weak_army_and_persists_the_conqueror() {
    let mut world = world();
    world.set_diplomacy_ai_seats(&[SECOND]);
    let center = world
        .player_buildings_of(SECOND, BuildingKind::TownCenter)
        .next()
        .unwrap()
        .id;
    let attacker = world.spawn_militia(FIRST, at(170, 40)).unwrap();
    let defenders: Vec<_> = (0..5)
        .map(|index| world.spawn_militia(SECOND, at(200 + index, 70)).unwrap())
        .collect();
    world.resolve_hit(
        (attacker, FIRST),
        center,
        10_000,
        0,
        crate::DamageKind::Melee,
        &mut Vec::new(),
    );
    world.advance();
    assert_eq!(world.surrendered_to(SECOND), None);
    let checkpoint = world.snapshot([0; 32]).unwrap();

    let mut restored = GameWorld::new(world.config()).unwrap();
    restored
        .restore_snapshot(&checkpoint, [0; 32], &|| false)
        .unwrap();
    for game in [&mut world, &mut restored] {
        game.resolve_hit(
            (attacker, FIRST),
            defenders[0],
            10_000,
            0,
            crate::DamageKind::Melee,
            &mut Vec::new(),
        );
        game.advance();
        assert_eq!(game.surrendered_to(SECOND), Some(FIRST));
        for &id in &defenders[1..] {
            assert_eq!(game.unit(id).unwrap().player, FIRST);
        }
    }
    assert_eq!(world.diplomacy_log(FIRST), restored.diplomacy_log(FIRST));
}

#[test]
fn only_a_conquered_ai_surrenders() {
    let mut world = world();
    world.set_diplomacy_ai_seats(&[SECOND]);
    let human_home = world
        .player_buildings_of(FIRST, BuildingKind::TownCenter)
        .next()
        .unwrap()
        .id;
    let ai_home = world
        .player_buildings_of(SECOND, BuildingKind::TownCenter)
        .next()
        .unwrap()
        .id;
    let attacker = world.spawn_militia(SECOND, at(190, 42)).unwrap();
    world.resolve_hit(
        (attacker, SECOND),
        human_home,
        10_000,
        0,
        crate::DamageKind::Melee,
        &mut Vec::new(),
    );
    world.destroy_building(ai_home);
    world.advance();
    assert_eq!(world.surrendered_to(FIRST), None);
    assert_eq!(world.surrendered_to(SECOND), None);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn surrender_is_identical_at_one_and_four_workers() {
    let run = |threads| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap()
            .install(|| {
                let mut world = world();
                world.set_diplomacy_ai_seats(&[SECOND]);
                let center = world
                    .player_buildings_of(SECOND, BuildingKind::TownCenter)
                    .next()
                    .unwrap()
                    .id;
                let attacker = world.spawn_militia(FIRST, at(180, 40)).unwrap();
                world.spawn_militia(SECOND, at(202, 40)).unwrap();
                world.resolve_hit(
                    (attacker, FIRST),
                    center,
                    10_000,
                    0,
                    crate::DamageKind::Melee,
                    &mut Vec::new(),
                );
                world.advance();
                assert_eq!(world.surrendered_to(SECOND), Some(FIRST));
                world.canonical_hash()
            })
    };
    assert_eq!(run(1), run(4));
}
