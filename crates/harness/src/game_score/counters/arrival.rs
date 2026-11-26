//! Movement orders seen from outside, at tick boundaries: when an order
//! shows, when its unit first moves, and when it gets within two tiles of the
//! goal (the convention of `quality_metrics` and the save replay). Nothing here
//! reads an engine counter: it watches `movement_order` and the unit.
use super::row::{Delay, Events, TickRow};
use std::collections::HashMap;

/// Subunits of one tile.
const TILE: i64 = 1024;
/// A unit within this distance of its goal has arrived.
const ARRIVED_WITHIN: i64 = 2 * TILE;

pub(in crate::game_score) type Point = (i32, i32);

fn near(a: Point, b: Point) -> bool {
    let x = i64::from(a.0) - i64::from(b.0);
    let y = i64::from(a.1) - i64::from(b.1);
    x * x + y * y <= ARRIVED_WITHIN * ARRIVED_WITHIN
}

/// One unit as the observer saw it after a tick.
#[derive(Clone, Copy, Debug)]
pub(in crate::game_score) struct Seen {
    pub id: u32,
    pub position: Point,
    /// The unit's position changed during the tick (`previous_position`, which
    /// the engine resets at the start of every mover's turn, differs from it).
    pub stepped: bool,
    /// The goal of its movement order; `None` without one.
    pub goal: Option<Point>,
}

struct Order {
    goal: Point,
    begun: u64,
    stepped: bool,
    arrived: bool,
    /// The last tick and place the unit showed.
    seen: (u64, Point),
}

/// The orders of every unit that has one.
#[derive(Default)]
pub(in crate::game_score) struct Orders(HashMap<u32, Order>);

impl Orders {
    /// One unit after tick `tick`. `row` and `events` take what it shows.
    pub fn watch(&mut self, tick: u64, unit: Seen, row: &mut TickRow, events: &mut Events) {
        let tracked = self.0.remove(&unit.id);
        let Some(goal) = unit.goal else {
            // The order ended (or never was): near the goal it arrived.
            if let Some(order) = tracked {
                Self::end(order, unit.position, tick, row, events);
            }
            return;
        };
        let mut order = match tracked {
            Some(order) if order.goal == goal => order,
            replaced => {
                if let Some(old) = replaced {
                    Self::end(old, unit.position, tick, row, events);
                }
                row.orders_given += 1;
                Order {
                    goal,
                    begun: tick,
                    stepped: false,
                    arrived: false,
                    seen: (tick, unit.position),
                }
            }
        };
        order.seen = (tick, unit.position);
        if unit.stepped && !order.stepped {
            order.stepped = true;
            events.first_steps.push(Delay {
                tick,
                ticks: tick - order.begun,
            });
        }
        if !order.arrived && near(unit.position, goal) {
            order.arrived = true;
            row.orders_arrived += 1;
            events.arrivals.push(Delay {
                tick,
                ticks: tick - order.begun,
            });
        }
        self.0.insert(unit.id, order);
    }

    /// Ends the orders of units that did not show after tick `tick`: dead,
    /// removed or inside a building. Near the goal where last seen counts as
    /// arrived at the tick of that last sight.
    pub fn finish(&mut self, tick: u64, row: &mut TickRow, events: &mut Events) {
        let gone: Vec<u32> = self
            .0
            .iter()
            .filter(|(_, order)| order.seen.0 != tick)
            .map(|(&id, _)| id)
            .collect();
        for id in gone {
            if let Some(order) = self.0.remove(&id) {
                let (at, place) = order.seen;
                Self::end(order, place, at, row, events);
            }
        }
    }

    /// An order is over at `place`: arrived if it was close enough, else given up.
    fn end(order: Order, place: Point, tick: u64, row: &mut TickRow, events: &mut Events) {
        if order.arrived {
            return;
        }
        if near(place, order.goal) {
            row.orders_arrived += 1;
            events.arrivals.push(Delay {
                tick,
                ticks: tick - order.begun,
            });
        } else {
            row.orders_abandoned += 1;
        }
    }

    /// Orders being followed now.
    #[cfg(test)]
    pub fn open(&self) -> usize {
        self.0.len()
    }
}
