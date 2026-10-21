//! Current village wire state; independent of simulation storage.
use aoe_core::{EntityId, PlayerId, TileRect};
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{SeqAccess, Visitor},
};
use std::fmt;

pub const MAX_ECONOMY_PLAYERS: usize = 128;
pub const MAX_ECONOMY_BUILDINGS: usize = 4_096;
/// Villagers one build order may name.
pub const MAX_BUILD_GROUP: usize = 32;
/// The population ceiling, however many houses stand.
pub const MAX_POPULATION: u16 = 200;
pub const MAX_TRAINING_QUEUE: usize = 5;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum UnitKind {
    Villager,
    Scout,
    Militia,
    FishingShip,
    Archer,
    Skirmisher,
    Spearman,
    ManAtArms,
}
impl UnitKind {
    pub const ALL: [Self; 8] = [
        Self::Villager,
        Self::Scout,
        Self::Militia,
        Self::FishingShip,
        Self::Archer,
        Self::Skirmisher,
        Self::Spearman,
        Self::ManAtArms,
    ];
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum BuildingKind {
    TownCenter,
    House,
    LumberCamp,
    MiningCamp,
    Mill,
    Farm,
    Barracks,
    PalisadeWall,
    Outpost,
    Dock,
    ArcheryRange,
    Stable,
    Blacksmith,
    Market,
    WatchTower,
    PalisadeGate,
}
impl BuildingKind {
    pub const ALL: [Self; 16] = [
        Self::TownCenter,
        Self::House,
        Self::LumberCamp,
        Self::MiningCamp,
        Self::Mill,
        Self::Farm,
        Self::Barracks,
        Self::PalisadeWall,
        Self::Outpost,
        Self::Dock,
        Self::ArcheryRange,
        Self::Stable,
        Self::Blacksmith,
        Self::Market,
        Self::WatchTower,
        Self::PalisadeGate,
    ];
    /// Footprint side in tiles; the single wire-level size table.
    pub const fn side(self) -> i32 {
        match self {
            Self::TownCenter | Self::Market | Self::PalisadeGate => 4,
            Self::House | Self::LumberCamp | Self::MiningCamp | Self::Mill => 2,
            Self::Farm
            | Self::Barracks
            | Self::Dock
            | Self::ArcheryRange
            | Self::Stable
            | Self::Blacksmith => 3,
            Self::PalisadeWall | Self::Outpost | Self::WatchTower => 1,
        }
    }
    pub const fn depth(self) -> i32 {
        if matches!(self, Self::PalisadeGate) {
            1
        } else {
            self.side()
        }
    }
}
/// The player's age; Feudal gates buildings and economy research.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Age {
    Dark,
    Feudal,
}
/// A technology; bit `1 << tech as u8` of `GameplayPlayerState::researched`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Tech {
    Loom,
    TownWatch,
    FeudalAge,
    DoubleBitAxe,
    HorseCollar,
    GoldMining,
    StoneMining,
    Wheelbarrow,
    ManAtArms,
    Forging,
    ScaleMailArmor,
    ScaleBardingArmor,
    Fletching,
    PaddedArcherArmor,
}
impl Tech {
    pub const ALL: [Tech; 14] = [
        Tech::Loom,
        Tech::TownWatch,
        Tech::FeudalAge,
        Tech::DoubleBitAxe,
        Tech::HorseCollar,
        Tech::GoldMining,
        Tech::StoneMining,
        Tech::Wheelbarrow,
        Tech::ManAtArms,
        Tech::Forging,
        Tech::ScaleMailArmor,
        Tech::ScaleBardingArmor,
        Tech::Fletching,
        Tech::PaddedArcherArmor,
    ];
    pub const fn building(self) -> BuildingKind {
        match self {
            Self::Loom | Self::TownWatch | Self::FeudalAge | Self::Wheelbarrow => {
                BuildingKind::TownCenter
            }
            Self::DoubleBitAxe => BuildingKind::LumberCamp,
            Self::HorseCollar => BuildingKind::Mill,
            Self::GoldMining | Self::StoneMining => BuildingKind::MiningCamp,
            Self::ManAtArms => BuildingKind::Barracks,
            Self::Forging
            | Self::ScaleMailArmor
            | Self::ScaleBardingArmor
            | Self::Fletching
            | Self::PaddedArcherArmor => BuildingKind::Blacksmith,
        }
    }
    pub const fn bit(self) -> u32 {
        1 << self as u8
    }
}
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
    pub researched: u32,
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
}

fn valid_prices(prices: MarketPrices) -> bool {
    [prices.wood, prices.food, prices.stone]
        .iter()
        .all(|price| (MARKET_MIN_PRICE..=MARKET_MAX_PRICE).contains(price))
}
fn valid_research(player: &GameplayPlayerState) -> bool {
    let known = Tech::ALL.iter().fold(0, |bits, tech| bits | tech.bit());
    player.researched & !known == 0
        && (player.age == Age::Feudal || player.researched & !Tech::Loom.bit() == 0)
        && (player.age == Age::Feudal) == (player.researched & Tech::FeudalAge.bit() != 0)
        && valid_prices(player.market_prices)
}
pub(super) fn valid_players(players: &[GameplayPlayerState]) -> bool {
    players.len() <= MAX_ECONOMY_PLAYERS
        && players.iter().all(valid_research)
        && players
            .iter()
            .all(|player| player.population_cap <= MAX_POPULATION)
        && players
            .windows(2)
            .all(|pair| pair[0].player < pair[1].player)
}
pub(super) fn valid_buildings(buildings: &[GameplayBuildingState]) -> bool {
    buildings.len() <= MAX_ECONOMY_BUILDINGS
        && buildings.iter().all(|building| {
            building.footprint.min.x >= 0
                && building.footprint.min.y >= 0
                && building.hp > 0
                && (!building.gate_open
                    || (building.kind == BuildingKind::PalisadeGate
                        && building.construction == ConstructionState::Complete))
                && building.footprint.width() == building.kind.side()
                && building.footprint.height() == building.kind.depth()
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
                        (BuildingKind::TownCenter, UnitKind::Villager)
                            | (BuildingKind::Barracks, UnitKind::Militia)
                            | (
                                BuildingKind::Barracks,
                                UnitKind::Spearman | UnitKind::ManAtArms
                            )
                            | (
                                BuildingKind::ArcheryRange,
                                UnitKind::Archer | UnitKind::Skirmisher
                            )
                            | (BuildingKind::Stable, UnitKind::Scout)
                            | (BuildingKind::Dock, UnitKind::FishingShip)
                    ) && entry.total_ticks > 0
                        && entry.progress_ticks <= entry.total_ticks
                })
        })
        && buildings.windows(2).all(|pair| pair[0].id < pair[1].id)
}
pub(super) fn valid_units(units: &[super::UnitState]) -> bool {
    units.iter().all(|unit| {
        unit.facing < 8
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
                        UnitKind::Villager => load.amount <= 13,
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

pub(super) fn bounded<'de, D: Deserializer<'de>, T: Deserialize<'de>, const LIMIT: usize>(
    deserializer: D,
) -> Result<Vec<T>, D::Error> {
    struct Items<T, const N: usize>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>, const N: usize> Visitor<'de> for Items<T, N> {
        type Value = Vec<T>;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(formatter, "at most {N} economy entries")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
            if sequence.size_hint().is_some_and(|size| size > N) {
                return Err(serde::de::Error::custom("economy collection exceeds limit"));
            }
            let mut items = Vec::new();
            while items.len() < N {
                match sequence.next_element()? {
                    Some(item) => items.push(item),
                    None => return Ok(items),
                }
            }
            if sequence.next_element::<serde::de::IgnoredAny>()?.is_some() {
                return Err(serde::de::Error::custom("economy collection exceeds limit"));
            }
            Ok(items)
        }
    }
    deserializer.deserialize_seq(Items::<T, LIMIT>(std::marker::PhantomData))
}
pub(super) fn bounded_players<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Vec<GameplayPlayerState>, D::Error> {
    bounded::<D, _, MAX_ECONOMY_PLAYERS>(d)
}
pub(super) fn bounded_buildings<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Vec<GameplayBuildingState>, D::Error> {
    bounded::<D, _, MAX_ECONOMY_BUILDINGS>(d)
}
pub(super) fn bounded_builders<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<EntityId>, D::Error> {
    bounded::<D, _, MAX_BUILD_GROUP>(d)
}
pub(super) fn bounded_building_removals<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Vec<EntityId>, D::Error> {
    bounded::<D, _, MAX_ECONOMY_BUILDINGS>(d)
}
fn bounded_training<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<TrainingState>, D::Error> {
    bounded::<D, _, MAX_TRAINING_QUEUE>(d)
}
