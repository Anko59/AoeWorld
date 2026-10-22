//! The per-block candidate lists behind the replacement search.
use super::super::VILLAGE_RULES;
use crate::{GameWorld, Terrain};
use aoe_core::TileCoord;
use aoe_map::{MapChunkGenerator, ResourceKind, ResourceNode, ResourceOverlay};
use std::collections::BTreeMap;
use std::sync::Arc;

// Starting-history entries that chunk-list builds on this thread touched.
#[cfg(test)]
thread_local! {
    pub(crate) static HISTORY_VISITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn starting_history(
    overlay: &ResourceOverlay,
    min: TileCoord,
    max: TileCoord,
) -> impl Iterator<Item = ResourceNode> + '_ {
    overlay.starting_nodes_in(min, max).inspect(|_| {
        HISTORY_VISITS.with(|visits| visits.set(visits.get() + 1));
    })
}

#[cfg(not(test))]
pub(crate) fn starting_history(
    overlay: &ResourceOverlay,
    min: TileCoord,
    max: TileCoord,
) -> impl Iterator<Item = ResourceNode> + '_ {
    overlay.starting_nodes_in(min, max)
}

/// Side of one cached candidate block, in tiles. Small enough that a search
/// evaluates little terrain beyond its own radius.
pub(crate) const BLOCK_TILES: i32 = 8;
const BLOCK_TILES_U64: u64 = BLOCK_TILES as u64;

impl GameWorld {
    pub fn nearest_resource(
        &self,
        origin: TileCoord,
        kind: ResourceKind,
        radius: i32,
        exclude: Option<u64>,
    ) -> Option<ResourceNode> {
        let Terrain::Map { generator, overlay } = &self.terrain else {
            return None;
        };
        // Public callers cannot enlarge the rule's bounded replacement search.
        let radius = radius.clamp(0, VILLAGE_RULES.replacement_radius_tiles);
        let squared = (radius as u64).pow(2);
        let chunk = (
            origin.x.div_euclid(BLOCK_TILES),
            origin.y.div_euclid(BLOCK_TILES),
        );
        // A candidate tile lies at most `radius` from `origin`, so its chunk
        // is at most this many chunks away in Chebyshev terms.
        let rings = (radius as u32).div_ceil(BLOCK_TILES as u32);
        let mut nearest = None;
        let mut score = (u64::MAX, u64::MAX);
        for ring in 0..=rings {
            // A chunk `ring` chunks away holds tiles at least this far from
            // `origin`; once that already loses, no farther chunk can win.
            // An equal distance could still win on the lower id, so only
            // strictly worse rings are cut.
            let closest = u64::from(ring.saturating_sub(1)) * BLOCK_TILES_U64 + 1;
            if closest * closest > score.0 {
                break;
            }
            for dy in -(ring as i32)..=(ring as i32) {
                for dx in -(ring as i32)..=(ring as i32) {
                    if dx.abs().max(dy.abs()) != ring as i32 {
                        continue;
                    }
                    let block = (chunk.0 + dx, chunk.1 + dy);
                    // Skip a block no tile of which can lie within the radius
                    // or beat the best score (equal distance may still win).
                    let gap = |at: i32, low: i32| {
                        let low = i64::from(low) * i64::from(BLOCK_TILES);
                        let (at, high) = (i64::from(at), low + i64::from(BLOCK_TILES) - 1);
                        (low - at).max(at - high).max(0) as u64
                    };
                    let (gx, gy) = (gap(origin.x, block.0), gap(origin.y, block.1));
                    let nearest_tile = gx * gx + gy * gy;
                    if nearest_tile > squared || nearest_tile > score.0 {
                        continue;
                    }
                    let nodes = self.block_resource_nodes(generator, overlay, block);
                    for node in nodes.iter() {
                        let (dx, dy) = (
                            u64::from(node.tile.x.abs_diff(origin.x)),
                            u64::from(node.tile.y.abs_diff(origin.y)),
                        );
                        let distance = dx.saturating_mul(dx).saturating_add(dy.saturating_mul(dy));
                        if distance > squared
                            || node.kind != kind
                            || Some(node.id) == exclude
                            || !overlay.blocks_node(*node)
                            || (distance, node.id) >= score
                        {
                            continue;
                        }
                        nearest = Some(*node);
                        score = (distance, node.id);
                    }
                }
            }
        }
        nearest
    }

    /// Every candidate resource node of one block of tiles: for each tile the
    /// overlay's first starting node, else the generator's object.
    /// Inserting a starting node requires an object-free tile, so the two
    /// sets never share a tile and the union is exactly what the former
    /// per-tile scan (`overlay entry, else generator object`) resolved.
    /// The list is built once per block and reused by later searches; it is
    /// cleared when the overlay's starting set changes. A failed page read
    /// is no node, tile by tile, as in `object_at`.
    pub(crate) fn block_resource_nodes(
        &self,
        generator: &MapChunkGenerator,
        overlay: &ResourceOverlay,
        chunk: (i32, i32),
    ) -> Arc<Vec<ResourceNode>> {
        if let Ok(cache) = self.resource_chunks.lock()
            && let Some(nodes) = cache.get(&chunk)
        {
            return nodes.clone();
        }
        let (x, y) = chunk;
        let origin = TileCoord::new(x.saturating_mul(BLOCK_TILES), y.saturating_mul(BLOCK_TILES));
        let max = TileCoord::new(
            origin.x.saturating_add(BLOCK_TILES - 1),
            origin.y.saturating_add(BLOCK_TILES - 1),
        );
        // Each tile keeps its first starting entry in identity order, as the
        // former per-tile scan did.
        let mut history = BTreeMap::new();
        for node in starting_history(overlay, origin, max) {
            history.entry(node.tile).or_insert(node);
        }
        let mut nodes = Vec::new();
        for dy in 0..BLOCK_TILES {
            for dx in 0..BLOCK_TILES {
                let tile = TileCoord::new(origin.x.saturating_add(dx), origin.y.saturating_add(dy));
                if let Some(node) = history
                    .get(&tile)
                    .copied()
                    .or_else(|| generator.object_at(tile))
                {
                    nodes.push(node);
                }
            }
        }
        let nodes = Arc::new(nodes);
        // A poisoned cache only costs re-builds, never correctness.
        if let Ok(mut cache) = self.resource_chunks.lock() {
            cache.insert(chunk, nodes.clone());
        }
        nodes
    }
}
