use aoe_core::{EntityId, PlayerId, Tick, TileCoord, TileRect, WorldPosition};
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{SeqAccess, Visitor},
};
use std::{fmt, marker::PhantomData};

pub(crate) mod economy;
mod resources;
mod wildlife;
pub use economy::*;
pub use resources::{MAX_RESOURCE_CHANGES, ResourceAmount, ResourceState};
pub use wildlife::*;

pub const VERSION: u16 = 2;
pub const MAX_MESSAGE: usize = 1_048_576;
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
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ClientMessage {
    Hello {
        version: u16,
        resume_token: Option<ResumeToken>,
    },
    Subscribe {
        revision: u64,
        region: TileRect,
    },
    MoveOrder {
        sequence: u64,
        entity_id: EntityId,
        destination: WorldPosition,
    },
    Resync {
        revision: u64,
    },
    GatherOrder {
        sequence: u64,
        entity_id: EntityId,
        resource_id: u64,
    },
    TrainOrder {
        sequence: u64,
        building_id: EntityId,
        kind: UnitKind,
    },
    CancelTrainingOrder {
        sequence: u64,
        building_id: EntityId,
    },
    /// Attack another player's unit or building: walk into melee range, then
    /// hit until the target dies. Cancelled by a later move or gather order.
    AttackOrder {
        sequence: u64,
        entity_id: EntityId,
        target_id: EntityId,
    },
    /// Place a foundation of `kind` with its top-left tile at `origin` and send
    /// the named villagers to build it. The cost is paid on acceptance. Use
    /// `BuildWallOrder` for palisades.
    BuildOrder {
        sequence: u64,
        #[serde(deserialize_with = "economy::bounded_builders")]
        builder_ids: Vec<EntityId>,
        kind: BuildingKind,
        origin: TileCoord,
    },
    /// Lay palisade segments on the line from `from` to `to`, both included.
    BuildWallOrder {
        sequence: u64,
        #[serde(deserialize_with = "economy::bounded_builders")]
        builder_ids: Vec<EntityId>,
        from: TileCoord,
        to: TileCoord,
    },
    /// Cancel a foundation; the cost is refunded for the work not yet done.
    CancelConstructionOrder {
        sequence: u64,
        building_id: EntityId,
    },
    /// Send a villager to work a completed farm (one farmer per farm).
    FarmOrder {
        sequence: u64,
        entity_id: EntityId,
        farm_id: EntityId,
    },
    /// Send villagers to finish an unfinished foundation the player owns;
    /// nothing is charged.
    ConstructOrder {
        sequence: u64,
        #[serde(deserialize_with = "economy::bounded_builders")]
        builder_ids: Vec<EntityId>,
        building_id: EntityId,
    },
    /// Research `tech` at a completed building; the cost is paid on acceptance.
    ResearchOrder {
        sequence: u64,
        building_id: EntityId,
        tech: Tech,
    },
    /// Cancel the building's research and refund its full cost.
    CancelResearch {
        sequence: u64,
        building_id: EntityId,
    },
    HuntOrder {
        sequence: u64,
        #[serde(deserialize_with = "economy::bounded_builders")]
        unit_ids: Vec<EntityId>,
        animal_id: EntityId,
    },
    /// Halt an owned unit or sheep at its authoritative position.
    StopOrder {
        sequence: u64,
        entity_id: EntityId,
    },
}

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

fn validate_server(message: &ServerMessage) -> Result<(), Error> {
    wildlife::validate_server(message)?;
    match message {
        ServerMessage::ResourceState(state) if !state.valid() => Err(Error::InvalidResourceState),
        ServerMessage::Snapshot {
            units,
            players,
            buildings,
            ..
        } => {
            if units.len() > MAX_SUBSCRIBED_UNITS {
                return Err(Error::TooManyUnits);
            }
            if !economy::valid_units(units)
                || !economy::valid_players(players)
                || !economy::valid_buildings(buildings)
            {
                return Err(Error::InvalidEconomyState);
            }
            Ok(())
        }
        ServerMessage::Tick {
            changed_units,
            removals,
            changed_players,
            changed_buildings,
            building_removals,
            ..
        } => {
            if changed_units.len() > MAX_SUBSCRIBED_UNITS || removals.len() > MAX_SUBSCRIBED_UNITS {
                return Err(Error::TooManyUnits);
            }
            if !economy::valid_units(changed_units)
                || !economy::valid_players(changed_players)
                || !economy::valid_buildings(changed_buildings)
                || building_removals.len() > MAX_ECONOMY_BUILDINGS
            {
                return Err(Error::InvalidEconomyState);
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

pub fn encode_client(message: &ClientMessage) -> Result<Vec<u8>, Error> {
    wildlife::validate_client(message)?;
    let bytes = postcard::to_allocvec(message)?;
    check_size(&bytes)?;
    Ok(bytes)
}

pub fn decode_client(bytes: &[u8]) -> Result<ClientMessage, Error> {
    check_size(bytes)?;
    let message = postcard::from_bytes(bytes)?;
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
