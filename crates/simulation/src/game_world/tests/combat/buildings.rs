//! Buildings take damage like units and the unit-cost hook for the Barracks.
use super::*;
use crate::{BuildingKind, UnitKind};

fn two_centers() -> (GameWorld, EntityId, EntityId) {
    let mut world = world();
    let ours = world
        .place_town_center(PlayerId(0), TileCoord::new(60, 60))
        .unwrap();
    let theirs = world
        .place_town_center(PlayerId(1), TileCoord::new(120, 60))
        .unwrap();
    (world, ours, theirs)
}

#[test]
fn a_town_center_has_2400_hit_points_and_a_militia_scratches_one_per_hit() {
    let (mut world, ours, theirs) = two_centers();
    let center = world.building(theirs).unwrap();
    assert_eq!(
        (center.kind, center.hp, center.max_hp),
        (BuildingKind::TownCenter, 2400, 2400)
    );
    let militia = world.spawn_militia(PlayerId(0), at(110, 62)).unwrap();
    world.issue_attack(PlayerId(0), militia, theirs).unwrap();
    assert_eq!(world.unit(militia).unwrap().attack_target, Some(theirs));
    let ticks = ticks_until(&mut world, 400, |w| w.building(theirs).unwrap().hp < 2400);
    assert!(
        (150..=300).contains(&ticks),
        "first hit after {ticks} ticks"
    );
    let center = world.building(theirs).unwrap();
    assert_eq!(center.hp, 2399);
    assert_eq!(center.last_hit.map(|hit| hit.attacker), Some(militia));
    assert_eq!(center.last_hit.map(|hit| hit.tick), Some(world.tick()));
    // Standing on the perimeter, within one tile of the footprint.
    assert!(world.unit(militia).unwrap().position.x >= 119 * 1_024);
    advance(&mut world, 40);
    assert_eq!(world.building(theirs).unwrap().hp, 2398);
    assert_eq!(world.building(ours).unwrap().hp, 2400);
    assert!(matches!(
        world.issue_attack(PlayerId(0), militia, ours),
        Err(EconomyError::InvalidTarget)
    ));
}

#[test]
fn a_building_at_zero_hit_points_is_destroyed_and_frees_its_tiles_and_cap() {
    let (mut world, _, theirs) = two_centers();
    assert_eq!(world.player_economy(PlayerId(1)).unwrap().population_cap, 5);
    assert!(!world.tile_passable(TileCoord::new(121, 61)));
    world.economy.buildings.get_mut(&theirs).unwrap().hp = 2;
    let militia = world.spawn_militia(PlayerId(0), at(118, 62)).unwrap();
    world.issue_attack(PlayerId(0), militia, theirs).unwrap();
    ticks_until(&mut world, 400, |w| w.building(theirs).is_none());
    assert!(world.tile_passable(TileCoord::new(121, 61)));
    assert_eq!(world.player_economy(PlayerId(1)).unwrap().population_cap, 0);
    assert_eq!(world.buildings().count(), 1);
    let militia = world.unit(militia).unwrap();
    assert_eq!(
        (militia.attack_target, militia.task),
        (None, UnitTask::Idle)
    );
    // The freed tiles are walkable again.
    world.issue_move(militia.id, at(122, 62)).unwrap();
    advance(&mut world, 100);
    assert_eq!(world.unit(militia.id).unwrap().position, at(122, 62));
}

#[test]
fn militia_cost_is_paid_all_or_nothing_and_refunded_on_cancel() {
    let (mut world, _, _) = two_centers();
    let set = |world: &mut GameWorld, food, gold| {
        let stock = &mut world
            .economy
            .players
            .get_mut(&PlayerId(0))
            .unwrap()
            .stockpile;
        stock.food = food;
        stock.gold = gold;
    };
    let training = GameWorld::unit_training(UnitKind::Militia).unwrap();
    assert_eq!(training.seconds, 21);
    set(&mut world, 59, 100);
    assert!(matches!(
        world.pay_unit_cost(PlayerId(0), UnitKind::Militia),
        Err(EconomyError::InsufficientFood)
    ));
    set(&mut world, 100, 19);
    assert!(matches!(
        world.pay_unit_cost(PlayerId(0), UnitKind::Militia),
        Err(EconomyError::InsufficientResources)
    ));
    assert_eq!(
        world.player_economy(PlayerId(0)).unwrap().stockpile.food,
        100
    );
    set(&mut world, 100, 25);
    world.pay_unit_cost(PlayerId(0), UnitKind::Militia).unwrap();
    let stock = world.player_economy(PlayerId(0)).unwrap().stockpile;
    assert_eq!((stock.food, stock.gold), (40, 5));
    world.refund_unit_cost(PlayerId(0), UnitKind::Militia);
    let stock = world.player_economy(PlayerId(0)).unwrap().stockpile;
    assert_eq!((stock.food, stock.gold), (100, 25));
    assert!(matches!(
        world.pay_unit_cost(PlayerId(0), UnitKind::Villager),
        Err(EconomyError::WrongUnitKind)
    ));
}

#[test]
fn population_may_exceed_the_cap_after_a_town_center_falls_but_training_is_refused() {
    let (mut world, ours, _) = two_centers();
    let second = world
        .place_town_center(PlayerId(0), TileCoord::new(60, 100))
        .unwrap();
    for x in 0..7 {
        world.spawn_villager(PlayerId(0), at(70 + x, 80)).unwrap();
    }
    assert_eq!(
        world.player_economy(PlayerId(0)).unwrap().population_cap,
        10
    );
    world.economy.buildings.get_mut(&ours).unwrap().hp = 1;
    let raider = world.spawn_militia(PlayerId(1), at(58, 62)).unwrap();
    world.issue_attack(PlayerId(1), raider, ours).unwrap();
    ticks_until(&mut world, 400, |w| w.building(ours).is_none());
    let economy = world.player_economy(PlayerId(0)).unwrap();
    assert_eq!((economy.population_used, economy.population_cap), (7, 5));
    // Existing units survive; only new training is blocked.
    assert_eq!(world.units().filter(|u| u.player == PlayerId(0)).count(), 7);
    world
        .economy
        .players
        .get_mut(&PlayerId(0))
        .unwrap()
        .stockpile
        .food = 500;
    assert!(matches!(
        world.issue_train(PlayerId(0), second, UnitKind::Villager),
        Err(EconomyError::PopulationCap)
    ));
}

#[test]
fn attack_and_construct_orders_replace_each_other_without_resuming_old_work() {
    let (mut world, _, _) = two_centers();
    world
        .economy
        .players
        .get_mut(&PlayerId(0))
        .unwrap()
        .stockpile
        .wood = 25;
    let builder = world.spawn_villager(PlayerId(0), at(99, 100)).unwrap();
    let house = world
        .issue_build(
            PlayerId(0),
            &[builder],
            BuildingKind::House,
            TileCoord::new(100, 100),
        )
        .unwrap();
    advance(&mut world, 2);
    assert_eq!(world.unit(builder).unwrap().task, UnitTask::Building);
    let work = world.building(house).unwrap().work_done;
    let enemy = world.spawn_militia(PlayerId(1), at(99, 100)).unwrap();
    world.issue_attack(PlayerId(0), builder, enemy).unwrap();
    assert!(!world.economy.builders.contains_key(&builder));
    advance(&mut world, 2);
    assert_eq!(world.building(house).unwrap().work_done, work);
    world
        .issue_construct(PlayerId(0), &[builder], house)
        .unwrap();
    assert!(!world.combat.attackers.contains_key(&builder));
    assert_eq!(world.unit(builder).unwrap().attack_target, None);
    advance(&mut world, 2);
    assert!(world.building(house).unwrap().work_done > work);
}

#[test]
fn working_builders_do_not_retaliate_and_death_releases_their_job() {
    let (mut world, _, _) = two_centers();
    world
        .economy
        .players
        .get_mut(&PlayerId(0))
        .unwrap()
        .stockpile
        .wood = 25;
    let builder = world.spawn_villager(PlayerId(0), at(99, 100)).unwrap();
    let house = world
        .issue_build(
            PlayerId(0),
            &[builder],
            BuildingKind::House,
            TileCoord::new(100, 100),
        )
        .unwrap();
    advance(&mut world, 2);
    let enemy = world.spawn_militia(PlayerId(1), at(99, 100)).unwrap();
    world.issue_attack(PlayerId(1), enemy, builder).unwrap();
    world.advance();
    assert_eq!(world.unit(builder).unwrap().task, UnitTask::Building);
    assert_eq!(world.unit(builder).unwrap().attack_target, None);
    assert_eq!(world.unit(enemy).unwrap().hp, 40);
    ticks_until(&mut world, 400, |w| w.unit(builder).unwrap().hp == 0);
    assert!(!world.economy.builders.contains_key(&builder));
    let work = world.building(house).unwrap().work_done;
    advance(&mut world, 2);
    assert_eq!(world.building(house).unwrap().work_done, work);
}

#[test]
fn destroying_a_house_uses_its_armour_and_removes_only_its_housing() {
    let (mut world, _, _) = two_centers();
    world
        .economy
        .players
        .get_mut(&PlayerId(1))
        .unwrap()
        .stockpile
        .wood = 25;
    let builder = world.spawn_villager(PlayerId(1), at(99, 100)).unwrap();
    let house = world
        .issue_build(
            PlayerId(1),
            &[builder],
            BuildingKind::House,
            TileCoord::new(100, 100),
        )
        .unwrap();
    advance(&mut world, 600);
    assert!(world.building(house).unwrap().is_complete());
    assert_eq!(
        world.player_economy(PlayerId(1)).unwrap().population_cap,
        10
    );
    assert!(!world.tile_passable(TileCoord::new(100, 100)));
    world.economy.buildings.get_mut(&house).unwrap().hp = 4;
    let enemy = world.spawn_militia(PlayerId(0), at(99, 100)).unwrap();
    world.issue_attack(PlayerId(0), enemy, house).unwrap();
    world.advance();
    assert!(world.building(house).is_none());
    assert!(world.tile_passable(TileCoord::new(100, 100)));
    assert_eq!(world.player_economy(PlayerId(1)).unwrap().population_cap, 5);
}
