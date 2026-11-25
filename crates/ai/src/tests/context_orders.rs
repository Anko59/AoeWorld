//! The order budget and `Context::order` (`ai/aoe_ai/context.py`).
use aoe_sdk::{BuildingKind, EntityId, Order, TileCoord, UnitKind};

use super::fake::{FakeWorld, base, with_context};
use crate::blackboard::Blackboard;
use crate::config::Config;
use crate::context::OrderBudgetExhausted;

fn world() -> FakeWorld {
    let (units, buildings) = base(3);
    FakeWorld::new(units, buildings)
}

fn gather(unit: u32, resource: u64) -> Order {
    Order::Gather {
        unit: EntityId(unit),
        resource,
    }
}

#[test]
fn an_order_is_sent_through_the_seat_and_remembered_until_its_verdict_arrives() {
    let world = world();
    let mut board = Blackboard::default();
    let (sequence, seat) = with_context(&world, &[], &mut board, &Config::default(), |ctx| {
        ctx.order("gather", gather(1, 7))
    });
    assert_eq!(sequence, Some(1));
    assert_eq!(seat.orders, vec![gather(1, 7)]);
    let tracked = board
        .tracked
        .get(&1)
        .expect("the order is tracked by its sequence");
    assert_eq!(tracked.name, "gather");
    assert_eq!(tracked.order, gather(1, 7));
    assert_eq!(tracked.tick, 100);
}

#[test]
fn orders_bump_the_total_and_the_per_kind_counters() {
    let world = world();
    let mut board = Blackboard::default();
    with_context(&world, &[], &mut board, &Config::default(), |ctx| {
        ctx.order("gather", gather(1, 7));
        ctx.order("gather", gather(2, 7));
        ctx.order("stop", Order::Stop { unit: EntityId(3) });
        assert_eq!(ctx.orders, 3);
    });
    assert_eq!(board.stats.get("orders").copied(), Some(3));
    assert_eq!(board.stats.get("order:gather").copied(), Some(2));
    assert_eq!(board.stats.get("order:stop").copied(), Some(1));
}

#[test]
fn units_given_work_are_busy_for_the_rest_of_the_decision() {
    let world = world();
    let mut board = Blackboard::default();
    let (busy, _) = with_context(&world, &[], &mut board, &Config::default(), |ctx| {
        ctx.order("gather", gather(1, 7));
        ctx.order(
            "build",
            Order::Build {
                builders: vec![EntityId(2), EntityId(3)],
                kind: BuildingKind::House,
                origin: TileCoord::new(60, 60),
            },
        );
        ctx.order(
            "train",
            Order::Train {
                building: EntityId(500),
                kind: UnitKind::Villager,
            },
        );
        ctx.busy.iter().copied().collect::<Vec<u32>>()
    });
    assert_eq!(busy, vec![1, 2, 3], "a Train order gives no unit work");
}

#[test]
fn orders_stop_at_the_per_decision_budget_and_send_nothing_more() {
    let world = world();
    let mut board = Blackboard::default();
    let config = Config {
        orders_per_decision: 3,
        ..Config::default()
    };
    let (results, seat) = with_context(&world, &[], &mut board, &config, |ctx| {
        let results: Vec<Option<u64>> =
            (1..=5).map(|i| ctx.order("gather", gather(i, 7))).collect();
        assert_eq!(ctx.orders, 3);
        assert_eq!(ctx.require_order_budget(), Err(OrderBudgetExhausted));
        results
    });
    assert_eq!(results, vec![Some(1), Some(2), Some(3), None, None]);
    assert_eq!(seat.orders.len(), 3);
    assert_eq!(board.tracked.len(), 3);
    assert_eq!(board.stats.get("orders").copied(), Some(3));
}

#[test]
fn a_stage_limit_ends_its_work_early_but_never_exceeds_the_decision_budget() {
    let world = world();
    let mut board = Blackboard::default();
    let config = Config {
        orders_per_decision: 5,
        ..Config::default()
    };
    let (results, _) = with_context(&world, &[], &mut board, &config, |ctx| {
        ctx.order_limit = 1;
        let first = ctx.order("gather", gather(1, 7));
        let second = ctx.order("gather", gather(2, 7));
        ctx.order_limit = 99;
        let third = ctx.order("gather", gather(3, 7));
        (first, second, third, ctx.orders)
    });
    assert_eq!(results, (Some(1), None, Some(2), 2));
    let mut board = Blackboard::default();
    let (sent, _) = with_context(&world, &[], &mut board, &config, |ctx| {
        ctx.order_limit = 99;
        (1..=9)
            .filter_map(|i| ctx.order("gather", gather(i, 7)))
            .count()
    });
    assert_eq!(sent, 5, "the config's budget caps any stage limit");
}

#[test]
fn spending_needs_order_budget_and_leaves_the_purse_alone_without_it() {
    let world = world();
    let mut board = Blackboard::default();
    let config = Config {
        orders_per_decision: 1,
        ..Config::default()
    };
    let (spent, _) = with_context(&world, &[], &mut board, &config, |ctx| {
        ctx.order("gather", gather(1, 7));
        assert_eq!(ctx.spend([10, 0, 0, 0]), Err(OrderBudgetExhausted));
        assert_eq!(ctx.try_spend([10, 0, 0, 0]), Err(OrderBudgetExhausted));
        ctx.spent
    });
    assert_eq!(spent, [0, 0, 0, 0]);
}

#[test]
fn spend_adds_the_cost_while_orders_remain() {
    let world = world();
    let mut board = Blackboard::default();
    let (spent, _) = with_context(&world, &[], &mut board, &Config::default(), |ctx| {
        assert_eq!(ctx.spend([50, 25, 0, 0]), Ok(()));
        assert_eq!(ctx.spend([50, 0, 10, 0]), Ok(()));
        ctx.spent
    });
    assert_eq!(spent, [100, 25, 10, 0]);
}

#[test]
fn the_oldest_tracked_orders_are_dropped_when_too_many_await_a_verdict() {
    let world = world();
    let mut board = Blackboard::default();
    let config = Config {
        orders_per_decision: 10_000,
        ..Config::default()
    };
    with_context(&world, &[], &mut board, &config, |ctx| {
        for i in 0..4100 {
            ctx.order("stop", Order::Stop { unit: EntityId(i) });
        }
    });
    // The 4097th order tips the table over 4096: the 1024 lowest sequences go.
    assert_eq!(board.tracked.len(), 4100 - 1024);
    assert_eq!(board.tracked.keys().next().copied(), Some(1025));
    assert!(board.tracked.contains_key(&4100));
}
