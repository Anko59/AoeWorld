//! What the managers ask of a decision beyond its data: the clock, the land
//! budget spent, the terrain view and orders that end the manager's work when
//! the order budget is spent.
use aoe_sdk::Order;

use super::{Context, OrderBudgetExhausted};
use crate::view::Tile;

impl Context<'_> {
    /// Game ticks per second.
    pub fn hz(&self) -> u64 {
        u64::from(self.world.tick_hz())
    }

    /// Land queries spent so far this decision.
    pub fn land_queries(&self) -> u32 {
        self.land_queries
    }

    pub fn focus_view(&mut self, tile: Option<Tile>) {
        self.seat.focus_view(tile);
    }

    /// `order`, ending the manager's work when the budget is spent (Python's
    /// `OrderBudgetExhausted` raised from `ctx.order`).
    pub fn order_or_stop(&mut self, name: &str, order: Order) -> Result<u64, OrderBudgetExhausted> {
        self.order(name, order).ok_or(OrderBudgetExhausted)
    }
}
