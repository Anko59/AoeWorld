//! Far-view construction uses the ordinary placement and one grouped build order.
use super::*;
use aoe_protocol::CommandResult;
use wasm_bindgen::prelude::wasm_bindgen;

/// Far builds keep their own small burst so other UI orders keep room.
const FAR_BUILD_BURST: f64 = 20.0;

/// Track all outgoing human commands, so far builds leave room for other UI orders.
pub(crate) struct Admission {
    tokens: f64,
    last: f64,
    pub pending: Option<u64>,
}
impl Default for Admission {
    fn default() -> Self {
        Self {
            tokens: FAR_BUILD_BURST,
            last: 0.0,
            pending: None,
        }
    }
}
impl Admission {
    pub fn ready(&mut self, time: f64) -> bool {
        self.tokens = (self.tokens + (time - self.last).max(0.0) * 0.01).min(FAR_BUILD_BURST);
        self.last = time;
        self.tokens >= 1.0 && self.pending.is_none()
    }
    pub fn sent(&mut self, time: f64) {
        self.ready(time);
        self.tokens = (self.tokens - 1.0).max(0.0);
    }
}

#[wasm_bindgen(inline_js = "
export function far_build_preview(values, reason) { window.aoeNavigation?.strategic?.building?.preview(values, reason); }
export function far_build_result(message) { window.aoeNavigation?.strategic?.building?.result(message); }
")]
extern "C" {
    fn far_build_preview(values: &[f64], reason: &str);
    fn far_build_result(message: &str);
}

pub(crate) fn acknowledge(client: &mut Client, sequence: u64, result: CommandResult) {
    if client.village.build_admission.pending != Some(sequence) {
        return;
    }
    client.village.build_admission.pending = None;
    far_build_result(if result == CommandResult::Accepted {
        "Accepted 1 building order. Queued for nearby villagers."
    } else {
        map::feedback::order_message(result, None)
    });
}

pub(crate) fn connection_reset(client: &mut Client) {
    let pending = client.village.build_admission.pending.is_some();
    client.village.build_admission = Admission::default();
    if pending {
        far_build_result(
            "Connection reset; the previous building order was not confirmed. Check the site before trying again.",
        );
    }
}

pub(crate) fn preview(client: &Client) {
    let Some(kind) = client.village.placement else {
        far_build_preview(&[], "");
        return;
    };
    let Some(at) = client.pointer.and_then(|p| origin(client, p)) else {
        return;
    };
    let tiles = tiles(client, at).unwrap_or_default();
    let valid = span_valid(client, kind, &tiles);
    let reason = if valid {
        "Click to place a foundation · Esc cancels"
    } else if !enabled(client, kind, 1) {
        "Insufficient resources, age or required buildings"
    } else if tiles
        .iter()
        .any(|t| map::terrain_tile(client, t.x, t.y).is_none())
        && client.map_content_hash.is_some()
    {
        "Loading site terrain · choose an explored, clear site"
    } else {
        "Site blocked, unexplored or outside the map · check the full footprint and banks"
    };
    let scale = f64::from(client.canvas.client_width()) / f64::from(client.canvas.width());
    let mut values = vec![f64::from(u8::from(valid))];
    // One diamond per footprint. Walls/bridges retain their ordinary span rules.
    for tile in &tiles {
        for [x, y] in [
            [tile.x, tile.y],
            [tile.x.saturating_add(kind.side()), tile.y],
            [
                tile.x.saturating_add(kind.side()),
                tile.y.saturating_add(kind.depth()),
            ],
            [tile.x, tile.y.saturating_add(kind.depth())],
        ] {
            let p = crate::playground::game::strategic::screen_position(
                client,
                [f64::from(x), f64::from(y)],
            );
            values.extend([p.x * scale, p.y * scale]);
        }
    }
    far_build_preview(&values, reason);
}

/// Only the site is fetched at far zoom, never the whole strategic viewport.
pub(crate) fn chunks(client: &Client) -> Vec<(i32, i32)> {
    let Some(kind) = client.village.placement else {
        return Vec::new();
    };
    let Some(at) = client.pointer.and_then(|p| origin(client, p)) else {
        return Vec::new();
    };
    let mut chunks = std::collections::BTreeSet::new();
    for tile in tiles(client, at).unwrap_or_default() {
        for x in tile.x..tile.x.saturating_add(kind.side()) {
            for y in tile.y..tile.y.saturating_add(kind.depth()) {
                if x >= 0
                    && y >= 0
                    && x < client.config.width_tiles
                    && y < client.config.height_tiles
                {
                    chunks.insert((
                        x.div_euclid(aoe_map::CHUNK_TILES),
                        y.div_euclid(aoe_map::CHUNK_TILES),
                    ));
                }
            }
        }
    }
    chunks.into_iter().collect()
}

pub(crate) fn submitting() {
    far_build_result("Submitting building order…");
}
pub(crate) fn wait() {
    far_build_result("Waiting for the server or command budget · try again shortly.");
}
