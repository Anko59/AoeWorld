use super::*;

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

pub(super) fn include_chunk_height_bounds(client: &mut Client, chunk: &Chunk) {
    if let Some(bounds) = merge_chunk_height_bounds(&mut client.terrain_height_bounds, chunk) {
        remember_probe(
            &mut client.terrain_bounds.probes,
            &mut client.terrain_bounds.probe_order,
            (chunk.x, chunk.y),
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
    chunk: &Chunk,
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
    chunks: impl Iterator<Item = &'a Chunk>,
) -> Option<(i16, i16)> {
    chunks
        .filter_map(chunk_height_bounds)
        .fold(None, |mut bounds, next| {
            merge_height_bounds(&mut bounds, next);
            bounds
        })
}

pub(super) fn chunk_height_bounds(chunk: &Chunk) -> Option<(i16, i16)> {
    let mut corners = chunk
        .tiles
        .iter()
        .flat_map(|tile| tile.surface.corner_game_height_levels);
    let first = corners.next()?;
    Some(corners.fold((first, first), |(minimum, maximum), height| {
        (minimum.min(height), maximum.max(height))
    }))
}

#[derive(Default)]
pub(crate) struct MapHeightBounds {
    pub levels: Option<(i16, i16)>,
    pending: bool,
    retry_after: f64,
    chunk_retry_after: f64,
    candidates: Option<(Camera, i32, i32, Vec<(i32, i32)>)>,
    probes: std::collections::BTreeMap<(i32, i32), (i16, i16)>,
    probe_order: std::collections::VecDeque<(i32, i32)>,
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
    spawn_local(async move {
        let result = fetch_bounds(hash).await;
        let mut client = shared.borrow_mut();
        if client.connection_id != connection || client.map_content_hash != Some(hash) {
            return;
        }
        client.terrain_bounds.pending = false;
        match result {
            Ok(bounds) => client.terrain_bounds.levels = Some(bounds),
            Err(error) => {
                client.status = format!("map height bounds unavailable: {error:?}");
                client.terrain_bounds.retry_after = super::super::now() + 5_000.0;
            }
        }
    });
    false
}

async fn fetch_bounds(hash: [u8; 32]) -> Result<(i16, i16), JsValue> {
    let content_hash = hash
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let window = web_sys::window().ok_or("No window")?;
    let response: Response =
        JsFuture::from(window.fetch_with_str(&format!("/maps/{content_hash}/height-bounds")))
            .await?
            .dyn_into()?;
    if !response.ok() {
        return Err(JsValue::from_str(&format!("HTTP {}", response.status())));
    }
    let body = JsFuture::from(response.text()?).await?;
    let bounds: serde_json::Value = serde_json::from_str(
        &body
            .as_string()
            .ok_or("height bounds response was not text")?,
    )
    .map_err(|error| JsValue::from_str(&error.to_string()))?;
    let minimum = bounds["minimum_height_level"]
        .as_i64()
        .and_then(|value| i16::try_from(value).ok());
    let maximum = bounds["maximum_height_level"]
        .as_i64()
        .and_then(|value| i16::try_from(value).ok());
    match (minimum, maximum) {
        (Some(min), Some(max))
            if min <= max && bounds["content_hash"].as_str() == Some(content_hash.as_str()) =>
        {
            Ok((min, max))
        }
        _ => Err(JsValue::from_str("invalid map height bounds")),
    }
}

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

pub(super) fn request_candidates(client: &mut Client, budget: usize) -> Vec<(i32, i32)> {
    if super::super::now() < client.terrain_bounds.chunk_retry_after {
        return Vec::new();
    }
    cached_candidates(client);
    let Some((_, _, _, candidates)) = &client.terrain_bounds.candidates else {
        return Vec::new();
    };
    let pending_focus = (client.focus_map_hash != client.map_content_hash).then_some((
        (client.camera.center[0].floor() as i32).div_euclid(CHUNK_TILES),
        (client.camera.center[1].floor() as i32).div_euclid(CHUNK_TILES),
    ));
    candidates
        .iter()
        .copied()
        .filter(|coordinate| {
            !client.terrain_chunks.contains_key(coordinate)
                && !client.terrain_inflight.contains(coordinate)
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
        .collect()
}

pub(super) fn resident_may_be_visible(client: &Client, coordinate: (i32, i32)) -> bool {
    client
        .terrain_bounds
        .probes
        .get(&coordinate)
        .is_none_or(|bounds| {
            chunk_may_be_visible(client.camera, client.config, coordinate, *bounds)
        })
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
    let x0 = f64::from(coordinate.0 * CHUNK_TILES);
    let y0 = f64::from(coordinate.1 * CHUNK_TILES);
    let x1 = f64::from(((coordinate.0 + 1) * CHUNK_TILES).min(config.width_tiles));
    let y1 = f64::from(((coordinate.1 + 1) * CHUNK_TILES).min(config.height_tiles));
    let points = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
    let mut minimum = [f64::INFINITY; 2];
    let mut maximum = [f64::NEG_INFINITY; 2];
    for height in [bounds.0, bounds.1] {
        for point in points {
            let screen = camera.world_to_screen_at_height(point, f64::from(height));
            minimum[0] = minimum[0].min(screen.x);
            minimum[1] = minimum[1].min(screen.y);
            maximum[0] = maximum[0].max(screen.x);
            maximum[1] = maximum[1].max(screen.y);
        }
    }
    let margin = 8.0 * aoe_core::ISO_TILE_WIDTH * camera.zoom;
    maximum[0] >= -margin
        && maximum[1] >= -margin
        && minimum[0] <= camera.viewport[0] + margin
        && minimum[1] <= camera.viewport[1] + margin
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
