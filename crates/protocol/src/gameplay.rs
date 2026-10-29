use aoe_core::{EntityId, PlayerId, Tick, TileCoord, TileRect, WorldPosition};
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{SeqAccess, Visitor},
};
use std::{fmt, marker::PhantomData};

mod automation;
mod garrison;
mod validation;
pub use aoe_core::CombatStance;
pub use aoe_core::Formation;
pub use garrison::GarrisonUnitState;
use validation::validate_server;
pub(crate) mod economy;
mod fog;
mod projectiles;
mod queues;
mod rally;
pub use queues::MoveQueue;
mod resources;
pub use rally::{RallyPoint, RallyTarget};
mod wildlife;
pub use economy::*;
pub use fog::FogState;
pub use projectiles::{MAX_PROJECTILES, ProjectileKind, ProjectileState};
pub use resources::{MAX_RESOURCE_CACHE, MAX_RESOURCE_CHANGES, ResourceAmount, ResourceState};
pub use wildlife::*;

pub const VERSION: u16 = 3;
/// One gameplay frame: room for the largest combined tick (16,384 units and
/// 16,384 buildings with full fifteen-unit training queues).
pub const MAX_MESSAGE: usize = 4 * 1_048_576;
pub const MAX_SUBSCRIPTION_TILES: i32 = 512;
pub const MAX_SUBSCRIBED_UNITS: usize = 16_384;
pub const MAX_PENDING_COMMANDS_PER_CONNECTION: usize = 64;
pub const MAX_PENDING_COMMANDS_GLOBAL: usize = 1_024;
pub const MAX_ACK_HISTORY: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResumeToken(pub [u8; 24]);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Role {
    Controller,
    Spectator,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UnitState {
    pub id: EntityId,
    pub player: PlayerId,
    pub position: WorldPosition,
    pub moving: bool,
    pub planning: bool,
    pub facing: u8,
    pub kind: UnitKind,
    pub task: UnitTask,
    pub carried: Option<CarriedResource>,
    /// Assigned resource from the authoritative job, independent of carried load.
    pub gathering: Option<ResourceKind>,
    pub hp: u16,
    pub max_hp: u16,
    /// The unit or building this unit is attacking or walking to attack.
    pub attack_target: Option<EntityId>,
    pub last_hit: Option<HitState>,
    /// Authoritative start of death; retained through corpse decay.
    pub death_tick: Option<Tick>,
    /// Hunting or carcass job, independent of military attack orders.
    pub hunt_target: Option<EntityId>,
    pub death_hunt: Option<AnimalKind>,
    /// Authoritative cavalry exploration toggle.
    pub auto_scout: bool,
    pub cargo_count: u8,
    /// Owner-only home of a living Vassal or General.
    pub home_castle: Option<EntityId>,
    /// Owner-only ID of the Vassal currently managing this villager.
    pub managed_by: Option<EntityId>,
    /// Owner-only posture; foreign records use the default.
    pub stance: CombatStance,
    /// Owner-only General controlling this soldier.
    pub commanded_by: Option<EntityId>,
    /// Owner-only army summary for a living General.
    pub general: Option<GeneralStatus>,
}

impl UnitState {
    pub fn for_viewer(mut self, viewer: PlayerId) -> Self {
        if self.player != viewer {
            self.home_castle = None;
            self.managed_by = None;
            self.commanded_by = None;
            self.general = None;
            self.stance = CombatStance::Aggressive;
        }
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum GeneralState {
    Marching,
    Fighting,
    Retreating,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GeneralStatus {
    pub army_size: u16,
    pub state: GeneralState,
}

mod commands;
pub use commands::ClientMessage;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum CommandResult {
    Accepted,
    RejectedNotController,
    RejectedUnknownEntity,
    RejectedSequence,
    RejectedRateLimited,
    RejectedQueueFull,
    RejectedInvalidDestination,
    RejectedUnreachable,
    RejectedPathBudgetExceeded,
    RejectedNotOwner,
    RejectedWrongUnitKind,
    RejectedInvalidResource,
    RejectedInsufficientFood,
    RejectedPopulationCap,
    RejectedTrainingQueueFull,
    RejectedResourceUnavailable,
    /// The attack target is unknown, already dead, or the sender's own.
    RejectedInvalidTarget,
    RejectedInsufficientResources,
    RejectedNotBuildable,
    RejectedRequirementMissing,
    RejectedInvalidPlacement,
    RejectedUnderConstruction,
    RejectedAlreadyComplete,
    RejectedFarmUnavailable,
    RejectedNotResearchable,
    RejectedAlreadyResearched,
    RejectedResearchBusy,
    RejectedTechRequirement,
    RejectedRequiresFeudalAge,
    /// Trading needs a completed Market owned by the player.
    RejectedNoMarket,
    /// A world-wide capacity (buildings, building ground, workers, hunters,
    /// changed resources) is reached; the order is valid and the queue is not
    /// involved. Appended so earlier discriminants keep their wire value.
    RejectedWorldLimit,
    /// The order's unit group is empty, repeats a unit or exceeds
    /// `MAX_BUILD_GROUP`, or its wall exceeds the longest one order may lay.
    RejectedOrderTooLarge,
    /// At least one building footprint tile has never been explored by its owner.
    RejectedUnexploredPlacement,
    RejectedRequiresCastleAge,
    RejectedGarrisonFull,
    RejectedVassalExists,
    RejectedVassalLeash,
    RejectedVassalHome,
}

/// Immutable physical properties of the active geographic map. The package
/// itself remains available through bounded HTTP chunk requests.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MapMetadata {
    pub tile_size_meters: u8,
    pub compression_numerator: u32,
    pub compression_denominator: u32,
    pub terrain_schema_version: u16,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ServerMessage {
    Welcome {
        version: u16,
        world_id: u64,
        map_content_hash: Option<[u8; 32]>,
        map_metadata: Option<MapMetadata>,
        width_tiles: i32,
        height_tiles: i32,
        coordinate_precision: u16,
        tick_hz: u32,
        role: Role,
        primary_unit_id: EntityId,
        resume_token: Option<ResumeToken>,
    },
    Snapshot {
        revision: u64,
        tick: Tick,
        #[serde(deserialize_with = "bounded_vec")]
        units: Vec<UnitState>,
        #[serde(deserialize_with = "economy::bounded_players")]
        players: Vec<GameplayPlayerState>,
        #[serde(deserialize_with = "economy::bounded_buildings")]
        buildings: Vec<GameplayBuildingState>,
    },
    Tick {
        revision: u64,
        tick: Tick,
        #[serde(deserialize_with = "bounded_vec")]
        changed_units: Vec<UnitState>,
        #[serde(deserialize_with = "bounded_vec")]
        removals: Vec<EntityId>,
        #[serde(deserialize_with = "economy::bounded_players")]
        changed_players: Vec<GameplayPlayerState>,
        #[serde(deserialize_with = "economy::bounded_buildings")]
        changed_buildings: Vec<GameplayBuildingState>,
        #[serde(deserialize_with = "economy::bounded_building_removals")]
        building_removals: Vec<EntityId>,
    },
    CommandAck {
        sequence: u64,
        result: CommandResult,
        applied_tick: Tick,
    },
    RoleChange {
        role: Role,
        primary_unit_id: EntityId,
        resume_token: Option<ResumeToken>,
    },
    WorldReset {
        world_id: u64,
    },
    ResourceState(ResourceState),
    Error {
        code: u16,
        message: String,
    },
    WildlifeState {
        revision: u64,
        tick: Tick,
        #[serde(deserialize_with = "wildlife::bounded_animals")]
        animals: Vec<AnimalState>,
    },
    /// Appended variant: complete player view for one bounded subscription.
    FogState(FogState),
    Projectiles {
        revision: u64,
        tick: Tick,
        #[serde(deserialize_with = "projectiles::bounded_projectiles")]
        projectiles: Vec<ProjectileState>,
    },
    /// Complete queued-movement marker set for the current subscription.
    MoveQueues {
        revision: u64,
        tick: Tick,
        #[serde(deserialize_with = "bounded_vec")]
        queues: Vec<MoveQueue>,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("message exceeds {MAX_MESSAGE} bytes")]
    TooLarge,
    #[error("collection exceeds {MAX_SUBSCRIBED_UNITS} units")]
    TooManyUnits,
    #[error("invalid bounded resource state")]
    InvalidResourceState,
    #[error("invalid bounded economy state")]
    InvalidEconomyState,
    #[error("invalid postcard message: {0}")]
    Invalid(#[from] postcard::Error),
}

fn check_size(bytes: &[u8]) -> Result<(), Error> {
    if bytes.len() > MAX_MESSAGE {
        Err(Error::TooLarge)
    } else {
        Ok(())
    }
}

fn bounded_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Bounded<T>(PhantomData<T>);
    impl<'de, T: Deserialize<'de>> Visitor<'de> for Bounded<T> {
        type Value = Vec<T>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            write!(formatter, "at most {MAX_SUBSCRIBED_UNITS} units")
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
            if sequence
                .size_hint()
                .is_some_and(|size| size > MAX_SUBSCRIBED_UNITS)
            {
                return Err(serde::de::Error::custom("unit collection exceeds limit"));
            }
            let mut entries = Vec::new();
            while entries.len() < MAX_SUBSCRIBED_UNITS {
                match sequence.next_element()? {
                    Some(item) => entries.push(item),
                    None => return Ok(entries),
                }
            }
            if sequence.next_element::<serde::de::IgnoredAny>()?.is_some() {
                return Err(serde::de::Error::custom("unit collection exceeds limit"));
            }
            Ok(entries)
        }
    }
    deserializer.deserialize_seq(Bounded(PhantomData))
}

pub fn encode_client(message: &ClientMessage) -> Result<Vec<u8>, Error> {
    commands::validate(message)?;
    wildlife::validate_client(message)?;
    let bytes = postcard::to_allocvec(message)?;
    check_size(&bytes)?;
    Ok(bytes)
}

pub fn decode_client(bytes: &[u8]) -> Result<ClientMessage, Error> {
    check_size(bytes)?;
    let message = postcard::from_bytes(bytes)?;
    commands::validate(&message)?;
    wildlife::validate_client(&message)?;
    Ok(message)
}

pub fn encode_server(message: &ServerMessage) -> Result<Vec<u8>, Error> {
    validate_server(message)?;
    let bytes = postcard::to_allocvec(message)?;
    check_size(&bytes)?;
    Ok(bytes)
}

pub fn decode_server(bytes: &[u8]) -> Result<ServerMessage, Error> {
    check_size(bytes)?;
    let message = postcard::from_bytes(bytes)?;
    validate_server(&message)?;
    Ok(message)
}

#[cfg(test)]
mod tests;
