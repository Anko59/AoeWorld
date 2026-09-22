use super::*;

pub(super) fn visible_tiles_for_height_bounds(
    camera: Camera,
    config: aoe_core::WorldConfig,
    focus: f64,
    bounds: Option<(i16, i16)>,
) -> TileRect {
    let (minimum, maximum) = bounds.map_or((focus, focus), |(min, max)| {
        (focus.min(f64::from(min)), focus.max(f64::from(max)))
    });
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
