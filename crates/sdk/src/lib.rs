//! Client SDK for AoeWorld AI players.
//!
//! An AI is a separate process that speaks the same gameplay protocol as the
//! browser: it joins one slot of a match with that slot's token, receives only
//! what its fog of war allows, and sends ordinary orders. Nothing here reads
//! simulation state, so a bot cannot cheat.
//!
//! ```no_run
//! # async fn demo() -> Result<(), aoe_sdk::SdkError> {
//! use aoe_sdk::{Client, ConnectOptions, Order, RunLimits, run_bot};
//! let client = Client::connect("http://127.0.0.1:9000", "00ff..", ConnectOptions::default()).await?;
//! let mut bot = |decision: &aoe_sdk::Decision, orders: &mut Vec<Order>| {
//!     for unit in decision.world.idle_units(None) {
//!         orders.push(Order::Stop { unit: unit.id() });
//!     }
//! };
//! let (summary, _) = run_bot(&client, &mut bot, RunLimits::default()).await;
//! println!("{summary:?}");
//! # Ok(()) }
//! ```
mod bot;
mod catalog;
mod client;
mod error;
mod events;
mod fog;
mod http;
mod lobby;
mod model;
mod orders;
mod query;
pub mod rules;
mod world;

#[cfg(test)]
mod tests;

pub use bot::{Bot, RunLimits, RunSummary, run_bot};
pub use catalog::ResourceCatalog;
pub use client::{Client, ConnectOptions, ConnectionStats, Decision, ViewPolicy, parse_token};
pub use error::{Result, SdkError};
pub use events::Event;
pub use fog::FogMemory;
pub use http::Endpoint;
pub use lobby::{MatchInfo, MatchMap, SlotAssignment, SlotKind, start_match};
pub use model::{
    Age, Animal, AnimalKind, AnimalTask, Building, BuildingKind, CarriedResource, CommandResult,
    ConstructionState, Economy, GarrisonUnitState, MarketPrices, Projectile, RallyPoint,
    RallyTarget, ResourceKind, ResourceNode, Stockpile, Tech, TradeResource, Unit, UnitKind,
    UnitTask, Visibility, distance_squared,
};
pub use orders::Order;
pub use world::World;

pub use aoe_core::{EntityId, Formation, PlayerId, TileCoord, TileRect, WorldPosition};
