//! The values a save is made of.
use crate::game_world::{
    Age, BuildingKind, GameUnit, GameWorldError, HitRecord, PlayerEconomy, RallyPoint,
    ResearchEntry, TrainingEntry,
};
use crate::game_world::{AnimalKind, Facing, StewardSettings};
use aoe_core::{EntityId, Formation, PlayerId, TileRect, WorldPosition};
use aoe_map::ResourceOverlaySnapshot;
use serde::{Deserialize, Serialize};

pub const WORLD_SNAPSHOT_VERSION: u16 = 4;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorldSnapshot {
    pub version: u16,
    pub tick: u64,
    pub next_entity_id: u64,
    pub fog_disabled: bool,
    pub hp_quarters: Vec<u8>,
    pub players: Vec<PlayerEconomy>,
    /// Absent only for worlds without map terrain (test fixtures).
    pub resources: Option<ResourceOverlaySnapshot>,
    pub buildings: Vec<BuildingRecord>,
    pub units: Vec<UnitRecord>,
    pub formations: Vec<FormationRecord>,
    pub animals: Vec<AnimalRecord>,
    pub explored: Vec<(PlayerId, crate::game_world::visibility::ExploredChunks)>,
}

/// One army group: posture, members, destination and the waypoints behind it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FormationRecord {
    pub form: Formation,
    pub members: Vec<EntityId>,
    pub destination: WorldPosition,
    pub queue: Vec<(WorldPosition, bool)>,
    pub attack_move: bool,
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
    },
    Vassal {
        development: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BuildingRecord {
    pub id: EntityId,
    pub player: PlayerId,
    pub kind: BuildingKind,
    pub footprint: TileRect,
    pub age: Age,
    pub complete: bool,
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UnitRecord {
    pub unit: GameUnit,
    pub next_hit_tick: u64,
    /// The resource node and farm this worker was working.
    pub gather: Option<(u64, Option<EntityId>)>,
    /// The foundation this villager was building.
    pub build: Option<EntityId>,
    /// The building this unit was sheltering in.
    pub garrison_in: Option<EntityId>,
    /// Where it was walking (units outside army groups), and the waypoints queued behind.
    pub move_to: Option<WorldPosition>,
    pub queue: Vec<WorldPosition>,
    pub hunt: Option<EntityId>,
    /// The building this unit was walking to shelter in.
    pub garrison_order: Option<EntityId>,
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
