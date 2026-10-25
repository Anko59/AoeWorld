//! Initial AI decisions must not turn a hundred cold village regions into
//! synchronous terrain composition inside the first simulation ticks.
use aoe_core::EntityId;
use aoe_simulation::{GameWorld, GameWorldError, Terrain};
use std::collections::BTreeSet;

/// The browser first opens at the map centre, then recentres on its human's
/// primary unit. Retain the rendered scenes for both views, not just walk
/// bitmaps that HTTP terrain cannot reuse. Warm a bounded five-chunk square;
/// larger viewports and relief demand use the human priority lane afterward.
pub(super) fn camera(
    world: &GameWorld,
    starts: &[EntityId],
    humans: &[bool],
    content_hash: &str,
    cancelled: &dyn Fn() -> bool,
) -> Result<crate::terrain_cache::TerrainCache, GameWorldError> {
    let mut cache = crate::terrain_cache::TerrainCache::default();
    let Terrain::Map { generator, .. } = world.terrain() else {
        return Ok(cache);
    };
    let side = generator.width_tiles().div_euclid(aoe_map::CHUNK_TILES)
        + i32::from(generator.width_tiles().rem_euclid(aoe_map::CHUNK_TILES) != 0);
    let middle = generator.width_tiles() / 2 / aoe_map::CHUNK_TILES;
    let mut centres = vec![(middle, middle)];
    centres.extend(
        starts
            .iter()
            .zip(humans)
            .filter(|(_, human)| **human)
            .filter_map(|(id, _)| {
                world.unit(*id).map(|unit| {
                    let tile = unit.position.tile_floor();
                    (
                        tile.x.div_euclid(aoe_map::CHUNK_TILES),
                        tile.y.div_euclid(aoe_map::CHUNK_TILES),
                    )
                })
            }),
    );
    let mut chunks = BTreeSet::new();
    for (cx, cy) in centres {
        for dy in -2..=2 {
            for dx in -2..=2 {
                let (x, y) = (cx + dx, cy + dy);
                if x >= 0 && y >= 0 && x < side && y < side {
                    chunks.insert((x, y));
                }
            }
        }
    }
    let started = std::time::Instant::now();
    for (x, y) in chunks.iter().copied() {
        let scene = generator
            .landscape_chunk_with_cancel(x, y, cancelled)
            .map_err(GameWorldError::Environment)?;
        let compact =
            aoe_map::CompactChunk::encode(&scene).map_err(|_| GameWorldError::InvalidTerrain)?;
        cache.insert(
            crate::terrain_cache::Key {
                content_hash: content_hash.to_owned(),
                x,
                y,
            },
            compact,
        );
    }
    tracing::info!(
        camera_chunks = chunks.len(),
        elapsed_ms = started.elapsed().as_millis() as u64,
        "initial camera terrain prepared off tick"
    );
    Ok(cache)
}

pub(super) fn prepare(
    world: &GameWorld,
    starts: &[EntityId],
    cancelled: &dyn Fn() -> bool,
) -> Result<usize, GameWorldError> {
    let Terrain::Map { generator, .. } = world.terrain() else {
        return Ok(0);
    };
    if generator.has_static_navigation() {
        return Ok(0);
    }
    let side = generator.width_tiles().div_euclid(aoe_map::CHUNK_TILES)
        + i32::from(generator.width_tiles().rem_euclid(aoe_map::CHUNK_TILES) != 0);
    let mut chunks = BTreeSet::new();
    for id in starts {
        let Some(unit) = world.unit(*id) else {
            continue;
        };
        let tile = unit.position.tile_floor();
        let (x, y) = (
            tile.x.div_euclid(aoe_map::CHUNK_TILES),
            tile.y.div_euclid(aoe_map::CHUNK_TILES),
        );
        for dy in -1..=1 {
            for dx in -1..=1 {
                let key = (x + dx, y + dy);
                if key.0 >= 0 && key.1 >= 0 && key.0 < side && key.1 < side {
                    chunks.insert(key);
                }
            }
        }
    }
    let started = std::time::Instant::now();
    for &(x, y) in &chunks {
        if cancelled() {
            return Err(GameWorldError::Environment(
                aoe_map::EnvironmentPageError::Cancelled,
            ));
        }
        generator
            .precompose_walk_chunk(x, y, cancelled)
            .map_err(GameWorldError::Environment)?;
        if !generator.walk_chunk_resident(x, y) {
            return Err(GameWorldError::Environment(
                aoe_map::EnvironmentPageError::Unavailable,
            ));
        }
    }
    tracing::info!(
        chunks = chunks.len(),
        elapsed_ms = started.elapsed().as_millis() as u64,
        "initial local terrain prepared off tick"
    );
    Ok(chunks.len())
}

#[path = "warmup/tests.rs"]
#[cfg(test)]
mod tests;
