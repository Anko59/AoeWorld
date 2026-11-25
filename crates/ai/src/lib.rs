//! The everyday computer player, ported from the Python AI in `ai/aoe_ai`
//! (docs/hierarchy/spec.md section 7: Rust rulers replace the Python AIs, and
//! the Python SDK stays for improvement tournaments).
//!
//! Like the Python AI it reads only what its slot's fog of war shows, through
//! the SDK's world model, and sends ordinary orders: the same strategy, ported
//! manager by manager, without a redesign. Decisions are deterministic: the
//! same world and events give the same orders.

#[cfg(test)]
mod tests;
