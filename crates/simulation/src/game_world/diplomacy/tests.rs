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
