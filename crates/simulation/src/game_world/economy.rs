//! Authoritative village rules and bounded integer economy state.
use aoe_core::{EntityId, PlayerId, TileCoord, TileRect};
use aoe_map::ResourceKind;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

mod buildings;
mod construction;
mod dropoff;
mod farm;
mod gathering;
mod hash;
mod movement;
mod navigation;
pub(crate) mod placement;
mod start_site;
mod starting_kit;
mod tick;
mod training;
mod wall;

pub use buildings::{
    BUILDING_RULES, BuildingRules, MAX_POPULATION, MAX_WALL_SEGMENTS, trainable_units,
};

pub const MAX_ECONOMY_PLAYERS: usize = 16;
pub const MAX_ECONOMY_BUILDINGS: usize = 256;
/// Villagers one build order or one world may assign to construction.
pub const MAX_BUILD_GROUP: usize = 32;
pub const MAX_BUILDERS: usize = 80;
pub const MAX_GATHERERS: usize = 80;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum UnitKind {
    Villager,
    Scout,
    Militia,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
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
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ConstructionState {
    UnderConstruction,
    Complete,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum UnitTask {
    Idle,
    Moving,
    Gathering,
    Carrying,
    Returning,
    /// Walking to or hitting an attack target.
    Attacking,
    /// 0 HP; the body is kept briefly so a client can animate the death.
    Dying,
    Building,
    Farming,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CarriedResource {
    pub kind: ResourceKind,
    pub amount: u16,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Stockpile {
    pub food: u32,
    pub wood: u32,
    pub gold: u32,
    pub stone: u32,
}
impl Stockpile {
    pub(crate) fn deposit(&mut self, load: CarriedResource) {
        let amount = match load.kind {
            ResourceKind::Food => &mut self.food,
            ResourceKind::Wood => &mut self.wood,
            ResourceKind::Gold => &mut self.gold,
            ResourceKind::Stone => &mut self.stone,
        };
        *amount = amount.saturating_add(u32::from(load.amount));
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlayerEconomy {
    pub player: PlayerId,
    pub stockpile: Stockpile,
    pub population_used: u16,
    pub population_cap: u16,
    pub age: super::Age,
    /// Bit `1 << tech as u8` is set once researched.
    pub researched: u32,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TrainingEntry {
    pub kind: UnitKind,
    pub progress_ticks: u64,
    pub total_ticks: u64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GameBuilding {
    pub id: EntityId,
    pub player: PlayerId,
    pub kind: BuildingKind,
    pub footprint: TileRect,
    pub training_queue: VecDeque<TrainingEntry>,
    pub last_hit: Option<super::HitRecord>,
    pub state: ConstructionState,
    /// Work done and work needed; one builder adds 3 per tick, n add n + 2.
    pub work_done: u64,
    pub work_total: u64,
    pub hp: u16,
    pub max_hp: u16,
    pub los: u8,
    /// Food left on a farm; `Some(0)` is an exhausted farm.
    pub food_remaining: Option<u16>,
    pub research: Option<super::ResearchEntry>,
}
impl GameBuilding {
    /// Construction progress in thousandths, 1000 once complete.
    pub fn progress_permille(&self) -> u16 {
        if self.state == ConstructionState::Complete || self.work_total == 0 {
            return 1000;
        }
        (self.work_done.saturating_mul(1000) / self.work_total).min(1000) as u16
    }
    pub fn is_complete(&self) -> bool {
        self.state == ConstructionState::Complete
    }
}

/// The single current rules table. Rates are hundredths of a resource / second;
/// speed is tiles / second. Scout speed remains the world's movement config.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VillageRules {
    pub villager_speed_numerator: u32,
    pub villager_speed_denominator: u32,
    pub villager_capacity: u16,
    /// Food, wood, gold, stone (ResourceKind's canonical order).
    pub gather_hundredths_per_second: [u16; 4],
    pub town_center_side: i32,
    pub population_support: u16,
    pub starting_villagers: u16,
    pub starting_stockpile: Stockpile,
    pub villager_food_cost: u32,
    pub villager_training_seconds: u32,
    pub training_queue_limit: usize,
    pub replacement_radius_tiles: i32,
    pub start_search_radius_tiles: i32,
}
pub const VILLAGE_RULES: VillageRules = VillageRules {
    villager_speed_numerator: 4,
    villager_speed_denominator: 5,
    villager_capacity: 10,
    gather_hundredths_per_second: [31, 39, 38, 36],
    town_center_side: 4,
    population_support: 5,
    starting_villagers: 3,
    starting_stockpile: Stockpile {
        food: 200,
        wood: 200,
        gold: 100,
        stone: 200,
    },
    villager_food_cost: 50,
    villager_training_seconds: 25,
    training_queue_limit: 5,
    replacement_radius_tiles: 10,
    start_search_radius_tiles: 32,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum EconomyError {
    #[error("entity does not exist")]
    UnknownEntity,
    #[error("entity belongs to another player")]
    NotOwner,
    #[error("this unit kind cannot perform that command")]
    WrongUnitKind,
    #[error("resource node is unknown or exhausted")]
    InvalidResource,
    #[error("not enough food to train a villager")]
    InsufficientFood,
    #[error("population cap is reached or reserved by queued training")]
    PopulationCap,
    #[error("town center training queue is full")]
    TrainingQueueFull,
    #[error("attack target is unknown, dead or belongs to the same player")]
    InvalidTarget,
    #[error("not enough resources")]
    InsufficientResources,
    #[error("town center footprint or spawn perimeter is unsuitable")]
    InvalidFootprint,
    #[error("the building is already complete")]
    AlreadyComplete,
    #[error("that building kind cannot be constructed")]
    NotBuildable,
    #[error("a required completed building is missing")]
    RequirementMissing,
    #[error("the building is still under construction")]
    UnderConstruction,
    #[error("that building cannot research that technology")]
    NotResearchable,
    #[error("the technology is already researched")]
    AlreadyResearched,
    #[error("this building or technology is already busy researching")]
    ResearchBusy,
    #[error("the technology requirements are not met")]
    TechRequirementMissing,
    #[error("the farm is exhausted or already worked")]
    FarmUnavailable,
    #[error("bounded village state is full")]
    Capacity,
    #[error("player already has a starting village")]
    AlreadyStarted,
    #[error("village movement failed: {0}")]
    Movement(#[from] super::GameWorldError),
}

#[derive(Debug, Default)]
pub(crate) struct Economy {
    pub(crate) players: BTreeMap<PlayerId, PlayerEconomy>,
    pub(crate) buildings: BTreeMap<EntityId, GameBuilding>,
    pub(crate) blocked_tiles: BTreeSet<TileCoord>,
    pub(crate) gatherers: BTreeMap<EntityId, GatherTask>,
    pub(crate) builders: BTreeMap<EntityId, BuildTask>,
    pub(crate) depletions: Vec<aoe_map::Depletion>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GatherPhase {
    Approaching,
    Gathering,
    Returning,
    WaitingDropoff,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GatherTask {
    node: u64,
    /// Set for a farmer: the farm building worked instead of a map node.
    farm: Option<EntityId>,
    tile: TileCoord,
    kind: ResourceKind,
    phase: GatherPhase,
    credits: u64,
    dropoff: Option<EntityId>,
    return_target: Option<TileCoord>,
    failed_dropoffs: BTreeSet<TileCoord>,
    retry_tick: u64,
}

/// A villager assigned to a foundation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BuildTask {
    pub(crate) building: EntityId,
    pub(crate) attempts: u8,
}

impl super::GameWorld {
    pub fn buildings(&self) -> impl Iterator<Item = &GameBuilding> {
        self.economy.buildings.values()
    }
    pub fn building(&self, id: EntityId) -> Option<&GameBuilding> {
        self.economy.buildings.get(&id)
    }
    pub fn player_economy(&self, player: PlayerId) -> Option<PlayerEconomy> {
        self.economy.players.get(&player).copied()
    }
    pub fn players(&self) -> impl Iterator<Item = PlayerEconomy> + '_ {
        self.economy.players.values().copied()
    }
    pub fn gathering_kind(&self, id: EntityId) -> Option<ResourceKind> {
        self.economy.gatherers.get(&id).map(|task| task.kind)
    }
    pub fn gather_target(&self, id: EntityId) -> Option<u64> {
        self.economy.gatherers.get(&id).map(|task| task.node)
    }
    pub fn take_resource_changes(&mut self) -> Vec<aoe_map::Depletion> {
        std::mem::take(&mut self.economy.depletions)
    }
}
