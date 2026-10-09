//! Independently frozen pre-loop wire declarations: values, bytes and exact errors.
use super::*;

// Serde's sequence diagnostics embed Rust identifiers, not just wire renames.
// Keep the original identifiers in this independent module and alias them below.
mod frozen {
    use super::*;

    #[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
    pub(super) struct ResumeToken(pub(super) [u8; 24]);

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub(super) enum ServerMessage {
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

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub(super) enum ClientMessage {
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
}

use frozen::{
    ClientMessage as FrozenClient, ResumeToken as FrozenToken, ServerMessage as FrozenServer,
};

fn frozen_welcome() -> FrozenServer {
    FrozenServer::Welcome {
        version: VERSION,
        world_id: 11,
        map_content_hash: Some(std::array::from_fn(|i| (i * 7) as u8)),
        map_metadata: Some(MapMetadata {
            tile_size_meters: 8,
            compression_numerator: 30,
            compression_denominator: 1,
            terrain_schema_version: 10,
        }),
        width_tiles: 1_200,
        height_tiles: 800,
        coordinate_precision: 1_024,
        tick_hz: 20,
        role: Role::Spectator,
        primary_unit_id: EntityId(3),
        resume_token: Some(FrozenToken(std::array::from_fn(|i| (255 - i * 3) as u8))),
    }
}

fn json_equal<A, B>(source: &str)
where
    A: serde::de::DeserializeOwned + Serialize,
    B: serde::de::DeserializeOwned + Serialize,
{
    match (
        serde_json::from_str::<A>(source),
        serde_json::from_str::<B>(source),
    ) {
        (Ok(actual), Ok(frozen)) => {
            assert_eq!(
                serde_json::to_string(&actual).unwrap(),
                serde_json::to_string(&frozen).unwrap(),
                "{source}"
            );
            assert_eq!(
                postcard::to_allocvec(&actual).unwrap(),
                postcard::to_allocvec(&frozen).unwrap(),
                "{source}"
            );
        }
        (Err(actual), Err(frozen)) => {
            assert_eq!(actual.to_string(), frozen.to_string(), "{source}");
            assert_eq!(actual.classify(), frozen.classify(), "{source}");
            assert_eq!(
                (actual.line(), actual.column()),
                (frozen.line(), frozen.column()),
                "{source}"
            );
        }
        _ => panic!("changed acceptance for {source}"),
    }
}

fn welcome_field(field: &str, replacement: Option<&str>) -> String {
    let mut value = serde_json::to_value(frozen_welcome()).unwrap();
    let fields = value["Welcome"].as_object_mut().unwrap();
    fields.remove(field);
    let mut source = serde_json::to_string(&value).unwrap();
    if let Some(replacement) = replacement {
        let suffix = source.len() - 2;
        source.insert_str(suffix, &format!(",\"{field}\":{replacement}"));
    }
    source
}

fn byte_list(length: usize) -> String {
    format!(
        "[{}]",
        (0..length)
            .map(|i| (i % 256).to_string())
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn check_hash(source: &str) {
    json_equal::<ServerMessage, FrozenServer>(source);
}
fn check_token(source: &str) {
    json_equal::<ResumeToken, FrozenToken>(source);
    check_hash(&welcome_field("resume_token", Some(source)));
    json_equal::<ClientMessage, FrozenClient>(&format!(
        "{{\"Hello\":{{\"version\":8,\"resume_token\":{source}}}}}"
    ));
}

#[test]
fn byte_array_json_lengths_missing_null_values_and_serialization_match_frozen_wire() {
    check_hash(&serde_json::to_string(&frozen_welcome()).unwrap());
    for length in 0..=40 {
        let bytes = byte_list(length);
        check_token(&bytes);
        check_hash(&welcome_field("map_content_hash", Some(&bytes)));
    }
    for field in ["map_content_hash", "resume_token"] {
        check_hash(&welcome_field(field, None));
        check_hash(&welcome_field(field, Some("null")));
    }
    check_token("null");
    // Serde accepts both map and sequence forms of struct variants. A field
    // default added for the hash would change missing-sequence error indices.
    for length in 0..=11 {
        let fields = [
            "8",
            "11",
            "null",
            "null",
            "1200",
            "800",
            "1024",
            "20",
            "\"Spectator\"",
            "3",
            "null",
        ];
        check_hash(&format!("{{\"Welcome\":[{}]}}", fields[..length].join(",")));
    }
    json_equal::<ClientMessage, FrozenClient>("{\"Hello\":{\"version\":8}}");
    // Full postcard ordering is verified against the independently frozen enum.
    let source = serde_json::to_string(&frozen_welcome()).unwrap();
    let actual: ServerMessage = serde_json::from_str(&source).unwrap();
    assert_eq!(
        encode_server(&actual).unwrap(),
        postcard::to_allocvec(&frozen_welcome()).unwrap()
    );
}

#[test]
fn byte_array_json_exact_index_type_float_overflow_and_location_errors_match_frozen_wire() {
    for malformed in [
        "true", "false", "{}", "\"text\"", "-1", "256", "1.0", "1e0", "1e309",
    ] {
        check_token(malformed);
        check_hash(&welcome_field("map_content_hash", Some(malformed)));
    }
    for length in [24, 32] {
        for index in [0, length / 2, length - 1] {
            for malformed in [
                "null", "true", "{}", "[]", "\"0\"", "-1", "256", "1.0", "1e0", "1e309", "NaN",
            ] {
                let mut entries = vec!["0"; length];
                entries[index] = malformed;
                let bytes = format!("[\n {}\n]", entries.join(",\n "));
                if length == 24 {
                    check_token(&bytes);
                } else {
                    check_hash(&welcome_field("map_content_hash", Some(&bytes)));
                }
            }
        }
    }
    for length in [24, 32] {
        let bytes = byte_list(length);
        for extra in [" true", " null", ",", "{}", "\n[0]"] {
            if length == 24 {
                check_token(&format!("{bytes}{extra}"));
            } else {
                let source = welcome_field("map_content_hash", Some(&bytes));
                check_hash(&format!("{source}{extra}"));
            }
        }
    }
}

fn postcard_equal<A, B>(bytes: &[u8])
where
    A: serde::de::DeserializeOwned + Serialize,
    B: serde::de::DeserializeOwned + Serialize,
{
    match (
        postcard::from_bytes::<A>(bytes),
        postcard::from_bytes::<B>(bytes),
    ) {
        (Ok(actual), Ok(frozen)) => {
            assert_eq!(
                postcard::to_allocvec(&actual).unwrap(),
                postcard::to_allocvec(&frozen).unwrap()
            );
            assert_eq!(
                serde_json::to_string(&actual).unwrap(),
                serde_json::to_string(&frozen).unwrap()
            );
        }
        (Err(actual), Err(frozen)) => {
            assert_eq!(actual, frozen, "{bytes:?}");
            assert_eq!(actual.to_string(), frozen.to_string(), "{bytes:?}");
        }
        _ => panic!("changed binary acceptance for {bytes:?}"),
    }
}

#[test]
fn byte_array_postcard_full_enum_token_truncation_mutation_and_trailing_match_frozen_wire() {
    let token = postcard::to_allocvec(&FrozenToken(std::array::from_fn(|i| i as u8))).unwrap();
    let welcome = postcard::to_allocvec(&frozen_welcome()).unwrap();
    let hello = postcard::to_allocvec(&FrozenClient::Hello {
        version: VERSION,
        resume_token: Some(FrozenToken([255; 24])),
    })
    .unwrap();
    for end in 0..=token.len() {
        postcard_equal::<ResumeToken, FrozenToken>(&token[..end]);
    }
    for end in 0..=welcome.len() {
        postcard_equal::<ServerMessage, FrozenServer>(&welcome[..end]);
    }
    for end in 0..=hello.len() {
        postcard_equal::<ClientMessage, FrozenClient>(&hello[..end]);
    }
    for index in 0..welcome.len() {
        for byte in [0, 127, 128, 255] {
            let mut altered = welcome.clone();
            altered[index] = byte;
            postcard_equal::<ServerMessage, FrozenServer>(&altered);
        }
    }
    for extra in [vec![0], vec![255], vec![128, 0], vec![1, 2, 3]] {
        let mut bytes = welcome.clone();
        bytes.extend(&extra);
        postcard_equal::<ServerMessage, FrozenServer>(&bytes);
        let mut bytes = token.clone();
        bytes.extend(&extra);
        postcard_equal::<ResumeToken, FrozenToken>(&bytes);
    }
}
