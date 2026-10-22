//! `start_match`: asking a server to begin a match and learning its slot tokens.
use crate::client::{error, runtime};
use pyo3::prelude::*;

#[pyclass(frozen, get_all, module = "aoeworld")]
#[derive(Clone, Debug)]
pub struct SlotAssignment {
    pub slot: usize,
    pub player: u16,
    /// `"human"` or `"ai"`.
    pub kind: String,
    pub token: String,
    pub primary_unit_id: u32,
    pub start_tile: (i32, i32),
}

#[pymethods]
impl SlotAssignment {
    fn __repr__(&self) -> String {
        format!(
            "SlotAssignment(slot={}, player={}, kind={}, start_tile={:?})",
            self.slot, self.player, self.kind, self.start_tile
        )
    }
}

#[pyclass(frozen, get_all, module = "aoeworld")]
#[derive(Clone, Debug)]
pub struct MatchInfo {
    pub world_id: u64,
    pub content_hash: String,
    pub tiles_per_side: u64,
    pub slots: Vec<SlotAssignment>,
}

/// Starts a match on the server at `url`, replacing the running game.
/// `slots` is a list of `"ai"` and `"human"`; `admin_token` is the server's
/// `AOE_MATCH_TOKEN`. `terrain` is `"flat"` (default: a flat synthetic ground
/// of `tiles_per_side` tiles, the place to develop an AI) or `"generated"`
/// (rugged generated terrain, `tiles_per_side` tiles at 1:1); `map_hash`
/// names a stored package instead.
#[pyfunction]
#[pyo3(signature = (url, admin_token, slots, tiles_per_side=500, seed=1, terrain="flat", map_hash=None))]
pub fn start_match(
    py: Python<'_>,
    url: String,
    admin_token: String,
    slots: Vec<String>,
    tiles_per_side: u32,
    seed: u64,
    terrain: &str,
    map_hash: Option<String>,
) -> PyResult<MatchInfo> {
    let kinds = slots
        .iter()
        .map(|slot| match slot.as_str() {
            "ai" => Ok(aoe_sdk::SlotKind::Ai),
            "human" => Ok(aoe_sdk::SlotKind::Human),
            other => Err(pyo3::exceptions::PyValueError::new_err(format!(
                "slot kind must be \"ai\" or \"human\", not {other:?}"
            ))),
        })
        .collect::<PyResult<Vec<_>>>()?;
    let map = match (map_hash, terrain) {
        (Some(hash), _) => aoe_sdk::MatchMap::Package(hash),
        (None, "flat") => aoe_sdk::MatchMap::flat(tiles_per_side, seed),
        (None, "generated") => aoe_sdk::MatchMap::square(u64::from(tiles_per_side) * 2, seed),
        (None, other) => {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "terrain must be \"flat\" or \"generated\", not {other:?}"
            )));
        }
    };
    let rt = runtime()?;
    let info = py
        .allow_threads(move || rt.block_on(aoe_sdk::start_match(&url, &admin_token, &map, &kinds)))
        .map_err(error)?;
    Ok(MatchInfo {
        world_id: info.world_id,
        content_hash: info.content_hash,
        tiles_per_side: info.tiles_per_side,
        slots: info
            .slots
            .into_iter()
            .map(|slot| SlotAssignment {
                slot: slot.slot,
                player: slot.player,
                kind: match slot.kind {
                    aoe_sdk::SlotKind::Ai => "ai",
                    aoe_sdk::SlotKind::Human => "human",
                }
                .to_owned(),
                token: slot.token,
                primary_unit_id: slot.primary_unit_id,
                start_tile: slot.start_tile,
            })
            .collect(),
    })
}
