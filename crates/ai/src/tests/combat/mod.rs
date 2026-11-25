//! The combat manager, ported from `ai/tests/test_combat_orders.py`,
//! `test_predator_defence.py`, `test_navigation.py` and the combat cases of
//! `test_logic.py` and `test_review.py`. A test that Python runs through one
//! manager calls `CombatManager` (or `march`) on a `Context` here too; only
//! the ones Python plays through `Brain.decide` go through `Brain::decide`.
mod brain_level;
mod navigation;
mod orders;
mod predator;
mod raid;

use aoe_sdk::{BuildingKind, Event, Order, UnitKind};

use super::fake::{FakeWorld, Recorder, economy, enemy, foreign_building, soldier, with_context};
use crate::blackboard::Blackboard;
use crate::config::Config;
use crate::play::combat::CombatManager;
use crate::view::{Building, Economy, Tile, Unit};

/// `CombatManager().step(Context(client, decision(world, events), board, config))`.
pub fn step(
    world: &FakeWorld,
    events: &[Event],
    board: &mut Blackboard,
    config: &Config,
) -> Recorder {
    let (done, seat) = with_context(world, events, board, config, |ctx| {
        CombatManager::default().step(ctx)
    });
    done.expect("the order budget holds");
    seat
}

/// `count` spearmen of ours with ids `first`, `first + 1`, ... all on `tile`.
pub fn spearmen(first: u32, count: u32, tile: Tile) -> Vec<Unit> {
    (0..count).map(|i| soldier(first + i, tile)).collect()
}

/// An enemy house, the Python tests' stand-in for "a known enemy building".
pub fn house(id: u32, tile: Tile) -> Building {
    foreign_building(id, BuildingKind::House, tile)
}

/// A remembered building the army cannot see now (`visible=False`).
pub fn hidden(building: Building) -> Building {
    Building {
        visible: false,
        ..building
    }
}

pub fn ids(units: &[Unit]) -> Vec<u32> {
    units.iter().map(|u| u.id).collect()
}

/// A world with our `units` and `buildings` and the given population.
pub fn populated(units: Vec<Unit>, buildings: Vec<Building>, pop: u32, cap: u32) -> FakeWorld {
    let mut world = FakeWorld::new(units, buildings);
    world.economy = Some(Economy {
        population: pop,
        population_cap: cap,
        ..economy()
    });
    world
}

/// `test_logic.army_moves`: every plain move, then every member of every
/// formation move with that move's destination.
pub fn army_moves(seat: &Recorder) -> Vec<(u32, Tile)> {
    let mut moves = seat.moves();
    for (members, to, ..) in seat.formation_moves() {
        moves.extend(members.into_iter().map(|id| (id, to)));
    }
    moves
}

/// Every order about the soldier `id`, in the order sent.
pub fn about(seat: &Recorder, id: u32) -> Vec<Order> {
    seat.orders
        .iter()
        .filter(|o| match o {
            Order::Attack { unit, .. } | Order::Move { unit, .. } | Order::Stop { unit } => {
                unit.0 == id
            }
            Order::FormationMove { units, .. } => units.iter().any(|u| u.0 == id),
            _ => false,
        })
        .cloned()
        .collect()
}

/// An opposing militia of owner 2, as `test_combat_orders.raid` builds it.
pub fn raider(id: u32, tile: Tile) -> Unit {
    Unit {
        owner: 2,
        ..enemy(id, UnitKind::Militia, tile)
    }
}
