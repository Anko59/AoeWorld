//! Feudal economy regressions, pinned to the DE values cited in docs/economy.md.
use super::buildings::{finish, set_stock, stock, villager_at};
use super::*;
use crate::{Age, Tech, building_combat};

fn at(x: i32, y: i32) -> TileCoord {
    TileCoord::new(x, y)
}

fn rich(world: &mut GameWorld) {
    set_stock(world, 10_000, 10_000, 10_000, 10_000);
}

fn built(world: &mut GameWorld, kind: BuildingKind, x: i32) -> EntityId {
    let worker = villager_at(world, x - 1, 100);
    fixture::survey(world, PlayerId(0), at(x + 1, 101), 3);
    let id = world
        .issue_build(PlayerId(0), &[worker], kind, at(x, 100), false)
        .unwrap();
    finish(world, id, 4000);
    id
}

fn age_up(world: &mut GameWorld, center: EntityId) {
    built(world, BuildingKind::LumberCamp, 80);
    built(world, BuildingKind::Mill, 90);
    world
        .issue_research(PlayerId(0), center, Tech::FeudalAge)
        .unwrap();
    advance(world, 130 * world.config.tick_hz);
    assert_eq!(world.player_economy(PlayerId(0)).unwrap().age, Age::Feudal);
}

#[test]
fn feudal_buildings_are_age_gated_atomically_then_build_with_de_stats() {
    use BuildingKind::*;
    let (mut world, _, center) = village();
    rich(&mut world);
    let worker = villager_at(&mut world, 149, 100);
    for kind in [ArcheryRange, Stable, Blacksmith, Market, WatchTower] {
        let before = world.canonical_hash();
        assert_eq!(
            world.issue_build(PlayerId(0), &[worker], kind, at(150, 100), false),
            Err(EconomyError::RequiresFeudalAge)
        );
        assert_eq!(world.canonical_hash(), before);
    }
    age_up(&mut world, center);
    built(&mut world, Barracks, 110);
    for (i, (kind, wood, stone, seconds, hp, side, los, armour)) in [
        (ArcheryRange, 175, 0, 50, 1500, 3, 6, (1, 8)),
        (Stable, 175, 0, 50, 1500, 3, 6, (1, 8)),
        (Blacksmith, 150, 0, 40, 1800, 3, 6, (1, 8)),
        (Market, 175, 0, 60, 1800, 4, 6, (1, 8)),
        (WatchTower, 35, 125, 80, 850, 1, 10, (1, 7)),
    ]
    .into_iter()
    .enumerate()
    {
        let r = kind.rules();
        assert_eq!(
            (
                r.cost.wood,
                r.cost.stone,
                r.build_seconds,
                r.max_hp,
                r.side,
                r.los
            ),
            (wood, stone, seconds, hp, side, los)
        );
        let c = building_combat(kind);
        assert_eq!((c.melee_armour, c.pierce_armour), armour);
        assert_eq!(
            r.trainable,
            match kind {
                ArcheryRange => &[
                    UnitKind::Archer,
                    UnitKind::Skirmisher,
                    UnitKind::Crossbowman,
                    UnitKind::EliteSkirmisher
                ][..],
                Stable => &[UnitKind::Scout, UnitKind::Knight, UnitKind::LightCavalry][..],
                _ => &[][..],
            }
        );
        let x = 130 + 10 * i as i32;
        let worker = villager_at(&mut world, x - 1, 100);
        fixture::survey(&mut world, PlayerId(0), at(x + 1, 101), 3);
        let before = stock(&world);
        let id = world
            .issue_build(PlayerId(0), &[worker], kind, at(x, 100), false)
            .unwrap();
        assert_eq!(
            stock(&world),
            (before.0, before.1 - wood, before.2, before.3 - stone)
        );
        let b = world.building(id).unwrap();
        assert_eq!(
            (b.footprint.width(), b.footprint.height(), b.hp),
            (side, side, 1)
        );
        assert!(
            world
                .issue_build(PlayerId(0), &[worker], kind, at(x, 100), false)
                .is_err()
        );
        finish(&mut world, id, seconds * 20 + 30);
        assert_eq!(world.building(id).unwrap().hp, hp);
    }
}

#[test]
fn every_feudal_tech_has_its_cost_time_age_gate_and_researching_building() {
    use BuildingKind::*;
    use Tech::*;
    let (mut world, _, center) = village();
    rich(&mut world);
    let lumber = built(&mut world, LumberCamp, 80);
    let mill = built(&mut world, Mill, 90);
    let mining = built(&mut world, MiningCamp, 100);
    let cases = [
        (DoubleBitAxe, lumber, LumberCamp, 100, 50, 25),
        (HorseCollar, mill, Mill, 75, 75, 20),
        (GoldMining, mining, MiningCamp, 100, 75, 30),
        (StoneMining, mining, MiningCamp, 100, 75, 30),
        (Wheelbarrow, center, TownCenter, 175, 50, 75),
    ];
    for &(tech, site, kind, food, wood, seconds) in &cases {
        let r = tech.rules();
        assert_eq!(
            (
                r.building,
                r.cost.food,
                r.cost.wood,
                r.cost.gold,
                r.cost.stone,
                r.seconds
            ),
            (kind, food, wood, 0, 0, seconds)
        );
        let before = world.canonical_hash();
        assert_eq!(
            world.issue_research(PlayerId(0), site, tech),
            Err(EconomyError::RequiresFeudalAge)
        );
        assert_eq!(world.canonical_hash(), before);
    }
    world
        .issue_research(PlayerId(0), center, FeudalAge)
        .unwrap();
    advance(&mut world, 2600);
    for &(tech, site, _, food, wood, seconds) in &cases {
        let before = stock(&world);
        world.issue_research(PlayerId(0), site, tech).unwrap();
        assert_eq!(
            stock(&world),
            (before.0 - food, before.1 - wood, before.2, before.3)
        );
        advance(&mut world, seconds * 20 - 1);
        assert!(!world.has_researched(PlayerId(0), tech));
        world.advance();
        assert!(world.has_researched(PlayerId(0), tech));
    }
}

#[test]
fn horse_collar_adds_seventy_five_food_only_to_farms_placed_after_research() {
    let (mut world, _, center) = village();
    rich(&mut world);
    age_up(&mut world, center);
    let mill = world
        .buildings()
        .find(|b| b.kind == BuildingKind::Mill)
        .unwrap()
        .id;
    let old = built(&mut world, BuildingKind::Farm, 110);
    world.economy.gatherers.clear();
    let before = world.building(old).unwrap().food_remaining;
    world
        .issue_research(PlayerId(0), mill, Tech::HorseCollar)
        .unwrap();
    advance(&mut world, 400);
    assert_eq!(world.building(old).unwrap().food_remaining, before);
    let farmer = villager_at(&mut world, 119, 100);
    let farm = world
        .issue_build(
            PlayerId(0),
            &[farmer],
            BuildingKind::Farm,
            at(120, 100),
            false,
        )
        .unwrap();
    assert_eq!(world.building(farm).unwrap().food_remaining, Some(250));
}

#[test]
fn wheelbarrow_changes_existing_and_future_worker_speed_and_integer_carry_capacity() {
    let (mut world, _, center) = village();
    rich(&mut world);
    age_up(&mut world, center);
    let worker = villager_at(&mut world, 100, 130);
    world
        .issue_research(PlayerId(0), center, Tech::Wheelbarrow)
        .unwrap();
    advance(&mut world, 1500);
    assert_eq!(crate::villager_capacity(0), 10);
    assert_eq!(crate::villager_capacity(Tech::Wheelbarrow.bit()), 13);
    assert_eq!(world.villager_capacity(PlayerId(0)), 13); // ceil(10 * 1.2695)
    assert_eq!(world.villager_capacity(PlayerId(1)), 10);
    let fresh = villager_at(&mut world, 100, 140);
    for id in [worker, fresh] {
        let start = world.unit(id).unwrap().position;
        world
            .issue_move(id, WorldPosition::new(start.x + 20 * 1024, start.y))
            .unwrap();
        advance(&mut world, 100);
        assert_eq!(world.unit(id).unwrap().position.x - start.x, 4505); // floor(0.88 * 5 * 1024)
    }
}

#[test]
fn gather_techs_multiply_exact_credits_without_rounding_the_base_rates() {
    let (mut world, _, center) = village();
    rich(&mut world);
    age_up(&mut world, center);
    let lumber = world
        .buildings()
        .find(|b| b.kind == BuildingKind::LumberCamp)
        .unwrap()
        .id;
    let mining = built(&mut world, BuildingKind::MiningCamp, 110);
    for (tech, site, kind, base, percent) in [
        (Tech::DoubleBitAxe, lumber, ResourceKind::Wood, 39, 120),
        (Tech::GoldMining, mining, ResourceKind::Gold, 38, 115),
        (Tech::StoneMining, mining, ResourceKind::Stone, 36, 115),
    ] {
        assert_eq!(world.gather_rate(PlayerId(0), kind), (base * 100, 100));
        world.issue_research(PlayerId(0), site, tech).unwrap();
        advance(&mut world, tech.rules().seconds * 20);
        assert_eq!(world.gather_rate(PlayerId(0), kind), (base * percent, 100));
        assert_eq!(world.gather_rate(PlayerId(1), kind), (base * 100, 100));
    }
}

#[test]
fn watch_tower_shoots_enemies_with_pierce_armour_range_minimum_and_reload() {
    let arrow = building_combat(BuildingKind::WatchTower).attack.unwrap();
    assert_eq!(
        (
            arrow.attack,
            arrow.min_range,
            arrow.range,
            arrow.period_centiseconds
        ),
        (5, 1, 8, 200)
    );
    let (mut world, _, center) = village();
    rich(&mut world);
    age_up(&mut world, center);
    let tower = built(&mut world, BuildingKind::WatchTower, 130);
    let enemy = world
        .spawn_militia(
            PlayerId(1),
            WorldPosition::from_tile_center(at(137, 100)).unwrap(),
        )
        .unwrap();
    let friend = villager_at(&mut world, 132, 100);
    world.units[world.lookup[&enemy]].state.stance = aoe_core::CombatStance::NoAttack;
    world.advance();
    assert_eq!(world.unit(enemy).unwrap().hp, 40, "arrow must travel first");
    advance(&mut world, 20);
    assert_eq!(world.unit(enemy).unwrap().hp, 36); // 5 pierce - 1 armour
    assert_eq!(world.unit(enemy).unwrap().last_hit.unwrap().attacker, tower);
    assert_eq!(world.unit(friend).unwrap().hp, 25);
    advance(&mut world, 19);
    assert_eq!(world.unit(enemy).unwrap().hp, 36);
    advance(&mut world, 21);
    assert_eq!(world.unit(enemy).unwrap().hp, 32);
    let too_close = world
        .spawn_militia(
            PlayerId(1),
            WorldPosition::new(131 * 1024, 100 * 1024 + 512),
        )
        .unwrap();
    let too_far = world
        .spawn_militia(
            PlayerId(1),
            WorldPosition::from_tile_center(at(139, 100)).unwrap(),
        )
        .unwrap();
    for id in [too_close, too_far] {
        world.units[world.lookup[&id]].state.stance = aoe_core::CombatStance::NoAttack;
    }
    advance(&mut world, 40);
    assert_eq!(world.unit(too_close).unwrap().hp, 40);
    assert_eq!(world.unit(too_far).unwrap().hp, 40);
}

#[test]
fn age_up_updates_existing_damaged_new_and_unfinished_building_stats() {
    let (mut world, _, center) = village();
    rich(&mut world);
    let house = built(&mut world, BuildingKind::House, 110);
    let gate = built(&mut world, BuildingKind::PalisadeGate, 120);
    world.economy.buildings.get_mut(&house).unwrap().hp = 300;
    built(&mut world, BuildingKind::LumberCamp, 80);
    built(&mut world, BuildingKind::Mill, 90);
    world
        .issue_research(PlayerId(0), center, Tech::FeudalAge)
        .unwrap();
    advance(&mut world, 2599);
    let worker = villager_at(&mut world, 129, 100);
    let foundation = world
        .issue_build(
            PlayerId(0),
            &[worker],
            BuildingKind::House,
            at(130, 100),
            false,
        )
        .unwrap();
    world.advance();
    let h = world.building(house).unwrap();
    assert_eq!((h.hp, h.max_hp), (500, 750)); // the 250 damage is preserved
    assert_eq!(world.building(gate).unwrap().max_hp, 400);
    let f = world.building(foundation).unwrap();
    assert_eq!(f.max_hp, 750);
    assert!(f.hp < 10, "age-up does not heal a foundation");
    for b in world
        .buildings()
        .filter(|b| matches!(b.kind, BuildingKind::LumberCamp | BuildingKind::Mill))
    {
        assert_eq!((b.max_hp, b.los), (800, 6));
    }
    let later = built(&mut world, BuildingKind::House, 140);
    assert_eq!(world.building(later).unwrap().max_hp, 750);
}

fn worker_player(world: &mut GameWorld, researched: u64) {
    world.economy.players.insert(
        PlayerId(0),
        crate::PlayerEconomy {
            player: PlayerId(0),
            stockpile: crate::Stockpile::default(),
            population_used: 1,
            age: Age::Feudal,
            researched: researched | Tech::FeudalAge.bit(),
            market_prices: crate::MarketPrices::default(),
        },
    );
}

#[test]
fn gathering_uses_the_exact_researched_rates_on_real_resource_depletion() {
    for (kind, tech, expected) in [
        (ResourceKind::Wood, Tech::DoubleBitAxe, 9),
        (ResourceKind::Gold, Tech::GoldMining, 8),
        (ResourceKind::Stone, Tech::StoneMining, 8),
    ] {
        for upgraded in [false, true] {
            let (mut world, node, worker) = resource_world(kind);
            worker_player(&mut world, if upgraded { tech.bit() } else { 0 });
            let initial = world.resource_remaining(node.id).unwrap();
            world.issue_gather(PlayerId(0), worker, node.id).unwrap();
            advance(&mut world, 400);
            assert_eq!(
                initial - world.resource_remaining(node.id).unwrap(),
                if upgraded { expected } else { 7 }
            );
        }
    }
}

#[test]
fn wheelbarrow_fills_and_returns_thirteen_resource_loads() {
    let (mut world, node, worker) = resource_world(ResourceKind::Wood);
    worker_player(&mut world, Tech::Wheelbarrow.bit());
    world.issue_gather(PlayerId(0), worker, node.id).unwrap();
    advance(&mut world, 1000);
    assert_eq!(world.unit(worker).unwrap().carried.unwrap().amount, 13);
}

#[test]
fn feudal_research_tower_hits_and_gate_transitions_replay_deterministically() {
    let run = || {
        let (mut world, _, center) = village();
        rich(&mut world);
        age_up(&mut world, center);
        let lumber = world
            .buildings()
            .find(|b| b.kind == BuildingKind::LumberCamp)
            .unwrap()
            .id;
        let mill = world
            .buildings()
            .find(|b| b.kind == BuildingKind::Mill)
            .unwrap()
            .id;
        let mining = built(&mut world, BuildingKind::MiningCamp, 110);
        for (site, tech) in [
            (lumber, Tech::DoubleBitAxe),
            (mill, Tech::HorseCollar),
            (mining, Tech::GoldMining),
            (mining, Tech::StoneMining),
            (center, Tech::Wheelbarrow),
        ] {
            world.issue_research(PlayerId(0), site, tech).unwrap();
            advance(&mut world, tech.rules().seconds * 20);
        }
        built(&mut world, BuildingKind::WatchTower, 130);
        built(&mut world, BuildingKind::PalisadeGate, 140);
        let enemy = world
            .spawn_militia(
                PlayerId(1),
                WorldPosition::from_tile_center(at(137, 100)).unwrap(),
            )
            .unwrap();
        let mut hashes = Vec::new();
        for _ in 0..100 {
            world.advance();
            hashes.push(world.canonical_hash());
        }
        assert!(world.unit(enemy).unwrap().hp < 40);
        assert_ne!(hashes.first(), hashes.last());
        hashes
    };
    assert_eq!(run(), run());
}

#[test]
fn feudal_buildings_reveal_their_de_los_and_age_up_refreshes_sight() {
    use crate::TileVisibility::{Unexplored, Visible};
    let (mut world, _, center) = village();
    rich(&mut world);
    let camp = built(&mut world, BuildingKind::LumberCamp, 80);
    let edge = at(81, 107);
    assert_eq!(world.visibility(PlayerId(0), edge), Unexplored);
    built(&mut world, BuildingKind::Mill, 90);
    world
        .issue_research(PlayerId(0), center, Tech::FeudalAge)
        .unwrap();
    advance(&mut world, 2600);
    assert_eq!(world.visibility(PlayerId(0), edge), Visible);
    assert_eq!(world.building(camp).unwrap().age, Age::Feudal);
    built(&mut world, BuildingKind::Barracks, 110);
    for (i, kind) in [
        BuildingKind::ArcheryRange,
        BuildingKind::Stable,
        BuildingKind::Blacksmith,
        BuildingKind::Market,
        BuildingKind::WatchTower,
        BuildingKind::PalisadeGate,
    ]
    .into_iter()
    .enumerate()
    {
        let x = 130 + 20 * i as i32;
        let id = built(&mut world, kind, x);
        let b = world.building(id).unwrap();
        let center = at(
            (b.footprint.min.x + b.footprint.max.x) / 2,
            (b.footprint.min.y + b.footprint.max.y) / 2,
        );
        let edge = at(center.x + i32::from(b.los), center.y);
        assert_eq!(world.visibility(PlayerId(0), edge), Visible, "{kind:?}");
    }
}

#[test]
fn watch_tower_does_not_acquire_an_enemy_outside_player_visibility() {
    let (mut world, _, center) = village();
    rich(&mut world);
    age_up(&mut world, center);
    let tower = built(&mut world, BuildingKind::WatchTower, 130);
    let enemy = world
        .spawn_militia(
            PlayerId(1),
            WorldPosition::from_tile_center(at(135, 100)).unwrap(),
        )
        .unwrap();
    world.update_observer(tower, Some((PlayerId(0), at(130, 100), 1)));
    assert!(!world.unit_visible_to(PlayerId(0), enemy));
    advance(&mut world, 80);
    assert_eq!(world.unit(enemy).unwrap().hp, 40);
    world.update_building_vision(tower);
    advance(&mut world, 16);
    assert!(world.unit(enemy).unwrap().hp < 40);
}
