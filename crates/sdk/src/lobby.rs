//! Starting a match on a server and learning the slot tokens.
use crate::{
    error::{Result, SdkError},
    http::{Endpoint, check_status},
};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Seating a hundred players searches the whole map for ground.
const MATCH_START_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SlotKind {
    Human,
    Ai,
}

/// The map of a new match: a stored package by hash, or a fresh one made
/// from a `MapRequest` (the server composes terrain without external data).
#[derive(Clone, Debug)]
pub enum MatchMap {
    Package(String),
    Request(aoe_map::MapRequest),
    /// A flat synthetic ground of `tiles_per_side` tiles: only trees, bushes
    /// and ore obstruct movement. The place to develop and test an AI; the
    /// generated terrain of `Request` has cliffs that wall off most villages.
    Flat {
        tiles_per_side: u32,
        seed: u64,
    },
}

impl MatchMap {
    pub fn flat(tiles_per_side: u32, seed: u64) -> Self {
        MatchMap::Flat {
            tiles_per_side,
            seed,
        }
    }

    /// A square synthetic map `side_meters` wide at 1:1 compression
    /// (two metres per tile, so 1000 gives 500 x 500 tiles).
    pub fn square(side_meters: u64, seed: u64) -> Self {
        MatchMap::Request(aoe_map::MapRequest {
            requested_side_meters: side_meters,
            compression: aoe_map::Ratio {
                numerator: 1,
                denominator: 1,
            },
            seed,
            ..aoe_map::MapRequest::default()
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct SlotAssignment {
    pub slot: usize,
    pub player: u16,
    pub kind: SlotKind,
    /// 48 hex digits; give it to [`crate::Client::connect`] (or the browser).
    pub token: String,
    pub primary_unit_id: u32,
    pub start_tile: (i32, i32),
}

#[derive(Clone, Debug, Deserialize)]
pub struct MatchInfo {
    pub world_id: u64,
    pub content_hash: String,
    pub tiles_per_side: u64,
    pub slots: Vec<SlotAssignment>,
}

/// `POST /match`: replaces the running game on `url` with a new match.
/// `admin_token` is the server's `AOE_MATCH_TOKEN`.
pub async fn start_match(
    url: &str,
    admin_token: &str,
    map: &MatchMap,
    slots: &[SlotKind],
) -> Result<MatchInfo> {
    let endpoint = Endpoint::parse(url)?;
    let body = match map {
        MatchMap::Package(hash) => serde_json::json!({"map_hash": hash, "slots": slots}),
        MatchMap::Request(request) => serde_json::json!({"map": request, "slots": slots}),
        MatchMap::Flat {
            tiles_per_side,
            seed,
        } => serde_json::json!({
            "flat": {"tiles_per_side": tiles_per_side, "seed": seed},
            "slots": slots,
        }),
    };
    let (status, response) = endpoint
        .request_within(
            MATCH_START_TIMEOUT,
            "POST",
            "/match",
            &[("X-AoeWorld-Admin-Token", admin_token)],
            Some(body.to_string().as_bytes()),
        )
        .await?;
    check_status(status, &response)?;
    serde_json::from_slice(&response).map_err(SdkError::from)
}
