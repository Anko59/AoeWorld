//! The slot's order channel, the Rust counterpart of the Python client's order
//! methods (`move`, `attack`, `train`, ...: one `aoe_sdk::Order` variant each).
use aoe_sdk::Order;

pub trait Seat {
    /// Send one order and return its sequence. The server's verdict arrives
    /// later as `Event::OrderResult { sequence, result }`.
    fn send(&mut self, order: Order) -> u64;
}
