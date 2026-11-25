//! The everyday computer player, ported from the Python AI in `ai/aoe_ai`
//! (docs/hierarchy/spec.md section 7: Rust rulers replace the Python AIs, and
//! the Python SDK stays for improvement tournaments).
//!
//! Like the Python AI it reads only what its slot's fog of war shows, through
//! the SDK's world model, and sends ordinary orders: the same strategy, ported
//! manager by manager, without a redesign. Decisions are deterministic: the
//! same world and events give the same orders.
pub mod blackboard;
pub mod brain;
pub mod config;
pub mod context;
pub mod geometry;
pub mod play;
pub mod roster;
pub mod sdk_view;
pub mod seat;
pub mod view;
pub mod worker_view;

pub use brain::Brain;
pub use config::Config;
pub use seat::Seat;
pub use view::View;

#[cfg(test)]
mod tests;
