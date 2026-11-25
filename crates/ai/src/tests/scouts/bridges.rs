//! `ai/tests/test_bridges.py`: the optional local crossing heuristic.
use aoe_sdk::{Age, BuildingKind, CommandResult, UnitKind};

use crate::blackboard::Blackboard;
use crate::brain;
use crate::config::Config;
use crate::play::bridges::BridgeManager;
use crate::tests::fake::{
    FakeWorld, Recorder, base, economy, foreign_building, unit_of, with_context,
};
use crate::view::Economy;

/// `setup`: a militia west of a river, an enemy Town Center east of it, and a
/// bridge site across the river at (65, 50)-(70, 50).
fn setup(stone: u32) -> FakeWorld {
    let (mut units, buildings) = base(2);
    units.push(unit_of(200, UnitKind::Militia, (60, 50)));
    let mut world = FakeWorld::new(units, buildings);
    world.economy = Some(Economy {
        wood: 500,
        stone,
        age: Age::Feudal,
        ..economy()
    });
    world.enemy_buildings = vec![foreign_building(900, BuildingKind::TownCenter, (90, 50))];
    world.route_fn = Some(Box::new(|from, to| (to.0 <= 65).then(|| vec![from, to])));
    world.frontier_fn = Some(Box::new(|_, _| Some((65, 50))));
    world.bridge_site_fn = Some(Box::new(|_, _| Some(((65, 50), (70, 50)))));
    world
}

fn enabled() -> Config {
    Config {
        build_bridges: true,
        ..Config::default()
    }
}

fn bridges_of(world: &FakeWorld, config: &Config) -> Recorder {
    let mut board = Blackboard::default();
    let (done, seat) = with_context(world, &[], &mut board, config, |ctx| {
        BridgeManager::default().step(ctx)
    });
    done.expect("the order budget holds");
    seat
}

#[test]
fn test_optional_bridge_orders_use_a_reachable_villager_and_span_cost() {
    let world = setup(200);
    let config = enabled();
    let mut board = Blackboard::default();
    let (outcome, seat) = with_context(&world, &[], &mut board, &config, |ctx| {
        let manager = BridgeManager::default();
        manager.step(ctx).expect("within budget");
        let (spent, busy, sent) = (ctx.spent, ctx.busy.clone(), ctx.orders);
        manager.step(ctx).expect("within budget");
        (spent, busy, sent, ctx.orders)
    });
    let bridges = seat.bridges();
    assert_eq!(bridges.len(), 1);
    let (builders, from, to) = &bridges[0];
    assert_eq!(builders.len(), 1);
    assert_eq!((*from, *to), ((65, 50), (70, 50)));
    assert_eq!(outcome.0, [0, 100, 0, 20]);
    assert!(outcome.1.contains(&builders[0]));
    assert_eq!(outcome.2, outcome.3, "cooldown bounds repeated decisions");
}

#[test]
fn test_bridge_heuristic_is_disabled_by_default_and_respects_resources_and_known_sites() {
    assert!(!Config::default().build_bridges);
    for (on, stone) in [(false, 200), (true, 0)] {
        let config = Config {
            build_bridges: on,
            ..Config::default()
        };
        assert!(bridges_of(&setup(stone), &config).bridges().is_empty());
    }
    let mut world = setup(200);
    world.bridge_site_fn = Some(Box::new(|_, _| None));
    assert!(bridges_of(&world, &enabled()).bridges().is_empty());
}

#[test]
fn test_bridge_refusals_release_the_plan_and_builder() {
    for result in [
        CommandResult::RejectedInvalidPlacement,
        CommandResult::RejectedWorldLimit,
    ] {
        let world = setup(200);
        let config = enabled();
        let mut board = Blackboard::default();
        let (done, _) = with_context(&world, &[], &mut board, &config, |ctx| {
            BridgeManager::default().step(ctx)
        });
        done.expect("the order budget holds");
        let tracked = board
            .tracked
            .values()
            .next()
            .cloned()
            .expect("the bridge order is tracked");
        with_context(&world, &[], &mut board, &config, |ctx| {
            brain::on_rejected(ctx, &tracked, result);
        });
        assert!(board.plans.is_empty());
        assert!(board.builders.is_empty());
    }
}
