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

pub const MAX_ECONOMY_PLAYERS: usize = 128;
/// Buildings one message may carry. Larger own/remembered or fog-disabled
/// views are synchronized over several deliveries, within the same frame cap.
pub const MAX_ECONOMY_BUILDINGS: usize = 16_384;
/// A complete retained view may span messages, up to 128 seats at 8,192 each.
pub const MAX_RETAINED_BUILDINGS: usize = MAX_ECONOMY_PLAYERS * 8_192;
/// Villagers one build order may name.
pub const MAX_BUILD_GROUP: usize = 32;
/// The population ceiling, however many houses stand.
pub const MAX_POPULATION: u16 = 1_000;
/// The most a match rule may raise the population ceiling to; a wire state
/// above it is invalid.
pub const MAX_MATCH_POPULATION: u16 = 5_000;
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
    pub population_used: u16,
    pub population_cap: u16,
    pub age: Age,
    /// Bit `1 << tech as u8` is set once researched.
    pub researched: u64,
    /// The player's Market commodity prices, in gold per 100 units.
    pub market_prices: MarketPrices,
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

fn valid_prices(prices: MarketPrices) -> bool {
    [prices.wood, prices.food, prices.stone]
        .iter()
        .all(|price| (MARKET_MIN_PRICE..=MARKET_MAX_PRICE).contains(price))
}
fn valid_research(player: &GameplayPlayerState) -> bool {
    let known = Tech::ALL.iter().fold(0, |bits, tech| bits | tech.bit());
    player.researched & !known == 0
        && (player.age >= Age::Feudal || player.researched & !Tech::Loom.bit() == 0)
        && (player.age >= Age::Feudal) == (player.researched & Tech::FeudalAge.bit() != 0)
        && (player.age == Age::Castle) == (player.researched & Tech::CastleAge.bit() != 0)
        && (player.age == Age::Castle
            || player.researched & Tech::ALL[15..].iter().fold(0, |bits, t| bits | t.bit()) == 0)
        && valid_prices(player.market_prices)
}
pub(super) fn valid_players(players: &[GameplayPlayerState]) -> bool {
    players.len() <= MAX_ECONOMY_PLAYERS
        && players.iter().all(valid_research)
        && players
            .iter()
            .all(|player| player.population_cap <= MAX_MATCH_POPULATION)
        && players
            .windows(2)
            .all(|pair| pair[0].player < pair[1].player)
}
pub(super) fn valid_buildings(buildings: &[GameplayBuildingState]) -> bool {
    buildings.len() <= MAX_ECONOMY_BUILDINGS
        && buildings.iter().all(|building| {
            if !super::automation::valid_reseeds(building) || !super::garrison::valid(building) {
                return false;
            }
            building.rally_point.is_none_or(|r| {
                r.position.x >= 0
                    && r.position.y >= 0
                    && !matches!(
                        building.kind,
                        BuildingKind::House
                            | BuildingKind::Farm
                            | BuildingKind::Mill
                            | BuildingKind::LumberCamp
                            | BuildingKind::MiningCamp
                            | BuildingKind::Bridge
                            | BuildingKind::PalisadeWall
                            | BuildingKind::PalisadeGate
                            | BuildingKind::StoneWall
                            | BuildingKind::StoneGate
                            | BuildingKind::Outpost
                            | BuildingKind::Blacksmith
                            | BuildingKind::Market
                            | BuildingKind::WatchTower
                            | BuildingKind::University
                    )
                    && match r.target {
                        super::RallyTarget::Ground(p) => p == r.position,
                        _ => true,
                    }
            }) && building.footprint.min.x >= 0
                && building.footprint.min.y >= 0
                && building.hp > 0
                && (!building.gate_open
                    || (matches!(
                        building.kind,
                        BuildingKind::PalisadeGate | BuildingKind::StoneGate
                    ) && building.construction == ConstructionState::Complete))
                && if building.kind == BuildingKind::Bridge {
                    building
                        .bridge_span
                        .is_some_and(|span| span.valid() && span.footprint() == building.footprint)
                } else {
                    building.bridge_span.is_none()
                        && building.footprint.width() == building.kind.side()
                        && building.footprint.height() == building.kind.depth()
                }
                && building.progress_permille <= 1000
                && (building.construction == ConstructionState::Complete)
                    == (building.progress_permille == 1000)
                && building.max_hp > 0
                && building.hp <= building.max_hp
                && (building.kind == BuildingKind::Farm) == building.food_remaining.is_some()
                && (building.construction == ConstructionState::Complete
                    || building.training_queue.is_empty())
                && building.research.is_none_or(|research| {
                    building.kind == research.tech.building()
                        && building.construction == ConstructionState::Complete
                        && building.training_queue.is_empty()
                        && research.total_ticks > 0
                        && research.progress_ticks <= research.total_ticks
                })
                && building.training_queue.len() <= MAX_TRAINING_QUEUE
                && building.training_queue.iter().all(|entry| {
                    matches!(
                        (building.kind, entry.kind),
                        (BuildingKind::Castle, UnitKind::Vassal | UnitKind::General)
                            | (BuildingKind::TownCenter, UnitKind::Villager)
                            | (BuildingKind::Barracks, UnitKind::Militia)
                            | (
                                BuildingKind::Barracks,
                                UnitKind::Spearman
                                    | UnitKind::ManAtArms
                                    | UnitKind::LongSwordsman
                                    | UnitKind::Pikeman
                            )
                            | (
                                BuildingKind::ArcheryRange,
                                UnitKind::Archer
                                    | UnitKind::Skirmisher
                                    | UnitKind::Crossbowman
                                    | UnitKind::EliteSkirmisher
                            )
                            | (
                                BuildingKind::Stable,
                                UnitKind::Scout | UnitKind::Knight | UnitKind::LightCavalry
                            )
                            | (BuildingKind::Monastery, UnitKind::Monk)
                            | (
                                BuildingKind::SiegeWorkshop,
                                UnitKind::Mangonel | UnitKind::BatteringRam
                            )
                            | (
                                BuildingKind::Dock,
                                UnitKind::FishingShip
                                    | UnitKind::Galley
                                    | UnitKind::FireShip
                                    | UnitKind::DemolitionShip
                                    | UnitKind::TransportShip
                                    | UnitKind::WarGalley
                            )
                    ) && entry.total_ticks > 0
                        && entry.progress_ticks <= entry.total_ticks
                })
        })
        && buildings.windows(2).all(|pair| pair[0].id < pair[1].id)
}
pub(super) fn valid_units(units: &[super::UnitState]) -> bool {
    units.iter().all(|unit| {
        unit.home_castle.is_none_or(|id| {
            matches!(unit.kind, UnitKind::Vassal | UnitKind::General)
                && unit.hp > 0
                && id != unit.id
        }) && unit
            .managed_by
            .is_none_or(|id| unit.kind == UnitKind::Villager && unit.hp > 0 && id != unit.id)
            && unit.commanded_by.is_none_or(|id| {
                id != unit.id
                    && unit.hp > 0
                    && !matches!(
                        unit.kind,
                        UnitKind::Villager | UnitKind::Vassal | UnitKind::General | UnitKind::Monk
                    )
                    && !unit.kind.is_naval()
            })
            && unit
                .general
                .is_none_or(|g| unit.kind == UnitKind::General && unit.hp > 0 && g.army_size <= 100)
            && unit.cargo_count <= 5
            && (unit.cargo_count == 0 || (unit.kind == UnitKind::TransportShip && unit.hp > 0))
            && unit.facing < 8
            && super::automation::valid_auto_scout(unit)
            && unit.max_hp > 0
            && unit.hp <= unit.max_hp
            && unit.death_tick.is_none_or(|_| unit.hp == 0)
            && unit
                .death_hunt
                .is_none_or(|_| unit.hp == 0 && unit.kind == UnitKind::Villager)
            && unit
                .hunt_target
                .is_none_or(|id| unit.kind == UnitKind::Villager && id != unit.id)
            && (unit.hp == 0) == (unit.task == UnitTask::Dying)
            && (unit.hp > 0 || unit.attack_target.is_none())
            && (!matches!(unit.task, UnitTask::Hunting | UnitTask::Shepherding)
                || (unit.kind == UnitKind::Villager
                    && unit
                        .carried
                        .is_none_or(|load| load.kind == ResourceKind::Food)
                    && unit.gathering.is_none_or(|kind| kind == ResourceKind::Food)))
            && unit.gathering.is_none_or(|kind| {
                unit.kind == UnitKind::Villager
                    || (unit.kind == UnitKind::FishingShip && kind == ResourceKind::Food)
            })
            && unit.carried.is_none_or(|load| {
                load.amount > 0
                    && match unit.kind {
                        // Hunters start at 35; DAT carry upgrades ceil to 67.
                        // A stopped hunter keeps that food even without a job.
                        UnitKind::Villager => {
                            let limit = if load.kind == ResourceKind::Food {
                                67
                            } else {
                                21
                            };
                            load.amount <= limit
                        }
                        UnitKind::FishingShip => {
                            load.kind == ResourceKind::Food && load.amount <= 15
                        }
                        _ => false,
                    }
            })
            && (unit.task != UnitTask::Fishing
                || matches!(unit.kind, UnitKind::Villager | UnitKind::FishingShip)
                    && unit.gathering == Some(ResourceKind::Food))
    })
}
