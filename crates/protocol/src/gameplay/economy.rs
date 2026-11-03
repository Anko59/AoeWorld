//! Current village wire state; independent of simulation storage.
use aoe_core::{EntityId, PlayerId, TileRect};
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{SeqAccess, Visitor},
};
use std::fmt;
#[path = "economy/bounds.rs"]
mod bounds;
pub(super) use bounds::*;
#[path = "economy/rules.rs"]
mod rules;
pub(crate) use rules::{building_rule, player_rule, unit_rule};

pub const MAX_ECONOMY_PLAYERS: usize = 128;
/// Buildings one message may carry. Larger own/remembered or fog-disabled
/// views are synchronized over several deliveries, within the same frame cap.
pub const MAX_ECONOMY_BUILDINGS: usize = 16_384;
/// A complete retained view may span messages, up to 128 seats at 8,192 each.
pub const MAX_RETAINED_BUILDINGS: usize = MAX_ECONOMY_PLAYERS * 8_192;
/// Villagers one build order may name.
pub const MAX_BUILD_GROUP: usize = 32;
pub const MAX_TRAINING_QUEUE: usize = 15;

/// A Market commodity; wood, food and stone trade for gold.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TradeResource {
    Wood,
    Food,
    Stone,
}
impl TradeResource {
    pub const ALL: [Self; 3] = [Self::Wood, Self::Food, Self::Stone];
    /// The stockpile the commodity moves.
    pub const fn kind(self) -> ResourceKind {
        match self {
            Self::Wood => ResourceKind::Wood,
            Self::Food => ResourceKind::Food,
            Self::Stone => ResourceKind::Stone,
        }
    }
}
pub use aoe_core::{MARKET_MAX_PRICE, MARKET_MIN_PRICE};
/// A player's Market commodity prices, in gold per 100 units.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MarketPrices {
    pub wood: u16,
    pub food: u16,
    pub stone: u16,
}
impl Default for MarketPrices {
    /// DE starts every commodity at 100 gold per 100 units.
    fn default() -> Self {
        Self {
            wood: 100,
            food: 100,
            stone: 100,
        }
    }
}

mod kinds;
pub use kinds::{Age, BuildingKind, Tech, UnitKind};

/// A foundation is built by villagers; only a complete building works.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ConstructionState {
    UnderConstruction,
    Complete,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum UnitTask {
    Idle,
    Moving,
    Gathering,
    Carrying,
    Returning,
    /// Walking to, or hitting, `UnitState::attack_target`.
    Attacking,
    /// 0 HP. The body is kept about a second so a client can animate the death.
    Dying,
    /// Adding construction progress to a foundation.
    Building,
    /// Standing on a farm and working it.
    Farming,
    Hunting,
    Shepherding,
    Fishing,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ResourceKind {
    Food,
    Wood,
    Gold,
    Stone,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CarriedResource {
    pub kind: ResourceKind,
    pub amount: u16,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Stockpile {
    pub food: u32,
    pub wood: u32,
    pub gold: u32,
    pub stone: u32,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GameplayPlayerState {
    pub player: PlayerId,
    pub stockpile: Stockpile,
    /// A count, with no gameplay ceiling. Storage is bounded by frame and
    /// retained-entity limits, independently of this scalar.
    pub population_used: u32,
    /// Space from completed owned buildings, without a gameplay ceiling.
    pub population_cap: u32,
    pub age: Age,
    /// Bit `1 << tech as u8` is set once researched.
    pub researched: u64,
    /// The player's Market commodity prices, in gold per 100 units.
    pub market_prices: MarketPrices,
    /// Credited Food/Wood/Gold/Stone deliveries in twelve live five-second
    /// simulation buckets. Excludes stock edits, trade and refunds.
    pub recent_deliveries: [u64; 4],
}
/// The most recent damage taken. A client plays a hit reaction when `tick`
/// is newer than the one it last saw, and a swing for `attacker`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HitState {
    pub attacker: EntityId,
    pub tick: aoe_core::Tick,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TrainingState {
    pub kind: UnitKind,
    pub progress_ticks: u64,
    pub total_ticks: u64,
}
/// The technology a building is researching; it occupies the building's queue.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResearchState {
    pub tech: Tech,
    pub progress_ticks: u64,
    pub total_ticks: u64,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GameplayBuildingState {
    pub id: EntityId,
    pub player: PlayerId,
    pub kind: BuildingKind,
    pub footprint: TileRect,
    pub construction: ConstructionState,
    /// Construction progress in thousandths; 1000 once complete.
    pub progress_permille: u16,
    /// Authoritative hit points; combat reads and writes these.
    pub hp: u16,
    pub max_hp: u16,
    /// Line of sight in tiles.
    pub los: u8,
    /// Food left on a farm (`Some(0)` is exhausted); `None` for other kinds.
    pub food_remaining: Option<u16>,
    #[serde(deserialize_with = "bounded_training")]
    pub training_queue: Vec<TrainingState>,
    pub research: Option<ResearchState>,
    /// A building at 0 HP is destroyed and reported in `building_removals`.
    pub last_hit: Option<HitState>,
    /// Age at the last observation; fog memory must not use the owner's live age.
    pub age: Age,
    pub gate_open: bool,
    pub rally_point: Option<super::RallyPoint>,
    pub farm_reseeds: u8,
    #[serde(deserialize_with = "super::garrison::bounded_garrison")]
    pub garrison: Vec<super::GarrisonUnitState>,
    /// Canonical endpoints of the raster span; absent for ordinary buildings.
    pub bridge_span: Option<aoe_core::BridgeSpan>,
}

impl GameplayBuildingState {
    pub fn contains_tile(&self, tile: aoe_core::TileCoord) -> bool {
        self.bridge_span
            .map_or_else(|| self.footprint.contains(tile), |span| span.contains(tile))
    }
}
