//! Decision logic against a fake world, ported from `ai/tests`.
mod blackboard;
mod brain_absorb;
mod brain_damage;
mod brain_refusals;
mod combat;
mod config;
mod context;
mod context_land;
mod context_orders;
mod fake;
mod geometry;
mod management;
mod play;
mod recorder;
mod roster;
mod scouts;

#[path = "diplomacy/cases.rs"]
mod diplomacy;
mod relics;
