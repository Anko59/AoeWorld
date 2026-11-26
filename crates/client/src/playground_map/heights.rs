use super::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::Response;

const INITIAL_TERRAIN_HEIGHT_MARGIN_LEVELS: f64 = 40.0;
// Compact height metadata is independent of the 512 decoded-chunk cache.
// At a 16K viewport and minimum zoom, even the complete i16 height sweep
// fits this bound. Very large views retain the nearest bounded candidates.
const MAX_HEIGHT_PROBES: usize = 65_536;

pub(super) fn visible_tiles_for_height_bounds(
    camera: Camera,
    config: aoe_core::WorldConfig,
    focus: f64,
    bounds: Option<(i16, i16)>,
) -> TileRect {
    let (minimum, maximum) = bounds.map_or(
        (
            focus - INITIAL_TERRAIN_HEIGHT_MARGIN_LEVELS,
            focus + INITIAL_TERRAIN_HEIGHT_MARGIN_LEVELS,
        ),
        |(min, max)| (focus.min(f64::from(min)), focus.max(f64::from(max))),
    );
    let lower = camera.visible_tiles_at_height(config, 8.0, minimum);
    let upper = camera.visible_tiles_at_height(config, 8.0, maximum);
    TileRect::new(
        aoe_core::TileCoord::new(lower.min.x.min(upper.min.x), lower.min.y.min(upper.min.y)),
        aoe_core::TileCoord::new(upper.max.x.max(lower.max.x), upper.max.y.max(lower.max.y)),
    )
    .clamp(config.width_tiles, config.height_tiles)
}

pub(super) fn include_chunk_height_bounds(client: &mut Client, chunk: &CachedChunk) {
    if let Some(bounds) = merge_chunk_height_bounds(&mut client.terrain_height_bounds, chunk) {
        remember_probe(
            &mut client.terrain_bounds.probes,
            &mut client.terrain_bounds.probe_order,
            chunk.coordinate(),
            bounds,
            MAX_HEIGHT_PROBES,
        );
        merge_chunk_height_bounds(&mut client.terrain_resident_height_bounds, chunk);
    }
}

pub(super) fn refresh_chunk_height_bounds(client: &mut Client) {
    client.terrain_resident_height_bounds = resident_height_bounds(client.terrain_chunks.values());
}

pub(super) fn merge_chunk_height_bounds(
    target: &mut Option<(i16, i16)>,
    chunk: &CachedChunk,
) -> Option<(i16, i16)> {
    let bounds = chunk_height_bounds(chunk)?;
    merge_height_bounds(target, bounds);
    Some(bounds)
}

pub(super) fn merge_height_bounds(target: &mut Option<(i16, i16)>, (minimum, maximum): (i16, i16)) {
    *target = Some(
        target.map_or((minimum, maximum), |(old_minimum, old_maximum)| {
            (old_minimum.min(minimum), old_maximum.max(maximum))
        }),
    );
}

pub(super) fn resident_height_bounds<'a>(
    chunks: impl Iterator<Item = &'a CachedChunk>,
) -> Option<(i16, i16)> {
    chunks
        .filter_map(chunk_height_bounds)
        .fold(None, |mut bounds, next| {
            merge_height_bounds(&mut bounds, next);
            bounds
        })
}

pub(super) fn chunk_height_bounds(chunk: &CachedChunk) -> Option<(i16, i16)> {
    let mut corners = chunk
        .base_tiles()
        .flat_map(|tile| tile.surface.corner_game_height_levels);
    let first = corners.next()?;
    Some(corners.fold((first, first), |(minimum, maximum), height| {
        (minimum.min(height), maximum.max(height))
    }))
}

#[derive(Default)]
pub(crate) struct MapHeightBounds {
    pub levels: Option<(i16, i16)>,
    pub(super) failures: feedback::TerrainFailures,
    pending: bool,
    retry_after: f64,
    chunk_retry_after: f64,
    candidates: Option<(Camera, i32, i32, Vec<(i32, i32)>)>,
    probes: std::collections::BTreeMap<(i32, i32), (i16, i16)>,
    probe_order: std::collections::VecDeque<(i32, i32)>,
}

impl MapHeightBounds {
    pub(crate) fn memory_entries(&self) -> (usize, usize) {
        (
            self.probes.len(),
            self.candidates.as_ref().map_or(0, |c| c.3.capacity()),
        )
    }
}

pub(super) fn ensure_bounds(
    shared: Rc<RefCell<Client>>,
    client: &mut Client,
    hash: [u8; 32],
) -> bool {
    if client.terrain_bounds.levels.is_some() {
        return true;
    }
    if client.terrain_bounds.pending || super::super::now() < client.terrain_bounds.retry_after {
        return false;
    }
    client.terrain_bounds.pending = true;
    let connection = client.connection_id;
    let shared = Rc::downgrade(&shared);
    spawn_local(async move {
        let result = fetch_bounds(hash).await;
        let Some(shared) = shared.upgrade() else {
            return;
        };
        let mut client = shared.borrow_mut();
        if client.connection_id != connection || client.map_content_hash != Some(hash) {
            return;
        }
        client.terrain_bounds.pending = false;
        match result {
            Ok(bounds) => {
                client.terrain_bounds.levels = Some(bounds);
                client.terrain_bounds.failures.bounds = false;
            }
            Err(error) => {
                web_sys::console::error_1(&JsValue::from_str(&format!(
                    "map height bounds unavailable: {error:?}"
                )));
                client.terrain_bounds.failures.bounds = true;
                feedback::terrain_failed(&mut client);
                client.terrain_bounds.retry_after = super::super::now() + 5_000.0;
            }
        }
    });
    false
}

async fn fetch_bounds(hash: [u8; 32]) -> Result<(i16, i16), JsValue> {
    let content_hash = super::super::storage::hex(&hash);
    let window = web_sys::window().ok_or("No window")?;
    let response: Response =
        JsFuture::from(window.fetch_with_str(&format!("/maps/{content_hash}/height-bounds")))
            .await?
            .dyn_into()?;
    if !response.ok() {
        return Err(JsValue::from_str(&format!("HTTP {}", response.status())));
    }
    let body = JsFuture::from(response.text()?).await?;
    parse_bounds(
        &body
            .as_string()
            .ok_or("height bounds response was not text")?,
        &content_hash,
    )
}

// Browser JSON parsing preserves identity, range, order and duplicate guards.
fn parse_bounds(body: &str, content_hash: &str) -> Result<(i16, i16), JsValue> {
    crate::game_assets::decode_bounds(body, content_hash)
}

#[path = "heights/tests.rs"]
#[cfg(test)]
mod tests;

pub(super) fn cached_candidates(client: &mut Client) -> &[(i32, i32)] {
    let camera = client.camera;
    let config = client.config;
    if client
        .terrain_bounds
        .candidates
        .as_ref()
        .is_none_or(|(old, width, height, _)| {
            *old != camera || *width != config.width_tiles || *height != config.height_tiles
        })
    {
        let candidates = candidate_chunks(
            camera,
            config,
            client.terrain_bounds.levels.unwrap_or((i16::MIN, i16::MAX)),
            MAX_HEIGHT_PROBES,
        );
        client.terrain_bounds.candidates =
            Some((camera, config.width_tiles, config.height_tiles, candidates));
    }
    client
        .terrain_bounds
        .candidates
        .as_ref()
        .map_or(&[], |(_, _, _, values)| values)
}

/// A height interval sweeps the isometric viewport along its depth axis.
/// Intersect each row with the screen-horizontal interval before enumerating
/// chunks: work follows that narrow strip, never the area of the map square.
/// Retain a fixed maximum number of compact coordinates. Actual decoded
/// residency is separately bounded and offscreen probes are not fetched again.
pub(super) fn candidate_chunks(
    camera: Camera,
    config: aoe_core::WorldConfig,
    bounds: (i16, i16),
    capacity: usize,
) -> Vec<(i32, i32)> {
    if capacity == 0 {
        return Vec::new();
    }
    let visible = visible_tiles_for_height_bounds(
        camera,
        config,
        camera.focus_elevation_meters,
        Some(bounds),
    );
    let horizontal = camera.viewport[0] / (aoe_core::ISO_TILE_WIDTH * camera.zoom) + 16.0;
    let difference = camera.center[0] - camera.center[1];
    let center = [
        camera.center[0].round() as i64,
        camera.center[1].round() as i64,
    ];
    let mut nearest = std::collections::BTreeSet::new();
    for y in visible.min.y.div_euclid(CHUNK_TILES)
        ..=visible.max.y.saturating_sub(1).div_euclid(CHUNK_TILES)
    {
        let low = (f64::from(y * CHUNK_TILES) + difference - horizontal).floor() as i32;
        let high = (f64::from((y + 1) * CHUNK_TILES) + difference + horizontal).ceil() as i32;
        let minimum = low.max(visible.min.x).div_euclid(CHUNK_TILES);
        let maximum = high
            .min(visible.max.x.saturating_sub(1))
            .div_euclid(CHUNK_TILES);
        for x in minimum..=maximum {
            let dx = i64::from(x * CHUNK_TILES + CHUNK_TILES / 2) - center[0];
            let dy = i64::from(y * CHUNK_TILES + CHUNK_TILES / 2) - center[1];
            nearest.insert((dx * dx + dy * dy, x, y));
            if nearest.len() > capacity {
                nearest.pop_last();
            }
        }
    }
    nearest.into_iter().map(|(_, x, y)| (x, y)).collect()
}

pub(super) fn chunk_retry_pending(client: &Client) -> bool {
    super::super::now() < client.terrain_bounds.chunk_retry_after
}

pub(super) fn request_candidates(client: &mut Client, budget: usize) -> Vec<(i32, i32)> {
    if chunk_retry_pending(client) {
        return Vec::new();
    }
    if super::super::strategic::active(client) {
        return super::super::village::construction::strategic::chunks(client)
            .into_iter()
            .filter(|c| !client.terrain_chunks.contains_key(c))
            .take(budget)
            .collect();
    }
    cached_candidates(client);
    let Some((_, _, _, candidates)) = &client.terrain_bounds.candidates else {
        return Vec::new();
    };
    let pending_focus = (client.focus_map_hash != client.map_content_hash).then_some((
        (client.camera.center[0].floor() as i32).div_euclid(CHUNK_TILES),
        (client.camera.center[1].floor() as i32).div_euclid(CHUNK_TILES),
    ));
    let mut requested: Vec<_> = candidates
        .iter()
        .copied()
        .filter(|coordinate| {
            !client.terrain_chunks.contains_key(coordinate)
                && (pending_focus == Some(*coordinate)
                    || client
                        .terrain_bounds
                        .probes
                        .get(coordinate)
                        .is_none_or(|bounds| {
                            chunk_may_be_visible(client.camera, client.config, *coordinate, *bounds)
                        }))
        })
        .take(budget)
        .collect();
    let ahead = ahead_camera(client);
    if requested.len() < budget && ahead.center != client.camera.center {
        let candidates = candidate_chunks(
            ahead,
            client.config,
            client.terrain_bounds.levels.unwrap_or((i16::MIN, i16::MAX)),
            MAX_HEIGHT_PROBES,
        );
        for key in candidates {
            if !client.terrain_chunks.contains_key(&key)
                && !requested.contains(&key)
                && may_be_visible_from(client, ahead, key)
            {
                requested.push(key);
                if requested.len() == budget {
                    break;
                }
            }
        }
    }
    requested
}

pub(crate) fn ahead_camera(client: &Client) -> Camera {
    Camera {
        center: client.terrain_ahead,
        ..client.camera
    }
}

pub(in super::super) fn settle_counts(client: &mut Client) -> super::super::capture::ChunkCounts {
    use super::super::capture::ChunkCounts;
    if super::super::strategic::active(client)
        || client.surface_fixture
        || client.map_content_hash.is_none()
    {
        return ChunkCounts::default();
    }
    // Waiting for verified height bounds cannot advertise an already settled map.
    if client.terrain_bounds.levels.is_none() {
        return ChunkCounts {
            pending: 1,
            failed: 0,
        };
    }
    cached_candidates(client);
    let visible = client
        .terrain_bounds
        .candidates
        .as_ref()
        .map_or(&[][..], |(_, _, _, keys)| keys.as_slice());
    // Keep failures bounded to this demand window; cancelled/retired completions
    // never enter it. Include queued demand even during request backoff.
    client.capture.failures.retain(|key| visible.contains(key));
    let focus = (client.focus_map_hash != client.map_content_hash).then_some((
        (client.camera.center[0].floor() as i32).div_euclid(CHUNK_TILES),
        (client.camera.center[1].floor() as i32).div_euclid(CHUNK_TILES),
    ));
    ChunkCounts::for_visible(
        visible,
        |key| {
            client.terrain_chunks.contains_key(key)
                || (Some(*key) != focus && !resident_may_be_visible(client, *key))
        },
        &client.capture.failures,
    )
}

pub(super) fn resident_may_be_visible(client: &Client, coordinate: (i32, i32)) -> bool {
    may_be_visible_from(client, client.camera, coordinate)
}

pub(super) fn may_be_visible_from(client: &Client, camera: Camera, coordinate: (i32, i32)) -> bool {
    client
        .terrain_bounds
        .probes
        .get(&coordinate)
        .is_none_or(|bounds| chunk_may_be_visible(camera, client.config, coordinate, *bounds))
}

pub(super) fn visible_residents(client: &Client) -> Vec<(i32, i32)> {
    client
        .terrain_chunks
        .keys()
        .copied()
        .filter(|coordinate| resident_may_be_visible(client, *coordinate))
        .collect()
}

pub(super) fn chunk_may_be_visible(
    camera: Camera,
    config: aoe_core::WorldConfig,
    coordinate: (i32, i32),
    bounds: (i16, i16),
) -> bool {
    let min = [coordinate.0 * CHUNK_TILES, coordinate.1 * CHUNK_TILES];
    let max = [
        (min[0] + CHUNK_TILES).min(config.width_tiles),
        (min[1] + CHUNK_TILES).min(config.height_tiles),
    ];
    projected_chunk_intersects(camera, min, max, bounds, 8.0)
}

pub(super) fn patch_may_intersect(client: &Client, camera: Camera, key: (i32, i32)) -> bool {
    let probes = &client.terrain_bounds.probes;
    let Some(mut bounds) = probes.get(&key).copied() else {
        return true;
    };
    let min = [key.0 * CHUNK_TILES, key.1 * CHUNK_TILES];
    let max = min.map(|value| value + CHUNK_TILES);
    // Include the maximum half-tile presentation offsets at banks and rims.
    // The whole chunk also covers coarse cells at partial map edges.
    let margin = 0.5 * aoe_core::ISO_TILE_WIDTH * camera.zoom;
    if !chunk_overlaps_horizontal(camera, min, max, margin) {
        return false;
    }
    // At a partial map edge, a coarse vertex beyond the source may use the
    // patch's average height. The resident range bounds that average too.
    if max[0] > client.config.width_tiles || max[1] > client.config.height_tiles {
        bounds = client.terrain_resident_height_bounds.unwrap_or(bounds);
    }
    // A coarse corner can take heights from adjacent chunks, and a closing
    // face can join either side of the boundary.
    for y in key.1 - 1..=key.1 + 1 {
        for x in key.0 - 1..=key.0 + 1 {
            if let Some(&(low, high)) = probes.get(&(x, y)) {
                bounds.0 = bounds.0.min(low);
                bounds.1 = bounds.1.max(high);
            }
        }
    }
    chunk_overlaps_vertical(camera, min, max, bounds, margin)
}

fn projected_chunk_intersects(
    camera: Camera,
    min: [i32; 2],
    max: [i32; 2],
    bounds: (i16, i16),
    margin_tiles: f64,
) -> bool {
    let margin = margin_tiles * aoe_core::ISO_TILE_WIDTH * camera.zoom;
    chunk_overlaps_horizontal(camera, min, max, margin)
        && chunk_overlaps_vertical(camera, min, max, bounds, margin)
}

fn chunk_overlaps_horizontal(camera: Camera, min: [i32; 2], max: [i32; 2], margin: f64) -> bool {
    let [x0, y0] = min.map(f64::from);
    let [x1, y1] = max.map(f64::from);
    camera.world_to_screen([x1, y0]).x >= -margin
        && camera.world_to_screen([x0, y1]).x <= camera.viewport[0] + margin
}

fn chunk_overlaps_vertical(
    camera: Camera,
    min: [i32; 2],
    max: [i32; 2],
    (low, high): (i16, i16),
    margin: f64,
) -> bool {
    camera
        .world_to_screen_at_height(max.map(f64::from), f64::from(low))
        .y
        >= -margin
        && camera
            .world_to_screen_at_height(min.map(f64::from), f64::from(high))
            .y
            <= camera.viewport[1] + margin
}

pub(super) fn defer_failed_chunks(client: &mut Client) {
    client.terrain_bounds.chunk_retry_after = super::super::now() + 5_000.0;
}

pub(super) fn remember_probe(
    probes: &mut std::collections::BTreeMap<(i32, i32), (i16, i16)>,
    order: &mut std::collections::VecDeque<(i32, i32)>,
    coordinate: (i32, i32),
    bounds: (i16, i16),
    capacity: usize,
) {
    if probes.insert(coordinate, bounds).is_none() {
        order.push_back(coordinate);
    }
    while probes.len() > capacity {
        let Some(oldest) = order.pop_front() else {
            break;
        };
        probes.remove(&oldest);
    }
}
