//! The values a save is made of.
use crate::game_world::economy::MarketCommodity;
use crate::game_world::{
    Age, BuildingKind, ConstructionState, GameBuilding, GameUnit, GameWorldError, HitRecord,
    PlayerEconomy, RallyPoint, ResearchEntry, TrainingEntry,
};
use crate::game_world::{AnimalKind, Facing, StewardSettings, VassalFocus};
use aoe_core::{EntityId, Formation, PlayerId, TileRect, WorldPosition};
use aoe_map::ResourceOverlaySnapshot;
use serde::{Deserialize, Serialize};

// Resource overlays are validated against the current gold deposit capacity.
pub const WORLD_SNAPSHOT_VERSION: u16 = 30;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorldSnapshot {
    pub version: u16,
    pub tick: u64,
    pub next_entity_id: u64,
    pub fog_disabled: bool,
    pub hp_quarters: Vec<u8>,
    pub players: Vec<PlayerEconomy>,
    /// Remaining server ticks, independent of the save's game-time tick rate.
    pub(crate) market_recovery: Vec<(u64, PlayerId, MarketCommodity)>,
    /// Absent only for worlds without map terrain (test fixtures).
    pub resources: Option<ResourceOverlaySnapshot>,
    pub buildings: Vec<BuildingRecord>,
    pub(crate) dispatch_sites: Vec<(EntityId, Option<EntityId>)>,
    pub(crate) dispatch_cursor: Option<EntityId>,
    pub units: Vec<UnitRecord>,
    pub formations: Vec<FormationRecord>,
    pub animals: Vec<AnimalRecord>,
    pub(crate) town_bells: Vec<EntityId>,
    pub explored: Vec<(PlayerId, crate::game_world::visibility::ExploredChunks)>,
    pub(crate) building_memory: Vec<BuildingMemoryRecord>,
    /// Per player, the last seen amount of each changed resource node.
    pub(crate) resource_memory: Vec<(PlayerId, Vec<(u64, u16)>)>,
}

/// A player's last sight of foreign buildings, including unseen destruction.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct BuildingMemoryRecord {
    pub(crate) viewer: PlayerId,
    pub(crate) buildings: Vec<BuildingRecord>,
    /// In eviction order, oldest first.
    pub(crate) ghosts: Vec<EntityId>,
}

/// One army group: posture, members, destination and the waypoints behind it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FormationRecord {
    pub form: Formation,
    pub members: Vec<EntityId>,
    pub destination: WorldPosition,
    pub queue: Vec<(WorldPosition, bool)>,
    pub attack_move: bool,
    pub pace_limit: Option<(u64, u64)>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnimalRecord {
    pub id: EntityId,
    pub kind: AnimalKind,
    pub owner: Option<PlayerId>,
    pub hp: u16,
    pub position: WorldPosition,
    pub food_remaining: u16,
    pub facing: Facing,
    pub home: WorldPosition,
}

/// What a management unit was set to.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Management {
    Steward(StewardSettings),
    General {
        training: bool,
        mix: [u8; 5],
        solo: bool,
        destination: Option<WorldPosition>,
        /// Presence marks an owner march, and the flag records attack-move.
        owner_march: Option<bool>,
    },
    Vassal {
        development: bool,
        focus: VassalFocus,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BuildingRecord {
    pub id: EntityId,
    pub player: PlayerId,
    pub kind: BuildingKind,
    pub footprint: TileRect,
    pub age: Age,
    pub state: ConstructionState,
    pub bridge_span: Option<aoe_core::BridgeSpan>,
    pub training_queue: Vec<TrainingEntry>,
    /// The General that reserved the front training entry, and its bought kind.
    pub general_recruit: Option<(EntityId, crate::UnitKind)>,
    pub last_hit: Option<HitRecord>,
    pub work_done: u64,
    pub work_total: u64,
    pub hp: u16,
    pub max_hp: u16,
    pub los: u8,
    pub food_remaining: Option<u16>,
    pub research: Option<ResearchEntry>,
    pub gate_open: bool,
    pub gate_locked: bool,
    pub rally_point: Option<RallyPoint>,
    pub farm_reseeds: u8,
}

impl From<&GameBuilding> for BuildingRecord {
    fn from(building: &GameBuilding) -> Self {
        Self {
            id: building.id,
            player: building.player,
            kind: building.kind,
            footprint: building.footprint,
            age: building.age,
            state: building.state,
            bridge_span: building.bridge_span,
            training_queue: building.training_queue.iter().copied().collect(),
            general_recruit: None,
            last_hit: building.last_hit,
            work_done: building.work_done,
            work_total: building.work_total,
            hp: building.hp,
            max_hp: building.max_hp,
            los: building.los,
            food_remaining: building.food_remaining,
            research: building.research,
            gate_open: building.gate_open,
            gate_locked: building.gate_locked,
            rally_point: building.rally_point,
            farm_reseeds: building.farm_reseeds,
        }
    }
}

impl BuildingRecord {
    pub(crate) fn last_sighting(&self) -> GameBuilding {
        GameBuilding {
            id: self.id,
            player: self.player,
            kind: self.kind,
            footprint: self.footprint,
            age: self.age,
            state: self.state,
            bridge_span: self.bridge_span,
            training_queue: self.training_queue.iter().copied().collect(),
            last_hit: self.last_hit,
            work_done: self.work_done,
            work_total: self.work_total,
            hp: self.hp,
            max_hp: self.max_hp,
            los: self.los,
            food_remaining: self.food_remaining,
            research: self.research,
            gate_open: self.gate_open,
            gate_locked: self.gate_locked,
            rally_point: self.rally_point,
            farm_reseeds: self.farm_reseeds,
            garrison: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UnitRecord {
    pub unit: GameUnit,
    pub next_hit_tick: u64,
    pub(crate) transform_until: Option<u64>,
    pub(crate) swing_started: Option<u64>,
    pub(crate) scalar_attack_move: Option<WorldPosition>,
    pub(crate) patrol: Option<[WorldPosition; 2]>,
    /// The resource node and farm this worker was working.
    pub gather: Option<(u64, Option<EntityId>)>,
    /// The foundation this villager was building.
    pub build: Option<EntityId>,
    /// The building this unit was sheltering in.
    pub garrison_in: Option<EntityId>,
    /// The scalar movement destination of a unit outside formations.
    pub move_to: Option<WorldPosition>,
    /// Pending personal waypoints, or the General's pending army waypoints.
    pub queue: Vec<(WorldPosition, bool)>,
    pub hunt: Option<EntityId>,
    /// The building this unit was walking to shelter in.
    pub garrison_order: Option<EntityId>,
    pub(crate) shelter_intent: crate::game_world::economy::ShelterIntent,
    /// The transport this unit was aboard.
    pub embarked_in: Option<EntityId>,
    /// Exact army membership, independent of unit allocation/spawn order.
    pub general_controller: Option<EntityId>,
    /// Last owner order for a soldier excluded from automatic recruitment.
    pub general_order_tick: Option<u64>,
    pub management: Option<Management>,
}

/// What `restore_snapshot` could not put back.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RestoreReport {
    pub buildings: usize,
    pub units: usize,
    pub buildings_lost: usize,
    /// Finished bridges whose deck cannot stand (it crosses another deck): a
    /// seeding fault, dropped as the deckless shells they were, not a failure.
    pub bridges_dropped: usize,
    pub units_lost: usize,
    pub jobs_resumed: usize,
    pub jobs_lost: usize,
    pub animals: usize,
    pub animals_lost: usize,
    pub formations: usize,
    pub managers: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    #[error("the snapshot is from an unsupported version")]
    Version,
    #[error("the world already holds units or buildings")]
    NotEmpty,
    #[error("the snapshot does not fit this map: {0}")]
    Map(String),
    #[error("{units} units and {buildings} buildings could not be placed")]
    Unplaced { units: usize, buildings: usize },
    #[error(transparent)]
    World(#[from] GameWorldError),
}
