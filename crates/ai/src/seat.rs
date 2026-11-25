//! The slot's order channel, the Rust counterpart of the Python client's order
//! methods (`move`, `attack`, `train`, ...: one `aoe_sdk::Order` variant each).
use aoe_sdk::Order;

use crate::view::Tile;

pub trait Seat {
    /// Point the slot's terrain view at `tile`, or back to following the
    /// army when `None` (`client.focus_view`). Without a view to move, a seat
    /// ignores it.
    fn focus_view(&mut self, _tile: Option<Tile>) {}

    /// Send one order and return its sequence. The server's verdict arrives
    /// later as `Event::OrderResult { sequence, result }`.
    fn send(&mut self, order: Order) -> u64;
}
