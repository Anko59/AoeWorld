//! `Context` (`ai/aoe_ai/context.py`): what one decision reads, spends and measures.
use aoe_sdk::{Age, BuildingKind, ResourceKind, UnitKind};

use super::fake::{FakeWorld, base, building, economy, node, soldier, unit, unit_of, with_context};
use crate::blackboard::Blackboard;
use crate::config::Config;
use crate::geometry::dist;
use crate::view::{Economy, Unit};

fn world_with(units: Vec<Unit>, economy: Economy) -> FakeWorld {
    let (_, buildings) = base(0);
    let mut world = FakeWorld::new(units, buildings);
    world.economy = Some(economy);
    world
}

#[test]
fn a_context_sorts_units_into_villagers_scouts_and_an_army() {
    let (mut units, buildings) = base(3);
    units.extend([soldier(300, (55, 55)), soldier(301, (56, 55))]);
    let world = FakeWorld::new(units, buildings);
    let mut board = Blackboard::default();
    let (seen, _) = with_context(&world, &[], &mut board, &Config::default(), |ctx| {
        (
            ctx.units.len(),
            ctx.villagers.len(),
            ctx.scouts.len(),
            ctx.army.len(),
            ctx.led.len(),
        )
    });
    assert_eq!(seen, (6, 3, 1, 2, 0));
}

#[test]
fn soldiers_a_general_leads_take_his_orders_not_ours() {
    let (mut units, buildings) = base(2);
    let mut led = soldier(302, (55, 55));
    led.commanded_by = Some(900);
    units.extend([soldier(300, (55, 55)), soldier(301, (56, 55)), led]);
    let world = FakeWorld::new(units, buildings);
    let mut board = Blackboard::default();
    let (ids, _) = with_context(&world, &[], &mut board, &Config::default(), |ctx| {
        let army: Vec<u32> = ctx.army.iter().map(|u| u.id).collect();
        let led: Vec<u32> = ctx.led.iter().map(|u| u.id).collect();
        (army, led)
    });
    assert_eq!(ids, (vec![300, 301], vec![302]));
}

#[test]
fn home_is_the_town_center_then_the_first_villager_then_what_the_board_remembers() {
    let (units, buildings) = base(3);
    let world = FakeWorld::new(units.clone(), buildings);
    let mut board = Blackboard::default();
    let center = world.town_center_center();
    let (home, _) = with_context(&world, &[], &mut board, &Config::default(), |ctx| ctx.home);
    assert_eq!(home, Some(center));
    assert_eq!(board.home, Some(center));

    let mut no_center = FakeWorld::new(units, Vec::new());
    no_center.tick = 120;
    let (home, _) = with_context(&no_center, &[], &mut board, &Config::default(), |ctx| {
        ctx.home
    });
    assert_eq!(home, Some((49, 52)), "the first villager's tile");

    let empty = FakeWorld::new(Vec::new(), Vec::new());
    let mut remembered = Blackboard {
        home: Some((7, 9)),
        ..Blackboard::default()
    };
    let (home, _) = with_context(&empty, &[], &mut remembered, &Config::default(), |ctx| {
        ctx.home
    });
    assert_eq!(home, Some((7, 9)));
}

#[test]
fn queued_units_count_against_free_population() {
    let (units, mut buildings) = base(3);
    buildings[0].queue_length = 2;
    buildings.push(building(600, BuildingKind::Barracks, (60, 50)));
    buildings[1].queue_length = 1;
    let world = FakeWorld::new(units, buildings);
    let mut board = Blackboard::default();
    let (free, _) = with_context(&world, &[], &mut board, &Config::default(), |ctx| {
        assert_eq!(ctx.queued, 3);
        ctx.free_population()
    });
    assert_eq!(free, 10 - 4 - 3);
}

#[test]
fn without_an_economy_nothing_is_affordable_and_no_room_is_free() {
    let (units, buildings) = base(3);
    let mut world = FakeWorld::new(units, buildings);
    world.economy = None;
    let mut board = Blackboard::default();
    let (answers, _) = with_context(&world, &[], &mut board, &Config::default(), |ctx| {
        (
            ctx.can_afford([0, 0, 0, 0]),
            ctx.free_population(),
            ctx.feudal(),
            ctx.castle(),
            ctx.imperial(),
        )
    });
    assert_eq!(answers, (false, 0, false, false, false));
}

#[test]
fn ages_are_cumulative_castle_capabilities_stay_in_the_imperial_age() {
    let ages = [
        (Age::Dark, (false, false, false)),
        (Age::Feudal, (true, false, false)),
        (Age::Castle, (true, true, false)),
        (Age::Imperial, (true, true, true)),
    ];
    for (age, expected) in ages {
        let world = world_with(Vec::new(), Economy { age, ..economy() });
        let mut board = Blackboard::default();
        let (seen, _) = with_context(&world, &[], &mut board, &Config::default(), |ctx| {
            (ctx.feudal(), ctx.castle(), ctx.imperial())
        });
        assert_eq!(seen, expected, "{age:?}");
    }
}

#[test]
fn count_tallies_buildings_of_a_kind_and_optionally_only_finished_ones() {
    let (units, mut buildings) = base(1);
    buildings.push(building(600, BuildingKind::House, (60, 50)));
    let mut unfinished = building(601, BuildingKind::House, (62, 50));
    unfinished.complete = false;
    buildings.push(unfinished);
    let world = FakeWorld::new(units, buildings);
    let mut board = Blackboard::default();
    let (counts, _) = with_context(&world, &[], &mut board, &Config::default(), |ctx| {
        (
            ctx.count(BuildingKind::House, false),
            ctx.count(BuildingKind::House, true),
        )
    });
    assert_eq!(counts, (2, 1));
}

#[test]
fn spending_is_tracked_and_a_cost_beyond_the_stock_is_not_affordable() {
    let world = world_with(Vec::new(), economy());
    let mut board = Blackboard::default();
    let config = Config::default();
    let (spent, _) = with_context(&world, &[], &mut board, &config, |ctx| {
        assert!(ctx.can_afford([200, 200, 100, 200]));
        assert!(!ctx.can_afford([201, 0, 0, 0]));
        assert_eq!(ctx.try_spend([150, 0, 0, 0]), Ok(true));
        assert!(!ctx.can_afford([100, 0, 0, 0]), "only 50 food is left");
        assert_eq!(ctx.try_spend([100, 0, 0, 0]), Ok(false));
        ctx.spent
    });
    assert_eq!(spent, [150, 0, 0, 0]);
}

#[test]
fn reserve_sets_aside_at_most_what_is_left() {
    let world = world_with(Vec::new(), economy());
    let mut board = Blackboard::default();
    let (spent, _) = with_context(&world, &[], &mut board, &Config::default(), |ctx| {
        ctx.reserve([500, 50, 0, 0]);
        ctx.reserve([500, 0, 0, 0]);
        ctx.spent
    });
    assert_eq!(spent, [200, 50, 0, 0]);
}

#[test]
fn resources_skip_nodes_a_gather_was_refused_for() {
    let mut world = world_with(Vec::new(), economy());
    world.nodes = vec![
        node(1, ResourceKind::Wood, (56, 52)),
        node(3, ResourceKind::Wood, (58, 52)),
        node(2, ResourceKind::Food, (44, 52)),
    ];
    let mut board = Blackboard::default();
    board.bad_resources.insert(1);
    let (ids, _) = with_context(&world, &[], &mut board, &Config::default(), |ctx| {
        let wood: Vec<u64> = ctx
            .resources(ResourceKind::Wood)
            .iter()
            .map(|n| n.id)
            .collect();
        let food: Vec<u64> = ctx
            .resources(ResourceKind::Food)
            .iter()
            .map(|n| n.id)
            .collect();
        (wood, food)
    });
    assert_eq!(ids, (vec![3], vec![2]));
}

#[test]
fn a_huge_list_of_refused_nodes_is_forgotten_so_the_memory_stays_bounded() {
    let mut world = world_with(Vec::new(), economy());
    world.nodes = vec![node(1, ResourceKind::Wood, (56, 52))];
    let mut board = Blackboard::default();
    board.bad_resources.extend(1..=8193u64);
    let (ids, _) = with_context(&world, &[], &mut board, &Config::default(), |ctx| {
        ctx.resources(ResourceKind::Wood)
            .iter()
            .map(|n| n.id)
            .collect::<Vec<u64>>()
    });
    assert_eq!(ids, vec![1]);
    assert!(board.bad_resources.is_empty());
}

#[test]
fn dropoff_distance_is_to_the_nearest_finished_building_that_takes_the_resource() {
    let (units, mut buildings) = base(1);
    let mut unfinished_mill = building(600, BuildingKind::Mill, (54, 50));
    unfinished_mill.complete = false;
    buildings.extend([
        unfinished_mill,
        building(601, BuildingKind::LumberCamp, (58, 60)),
    ]);
    let world = FakeWorld::new(units, buildings);
    let center = world.town_center_center();
    let camp = world.buildings[2].center();
    let mut board = Blackboard::default();
    let food = node(1, ResourceKind::Food, (56, 52));
    let wood = node(2, ResourceKind::Wood, (58, 58));
    let (seen, _) = with_context(&world, &[], &mut board, &Config::default(), |ctx| {
        (ctx.dropoff_distance(&food), ctx.dropoff_distance(&wood))
    });
    assert_eq!(
        seen.0,
        dist((56, 52), center),
        "an unfinished mill takes no food"
    );
    assert_eq!(seen.1, dist((58, 58), camp).min(dist((58, 58), center)));
}

#[test]
fn dropoff_distance_follows_the_sites_when_they_change_between_decisions() {
    let (units, buildings) = base(1);
    let mut world = FakeWorld::new(units, buildings);
    let food = node(1, ResourceKind::Food, (56, 52));
    let mut board = Blackboard::default();
    let config = Config::default();
    let (before, _) = with_context(&world, &[], &mut board, &config, |ctx| {
        ctx.dropoff_distance(&food)
    });
    world
        .buildings
        .push(building(600, BuildingKind::Mill, (56, 54)));
    let mill = world.buildings[1].center();
    let (after, _) = with_context(&world, &[], &mut board, &config, |ctx| {
        ctx.dropoff_distance(&food)
    });
    assert!(after < before);
    assert_eq!(after, dist((56, 52), mill));
}

#[test]
fn nothing_to_drop_at_means_an_enormous_distance() {
    let world = FakeWorld::new(vec![unit(1, (50, 50))], Vec::new());
    let mut board = Blackboard::default();
    let gold = node(1, ResourceKind::Gold, (56, 52));
    let (far, _) = with_context(&world, &[], &mut board, &Config::default(), |ctx| {
        ctx.dropoff_distance(&gold)
    });
    assert_eq!(far, 1e9);
}

#[test]
fn settlement_distance_is_to_the_nearest_finished_town_center_or_else_home() {
    let (units, buildings) = base(1);
    let world = FakeWorld::new(units, buildings);
    let center = world.town_center_center();
    let mut board = Blackboard::default();
    let (seen, _) = with_context(&world, &[], &mut board, &Config::default(), |ctx| {
        (
            ctx.settlement_distance((70, 50)),
            ctx.settlement_distance((70, 50)),
        )
    });
    assert_eq!(seen, (dist((70, 50), center), dist((70, 50), center)));

    let homeless = FakeWorld::new(Vec::new(), Vec::new());
    let mut board = Blackboard::default();
    let (far, _) = with_context(&homeless, &[], &mut board, &Config::default(), |ctx| {
        ctx.settlement_distance((70, 50))
    });
    assert_eq!(far, 1e9);
    let mut board = Blackboard {
        home: Some((60, 50)),
        ..Blackboard::default()
    };
    let (near, _) = with_context(&homeless, &[], &mut board, &Config::default(), |ctx| {
        ctx.settlement_distance((70, 50))
    });
    assert_eq!(near, 10.0);
}

#[test]
fn a_refused_destination_is_not_chosen_again_for_the_rally_point() {
    let world = world_with(Vec::new(), economy());
    let mut board = Blackboard::default();
    let config = Config::default();
    let (answers, _) = with_context(&world, &[], &mut board, &config, |ctx| {
        let free = ctx.unrefused((100, 100));
        ctx.board.bad_moves.insert((100, 100));
        let second = ctx.unrefused((100, 100));
        ctx.board.bad_moves.insert((102, 100));
        let third = ctx.unrefused((100, 100));
        (free, second, third)
    });
    assert_eq!(answers, ((100, 100), (102, 100), (100, 102)));
}

#[test]
fn an_unrefused_tile_near_the_edge_stays_on_the_map() {
    let world = world_with(Vec::new(), economy());
    let mut board = Blackboard::default();
    board.bad_moves.insert((298, 150));
    let (tile, _) = with_context(&world, &[], &mut board, &Config::default(), |ctx| {
        ctx.unrefused((298, 150))
    });
    assert_eq!(tile, (296, 150));
}

#[test]
fn combat_lines_count_soldiers_warships_queued_units_and_garrisons_by_line() {
    let (mut units, mut buildings) = base(4);
    let mut dead = soldier(310, (55, 55));
    dead.alive = false;
    units.extend([
        soldier(300, (55, 55)),
        soldier(301, (56, 55)),
        unit_of(302, UnitKind::WarGalley, (80, 80)),
        dead,
    ]);
    buildings[0].queue_length = 1;
    buildings[0].queued_units = vec![UnitKind::Spearman];
    buildings[0].garrison = vec![
        (400, UnitKind::Archer, 30, 30),
        (401, UnitKind::Spearman, 0, 30),
        (402, UnitKind::Villager, 25, 25),
    ];
    let world = FakeWorld::new(units, buildings);
    let mut board = Blackboard::default();
    let (total, _) = with_context(&world, &[], &mut board, &Config::default(), |ctx| {
        ctx.combat_lines().values().sum::<u32>()
    });
    assert_eq!(
        total,
        2 + 1 + 1 + 1,
        "two soldiers, a warship, a queued spearman and a garrisoned archer"
    );
}

#[test]
fn a_late_economy_is_a_castle_age_one_with_enough_villagers() {
    let (units, buildings) = base(127);
    let mut world = FakeWorld::new(units, buildings);
    world.economy = Some(Economy {
        age: Age::Castle,
        ..economy()
    });
    let mut board = Blackboard::default();
    let config = Config::default();
    let (before, _) = with_context(&world, &[], &mut board, &config, |ctx| ctx.late_economy);
    world.units.push(unit(999, (50, 50)));
    let (after, _) = with_context(&world, &[], &mut board, &config, |ctx| ctx.late_economy);
    world.economy = Some(Economy {
        age: Age::Feudal,
        ..economy()
    });
    let (feudal, _) = with_context(&world, &[], &mut board, &config, |ctx| ctx.late_economy);
    assert_eq!((before, after, feudal), (false, true, false));
}

#[test]
fn memory_of_units_and_buildings_that_are_gone_is_pruned_when_a_decision_starts() {
    let (mut units, buildings) = base(2);
    units.push(soldier(300, (55, 55)));
    let world = FakeWorld::new(units, buildings);
    let mut board = Blackboard::default();
    board.marched.extend([(300, 50), (999, 50)]);
    board.generals.extend([300, 888]);
    board.seated.extend([(300, 500), (301, 999), (888, 500)]);
    board.land_failures.insert(((1, 1), (2, 2)), 50);
    board.land_failures.insert(((3, 3), (4, 4)), 5_000);
    with_context(&world, &[], &mut board, &Config::default(), |_| ());
    assert_eq!(board.marched.keys().copied().collect::<Vec<_>>(), vec![300]);
    assert_eq!(
        board.generals.iter().copied().collect::<Vec<_>>(),
        vec![300]
    );
    assert_eq!(board.seated.keys().copied().collect::<Vec<_>>(), vec![300]);
    assert_eq!(
        board.land_failures.len(),
        1,
        "an expired failure is dropped"
    );
}
