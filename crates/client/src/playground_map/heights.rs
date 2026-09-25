use super::*;

const INITIAL_TERRAIN_HEIGHT_MARGIN_LEVELS: f64 = 40.0;

/// Missing chunks form a deterministic camera-centered frontier. Resident
/// chunks expand exact height visibility, while this frontier prevents unseen
/// high relief from being permanently outside the first request rectangle.
/// Progress is bounded by the map, request concurrency, and cache limits; no
/// fixed elevation margin is used as the convergence proof.
pub(super) fn discovery_chunks(client: &Client, budget: usize) -> Vec<(i32, i32)> {
    let center = [
        (client.camera.center[0] / f64::from(CHUNK_TILES)) as i32,
        (client.camera.center[1] / f64::from(CHUNK_TILES)) as i32,
    ];
    let extent = [
        (client.config.width_tiles + CHUNK_TILES - 1) / CHUNK_TILES,
        (client.config.height_tiles + CHUNK_TILES - 1) / CHUNK_TILES,
    ];
    frontier_chunks(center, extent, budget, |coordinate| {
        client.terrain_chunks.contains_key(&coordinate)
            || client.terrain_discovered.contains(&coordinate)
            || client.terrain_inflight.contains(&coordinate)
    })
}

pub(super) fn frontier_chunks(
    center: [i32; 2],
    extent: [i32; 2],
    budget: usize,
    mut excluded: impl FnMut((i32, i32)) -> bool,
) -> Vec<(i32, i32)> {
    if budget == 0 {
        return Vec::new();
    }
    let mut result = Vec::with_capacity(budget);
    for radius in 0..extent[0].max(extent[1]) {
        for coordinate in discovery_ring(center, extent, radius) {
            if excluded(coordinate) {
                continue;
            }
            result.push(coordinate);
            if result.len() == budget {
                return result;
            }
        }
    }
    result
}

pub(super) fn discovery_ring(center: [i32; 2], extent: [i32; 2], radius: i32) -> Vec<(i32, i32)> {
    if radius == 0 {
        return valid_chunk(center, extent).into_iter().collect();
    }
    let mut chunks = Vec::new();
    for y in center[1] - radius..=center[1] + radius {
        for x in center[0] - radius..=center[0] + radius {
            if x.abs_diff(center[0]) == radius as u32 || y.abs_diff(center[1]) == radius as u32 {
                chunks.extend(valid_chunk([x, y], extent));
            }
        }
    }
    chunks.sort_by(|left, right| {
        chunk_distance(*left, center)
            .total_cmp(&chunk_distance(*right, center))
            .then(left.cmp(right))
    });
    chunks
}

fn valid_chunk(coordinate: [i32; 2], extent: [i32; 2]) -> Option<(i32, i32)> {
    (coordinate[0] >= 0
        && coordinate[1] >= 0
        && coordinate[0] < extent[0]
        && coordinate[1] < extent[1])
        .then_some((coordinate[0], coordinate[1]))
}

fn chunk_distance(coordinate: (i32, i32), center: [i32; 2]) -> f64 {
    let x = f64::from(coordinate.0 - center[0]);
    let y = f64::from(coordinate.1 - center[1]);
    x.mul_add(x, y * y)
}

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
    let Some((minimum, maximum)) = chunk_height_bounds(chunk) else {
        return;
    };
    client.terrain_height_bounds = Some(
        client
            .terrain_height_bounds
            .map_or((minimum, maximum), |(old_minimum, old_maximum)| {
                (old_minimum.min(minimum), old_maximum.max(maximum))
            }),
    );
}

pub(super) fn refresh_chunk_height_bounds(client: &mut Client) {
    client.terrain_height_bounds = client
        .terrain_chunks
        .values()
        .filter_map(chunk_height_bounds)
        .fold(None, |bounds, (minimum, maximum)| {
            Some(bounds.map_or(
                (minimum, maximum),
                |(old_minimum, old_maximum): (i16, i16)| {
                    (old_minimum.min(minimum), old_maximum.max(maximum))
                },
            ))
        });
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
