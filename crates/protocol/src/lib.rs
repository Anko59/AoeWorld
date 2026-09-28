//! Bounded, versioned binary WebSocket contract.
mod gameplay;

use aoe_core::{EntityId, PlayerId, Position, Region, Tick};
pub use gameplay::{
    ClientMessage as GameplayClientMessage, CommandResult, Error as GameplayError, MAX_ACK_HISTORY,
    MAX_MESSAGE as GAMEPLAY_MAX_MESSAGE, MAX_PENDING_COMMANDS_GLOBAL,
    MAX_PENDING_COMMANDS_PER_CONNECTION, MAX_RESOURCE_CHANGES, MAX_SUBSCRIBED_UNITS,
    MAX_SUBSCRIPTION_TILES, MapMetadata, ResourceAmount, ResourceState, ResumeToken,
    Role as GameplayRole, ServerMessage as GameplayServerMessage, UnitState as GameplayUnitState,
    VERSION as GAMEPLAY_VERSION, decode_client as decode_gameplay_client,
    decode_server as decode_gameplay_server, encode_client as encode_gameplay_client,
    encode_server as encode_gameplay_server,
};
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{SeqAccess, Visitor},
};
use std::{fmt, marker::PhantomData};

pub const VERSION: u16 = 1;
pub const MAX_MESSAGE: usize = 1_048_576;
pub const MAX_ENTITIES: usize = 16_384;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EntityState {
    pub id: EntityId,
    pub player: PlayerId,
    pub position: Position,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ClientMessage {
    Hello { version: u16 },
    Subscribe { region: Region },
    Resync,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ServerMessage {
    Hello {
        version: u16,
        build: String,
        scenario: String,
        world_size: i32,
    },
    Snapshot {
        tick: Tick,
        region: Region,
        #[serde(deserialize_with = "bounded_vec")]
        entities: Vec<EntityState>,
        loaded_chunks: u32,
        total_entities: u32,
    },
    Delta {
        tick: Tick,
        #[serde(deserialize_with = "bounded_vec")]
        upserts: Vec<EntityState>,
        #[serde(deserialize_with = "bounded_vec")]
        removals: Vec<EntityId>,
    },
    Error {
        code: u16,
        message: String,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("message exceeds {MAX_MESSAGE} bytes")]
    TooLarge,
    #[error("collection exceeds {MAX_ENTITIES} entities")]
    TooManyEntities,
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
            write!(formatter, "at most {MAX_ENTITIES} entities")
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
            if sequence.size_hint().is_some_and(|size| size > MAX_ENTITIES) {
                return Err(serde::de::Error::custom("entity collection exceeds limit"));
            }
            let mut entries = Vec::new();
            while entries.len() < MAX_ENTITIES {
                match sequence.next_element()? {
                    Some(item) => entries.push(item),
                    None => return Ok(entries),
                }
            }
            if sequence.next_element::<serde::de::IgnoredAny>()?.is_some() {
                return Err(serde::de::Error::custom("entity collection exceeds limit"));
            }
            Ok(entries)
        }
    }
    deserializer.deserialize_seq(Bounded(PhantomData))
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
    match message {
        ServerMessage::Snapshot { entities, .. } if entities.len() > MAX_ENTITIES => {
            return Err(Error::TooManyEntities);
        }
        ServerMessage::Delta {
            upserts, removals, ..
        } if upserts.len() > MAX_ENTITIES || removals.len() > MAX_ENTITIES => {
            return Err(Error::TooManyEntities);
        }
        _ => {}
    }
    let bytes = postcard::to_allocvec(message)?;
    check_size(&bytes)?;
    Ok(bytes)
}

pub fn decode_server(bytes: &[u8]) -> Result<ServerMessage, Error> {
    check_size(bytes)?;
    // This early byte limit bounds the largest allocation Postcard can request.
    Ok(postcard::from_bytes(bytes)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_round_trip() {
        let request = ClientMessage::Subscribe {
            region: Region {
                x: 1,
                y: 2,
                width: 3,
                height: 4,
            },
        };
        assert_eq!(
            decode_client(&encode_client(&request).unwrap()).unwrap(),
            request
        );
        let response = ServerMessage::Delta {
            tick: Tick(8),
            upserts: vec![],
            removals: vec![EntityId(3)],
        };
        assert_eq!(
            decode_server(&encode_server(&response).unwrap()).unwrap(),
            response
        );
    }

    #[test]
    fn rejects_oversized_data() {
        assert!(matches!(
            decode_client(&vec![0; MAX_MESSAGE + 1]),
            Err(Error::TooLarge)
        ));
    }

    #[test]
    fn a_short_frame_cannot_request_a_huge_snapshot_allocation() {
        // Postcard enum 1 is Snapshot; the vector length claims u32::MAX.
        let mut bytes = encode_server(&ServerMessage::Snapshot {
            tick: Tick(0),
            region: Region {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            entities: Vec::new(),
            loaded_chunks: 0,
            total_entities: 0,
        })
        .unwrap();
        let position = bytes.len() - 3;
        bytes.splice(position..position + 1, [0xff, 0xff, 0xff, 0xff, 0x0f]);
        assert!(decode_server(&bytes).is_err());
    }

    #[test]
    fn enforces_wire_and_collection_limits_for_both_directions() {
        assert!(matches!(decode_client(&[255]), Err(Error::Invalid(_))));
        assert!(matches!(
            decode_server(&[0; MAX_MESSAGE + 1]),
            Err(Error::TooLarge)
        ));
        let region = Region {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        };
        let entity = EntityState {
            id: EntityId(1),
            player: PlayerId(1),
            position: Position { x: 0, y: 0 },
        };
        let too_many = vec![entity; MAX_ENTITIES + 1];
        assert!(matches!(
            encode_server(&ServerMessage::Snapshot {
                tick: Tick(0),
                region,
                entities: too_many.clone(),
                loaded_chunks: 0,
                total_entities: 0,
            }),
            Err(Error::TooManyEntities)
        ));
        assert!(matches!(
            encode_server(&ServerMessage::Delta {
                tick: Tick(0),
                upserts: too_many,
                removals: Vec::new(),
            }),
            Err(Error::TooManyEntities)
        ));
        assert!(matches!(
            encode_server(&ServerMessage::Delta {
                tick: Tick(0),
                upserts: Vec::new(),
                removals: vec![EntityId(1); MAX_ENTITIES + 1],
            }),
            Err(Error::TooManyEntities)
        ));
    }

    #[test]
    fn bounded_collection_handles_advertised_and_streamed_lengths() {
        use serde::de::value::{Error as ValueError, SeqDeserializer, U8Deserializer};

        let advertised =
            SeqDeserializer::<_, ValueError>::new(std::iter::repeat_n(1u8, MAX_ENTITIES + 1));
        assert!(bounded_vec::<_, u8>(advertised).is_err());

        let mut remaining = MAX_ENTITIES + 1;
        let unknown_length = std::iter::from_fn(move || {
            if remaining == 0 {
                None
            } else {
                remaining -= 1;
                Some(1u8)
            }
        });
        assert!(
            bounded_vec::<_, u8>(SeqDeserializer::<_, ValueError>::new(unknown_length)).is_err()
        );

        let exactly_limit =
            SeqDeserializer::<_, ValueError>::new(std::iter::repeat_n(1u8, MAX_ENTITIES));
        assert_eq!(
            bounded_vec::<_, u8>(exactly_limit)
                .expect("exactly at limit")
                .len(),
            MAX_ENTITIES
        );

        let wrong_type = bounded_vec::<_, u8>(U8Deserializer::<ValueError>::new(1));
        assert!(wrong_type.is_err());
    }
}
