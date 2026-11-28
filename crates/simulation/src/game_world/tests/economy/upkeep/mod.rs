use super::{buildings::set_stock, *};

const PLAYER: PlayerId = PlayerId(0);

fn food(world: &GameWorld) -> u32 {
    world.player_economy(PLAYER).unwrap().stockpile.food
}

#[test]
fn a_market_delivery_clears_the_shortage_even_if_spent_in_the_same_tick() {
    let (mut world, _, center) = village();
    world.complete_research(PLAYER, crate::Tech::FeudalAge, &mut Vec::new());
    let mut insert = |kind, x| {
        world
            .insert_building(PLAYER, kind, TileRect::from_xywh(x, 82, 4, 4), true)
            .unwrap()
    };
    let market = insert(BuildingKind::Market, 82);
    let range = insert(BuildingKind::ArcheryRange, 92);
    set_stock(&mut world, 0, 500, 500, 0);
    let minute = world.config().tick_hz * 60;
    advance(&mut world, minute);
    assert_eq!(world.food_upkeep(PLAYER), (1, true));

    world
        .issue_market_buy(PLAYER, market, ResourceKind::Food)
        .unwrap();
    world
        .issue_train(PLAYER, center, UnitKind::Villager)
        .unwrap();
    world
        .issue_train(PLAYER, center, UnitKind::Villager)
        .unwrap();
    assert_eq!(food(&world), 0);
    assert_eq!(world.food_upkeep(PLAYER), (1, false));
    world.issue_train(PLAYER, range, UnitKind::Archer).unwrap();
}

#[test]
fn supplies_change_the_speed_of_an_attack_cooldown_in_progress() {
    let (mut world, _, _) = village();
    world.set_tick_hz(10);
    let at = |x| WorldPosition::from_tile_center(TileCoord::new(x, 96)).unwrap();
    let attacker = world.spawn_kind(PLAYER, UnitKind::Militia, at(96)).unwrap();
    set_stock(&mut world, 0, 0, 0, 0);
    let period = world.attack_period_ticks(&world.unit(attacker).unwrap());
    let first_bill = 300;
    advance(&mut world, first_bill - 8);
    let victim = world.spawn_villager(PlayerId(1), at(97)).unwrap();
    world.issue_attack(PLAYER, attacker, victim).unwrap();
    let initial_hp = world.unit(victim).unwrap().hp;
    while world.unit(victim).unwrap().hp == initial_hp && world.tick().0 < u64::from(first_bill) {
        world.advance();
    }
    let first_hit = world.tick().0 as u32;
    let hp = world.unit(victim).unwrap().hp;
    assert!(hp < initial_hp && first_hit < first_bill);

    advance(&mut world, period);
    assert_eq!(world.food_upkeep(PLAYER), (2, true));
    assert_eq!(world.unit(victim).unwrap().hp, hp);

    let tick = world.tick().0;
    world.economy.credit_delivery(
        PLAYER,
        CarriedResource {
            kind: ResourceKind::Food,
            amount: 10,
        },
        tick,
        10,
    );
    let slow_ticks = first_hit + period - first_bill + 1;
    let remaining = slow_ticks.div_ceil(2);
    advance(&mut world, remaining - 1);
    assert_eq!(world.unit(victim).unwrap().hp, hp);
    world.advance();
    assert!(world.unit(victim).unwrap().hp < hp);
}

#[test]
fn upkeep_charges_living_army_continuously_and_excludes_workers() {
    let (mut world, scout, _) = village();
    world.set_tick_hz(10);
    world
        .spawn_kind(
            PLAYER,
            UnitKind::Militia,
            WorldPosition::from_tile_center(TileCoord::new(95, 95)).unwrap(),
        )
        .unwrap();
    assert_eq!(world.food_upkeep(PLAYER), (2, false));
    let initial = food(&world);
    advance(&mut world, 299);
    assert_eq!(food(&world), initial);
    advance(&mut world, 1);
    assert_eq!(food(&world), initial - 1);
    world.kill_unit(world.lookup[&scout]);
    assert_eq!(world.food_upkeep(PLAYER), (1, false));
    advance(&mut world, 600);
    assert_eq!(food(&world), initial - 2);
}

#[test]
fn empty_stores_block_recruitment_until_food_returns() {
    let (mut world, _, center) = village();
    world.complete_research(PLAYER, crate::Tech::FeudalAge, &mut Vec::new());
    let range = world
        .insert_building(
            PLAYER,
            BuildingKind::ArcheryRange,
            TileRect::new(TileCoord::new(82, 82), TileCoord::new(85, 85)),
            true,
        )
        .unwrap();
    world
        .economy
        .players
        .get_mut(&PLAYER)
        .unwrap()
        .stockpile
        .food = 0;
    let minute = world.config().tick_hz * 60;
    advance(&mut world, minute);
    assert_eq!(world.food_upkeep(PLAYER), (1, true));
    let (resumed, _) = super::super::snapshot_roundtrip::roundtrip(&world, super::super::world());
    assert_eq!(resumed.food_upkeep(PLAYER), (1, true));
    assert_eq!(resumed.player_economy(PLAYER).unwrap().upkeep_slow_ticks, 1);
    assert_eq!(
        world.issue_train(PLAYER, range, UnitKind::Archer),
        Err(EconomyError::InsufficientFood)
    );

    world
        .economy
        .players
        .get_mut(&PLAYER)
        .unwrap()
        .stockpile
        .food = 200;
    assert_eq!(world.food_upkeep(PLAYER), (1, false));
    world
        .issue_train(PLAYER, center, UnitKind::Villager)
        .unwrap();
    world.issue_train(PLAYER, range, UnitKind::Archer).unwrap();
}

#[test]
fn upkeep_fraction_survives_save_and_a_clock_change() {
    let (mut world, _, _) = village();
    world.set_tick_hz(10);
    advance(&mut world, 597);
    let initial = food(&world);
    let mut fresh = super::super::world();
    fresh.set_tick_hz(20);
    let (mut restored, report) = super::super::snapshot_roundtrip::roundtrip(&world, fresh);
    assert_eq!(report.units_lost, 0);
    assert_eq!(restored.food_upkeep(PLAYER), (1, false));
    advance(&mut restored, 5);
    assert_eq!(food(&restored), initial);
    advance(&mut restored, 1);
    assert_eq!(food(&restored), initial - 1);
}

#[test]
fn a_sheltered_archer_still_costs_food_after_loading() {
    let (mut world, _, _) = village();
    let tower = world
        .insert_building(
            PLAYER,
            BuildingKind::WatchTower,
            TileRect::from_xywh(100, 100, 1, 1),
            true,
        )
        .unwrap();
    let archer = world
        .spawn_kind(
            PLAYER,
            UnitKind::Archer,
            WorldPosition::from_tile_center(TileCoord::new(99, 100)).unwrap(),
        )
        .unwrap();
    world.issue_garrison(PLAYER, archer, tower).unwrap();
    world.advance();
    assert!(world.unit(archer).is_none());
    assert_eq!(world.food_upkeep(PLAYER), (2, false));

    let (mut restored, _) =
        super::super::snapshot_roundtrip::roundtrip(&world, super::super::world());
    let initial = food(&restored);
    let half_minute = restored.config().tick_hz * 30;
    advance(&mut restored, half_minute);
    assert_eq!(restored.food_upkeep(PLAYER), (2, false));
    assert_eq!(food(&restored), initial - 1);
}
