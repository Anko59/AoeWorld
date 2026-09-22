use aoe_core::{EntityId, PlayerId, Tick, TileRect, WorldPosition};
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{SeqAccess, Visitor},
};
use std::{fmt, marker::PhantomData};

mod resources;
pub use resources::{MAX_RESOURCE_CHANGES, ResourceAmount, ResourceState};

pub const VERSION: u16 = 7;
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
    },
    Tick {
        revision: u64,
        tick: Tick,
        #[serde(deserialize_with = "bounded_vec")]
        changed_units: Vec<UnitState>,
        #[serde(deserialize_with = "bounded_vec")]
        removals: Vec<EntityId>,
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
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("message exceeds {MAX_MESSAGE} bytes")]
    TooLarge,
    #[error("collection exceeds {MAX_SUBSCRIBED_UNITS} units")]
    TooManyUnits,
    #[error("invalid bounded resource state")]
    InvalidResourceState,
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
    match message {
        ServerMessage::ResourceState(state) if !state.valid() => Err(Error::InvalidResourceState),
        ServerMessage::Snapshot { units, .. } if units.len() > MAX_SUBSCRIBED_UNITS => {
            Err(Error::TooManyUnits)
        }
        ServerMessage::Tick {
            changed_units,
            removals,
            ..
        } if changed_units.len() > MAX_SUBSCRIBED_UNITS
            || removals.len() > MAX_SUBSCRIBED_UNITS =>
        {
            Err(Error::TooManyUnits)
        }
        _ => Ok(()),
    }
}

pub fn encode_client(message: &ClientMessage) -> Result<Vec<u8>, Error> {
    let bytes = postcard::to_allocvec(message)?;
    check_size(&bytes)?;
    Ok(bytes)
}

pub fn decode_client(bytes: &[u8]) -> Result<ClientMessage, Error> {
    check_size(bytes)?;
    Ok(postcard::from_bytes(bytes)?)
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
mod tests {
    use super::*;
    use aoe_core::{Seed, TileCoord, WorldConfig};

    fn region() -> TileRect {
        TileRect::new(TileCoord::new(0, 0), TileCoord::new(8, 8))
    }

    #[test]
    fn gameplay_messages_round_trip_independently_of_diagnostic_protocol() {
        let message = ClientMessage::MoveOrder {
            sequence: 9,
            entity_id: EntityId(3),
            destination: WorldPosition::new(42, 84),
        };
        assert_eq!(
            decode_client(&encode_client(&message).unwrap()).unwrap(),
            message
        );
        let welcome = ServerMessage::Welcome {
            version: VERSION,
            world_id: 11,
            map_content_hash: None,
            map_metadata: None,
            width_tiles: WorldConfig::new(16_384, 16_384, Seed(1))
                .unwrap()
                .width_tiles,
            height_tiles: 16_384,
            coordinate_precision: 1_024,
            tick_hz: 20,
            role: Role::Spectator,
            primary_unit_id: EntityId(0),
            resume_token: None,
        };
        assert_eq!(
            decode_server(&encode_server(&welcome).unwrap()).unwrap(),
            welcome
        );
        let reset = ServerMessage::WorldReset { world_id: 12 };
        assert_eq!(
            decode_server(&encode_server(&reset).unwrap()).unwrap(),
            reset
        );
        let acknowledgment = ServerMessage::CommandAck {
            sequence: 7,
            result: CommandResult::RejectedPathBudgetExceeded,
            applied_tick: Tick(3),
        };
        assert_eq!(
            decode_server(&encode_server(&acknowledgment).unwrap()).unwrap(),
            acknowledgment
        );
    }

    #[test]
    fn gameplay_collections_are_bounded() {
        let too_many = vec![
            UnitState {
                id: EntityId(1),
                player: PlayerId(0),
                position: WorldPosition::new(0, 0),
                moving: false,
                planning: false,
                facing: 0
            };
            MAX_SUBSCRIBED_UNITS + 1
        ];
        assert!(matches!(
            encode_server(&ServerMessage::Snapshot {
                revision: 1,
                tick: Tick(0),
                units: too_many
            }),
            Err(Error::TooManyUnits)
        ));
        assert!(matches!(
            decode_client(&vec![0; MAX_MESSAGE + 1]),
            Err(Error::TooLarge)
        ));
        let _ = region();
    }
}
