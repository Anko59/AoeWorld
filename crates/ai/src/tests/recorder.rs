//! `fakes.FakeClient` and `Context` building for one closure: the order recorder.
use aoe_sdk::{Event, Formation, Order};

use super::fake::FakeWorld;
use crate::blackboard::Blackboard;
use crate::config::Config;
use crate::context::Context;
use crate::seat::Seat;
use crate::view::Tile;

/// `fakes.FakeClient`: records orders and returns increasing sequences (1, 2, ...).
#[derive(Default)]
pub struct Recorder {
    pub orders: Vec<Order>,
}

/// One `formation_move`: members, destination, formation, `attack_move`.
pub type Group = (Vec<u32>, Tile, Formation, bool);

impl Recorder {
    /// `client.named("attack")`: (soldier, target) in the order sent.
    pub fn attacks(&self) -> Vec<(u32, u32)> {
        self.orders
            .iter()
            .filter_map(|o| match o {
                Order::Attack { unit, target } => Some((unit.0, target.0)),
                _ => None,
            })
            .collect()
    }

    /// `client.named("move")`: (unit, destination) in the order sent.
    pub fn moves(&self) -> Vec<(u32, Tile)> {
        self.orders
            .iter()
            .filter_map(|o| match o {
                Order::Move { unit, to } => Some((unit.0, (to.x, to.y))),
                _ => None,
            })
            .collect()
    }

    /// `client.named("stop")`.
    pub fn stops(&self) -> Vec<u32> {
        self.orders
            .iter()
            .filter_map(|o| match o {
                Order::Stop { unit } => Some(unit.0),
                _ => None,
            })
            .collect()
    }

    /// `client.named("formation_move")` with its `attack_move` option.
    pub fn formation_moves(&self) -> Vec<Group> {
        self.orders
            .iter()
            .filter_map(|o| match o {
                Order::FormationMove {
                    units,
                    formation,
                    to,
                    attack_move,
                    ..
                } => Some((
                    units.iter().map(|u| u.0).collect(),
                    (to.x, to.y),
                    *formation,
                    *attack_move,
                )),
                _ => None,
            })
            .collect()
    }

    /// `client.named("build_bridge")`: (builders, from, to).
    pub fn bridges(&self) -> Vec<(Vec<u32>, Tile, Tile)> {
        self.orders
            .iter()
            .filter_map(|o| match o {
                Order::BuildBridge {
                    builders, from, to, ..
                } => Some((
                    builders.iter().map(|u| u.0).collect(),
                    (from.x, from.y),
                    (to.x, to.y),
                )),
                _ => None,
            })
            .collect()
    }
}

impl Seat for Recorder {
    fn send(&mut self, order: Order) -> u64 {
        self.orders.push(order);
        self.orders.len() as u64
    }
}

/// Build a `Context` for one closure, as `Context(client, decision, board, config)` did.
pub fn with_context<R>(
    world: &FakeWorld,
    events: &[Event],
    board: &mut Blackboard,
    config: &Config,
    f: impl FnOnce(&mut Context<'_>) -> R,
) -> (R, Recorder) {
    let mut seat = Recorder::default();
    let result = {
        let mut ctx = Context::new(world, events, board, config, &mut seat);
        f(&mut ctx)
    };
    (result, seat)
}
