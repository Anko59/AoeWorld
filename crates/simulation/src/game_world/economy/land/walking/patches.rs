//! A patch is a geographic group of like resources. Harvesting changes
//! availability for gathering and routing, but cannot split its ownership.
use super::{BLOCK_TILES, Field, neighbors, page, pages};
use crate::{GameWorld, Terrain};
use aoe_core::{EntityId, TileCoord, TileRect};
use aoe_map::{CHUNK_TILES, MapChunkGenerator, ObjectKind, ResourceKind, ResourceOverlay};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, MutexGuard};

const CELLS: usize = (BLOCK_TILES * BLOCK_TILES) as usize;
const PATCH_CELLS: usize = (CHUNK_TILES * CHUNK_TILES) as usize;
const PATCH_CACHE_CHUNKS: usize = 4096;
type Key = (i32, i32);

#[derive(Debug)]
struct Block {
    kinds: [Option<ResourceKind>; PATCH_CELLS],
    patch: [u32; PATCH_CELLS],
}

#[derive(Debug)]
pub(super) struct PatchCatalog {
    revision: u64,
    blocks: BTreeMap<Key, Block>,
    patches: Vec<Patch>,
    retained_chunks: usize,
}

impl Default for PatchCatalog {
    fn default() -> Self {
        Self {
            revision: 0,
            blocks: BTreeMap::new(),
            patches: Vec::new(),
            retained_chunks: PATCH_CACHE_CHUNKS,
        }
    }
}

#[derive(Clone, Debug)]
struct Patch {
    key: TileCoord,
    tiles: Arc<[TileCoord]>,
}

#[derive(Debug)]
pub(super) struct Claim {
    patch: Patch,
    cost: u16,
}

/// Enumerate a patch only once across all of the fields being rebuilt.
pub(super) struct PatchBlocks<'a> {
    world: &'a GameWorld,
    generator: &'a MapChunkGenerator,
    overlay: &'a ResourceOverlay,
    catalog: MutexGuard<'a, PatchCatalog>,
    failed: bool,
}

impl<'a> PatchBlocks<'a> {
    pub(super) fn new(world: &'a GameWorld) -> Option<Self> {
        let Terrain::Map { generator, overlay } = &world.terrain else {
            return None;
        };
        let cache = &world.economy.walking;
        let mut catalog = cache
            .patches
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if catalog.revision != cache.patch_revision
            || catalog.blocks.len() >= catalog.retained_chunks
        {
            *catalog = PatchCatalog {
                revision: cache.patch_revision,
                ..Default::default()
            };
        }
        Some(Self {
            world,
            generator,
            overlay,
            catalog,
            failed: false,
        })
    }

    fn block(&mut self, key: Key) -> &mut Block {
        self.catalog.blocks.entry(key).or_insert_with(|| {
            let origin = TileCoord::new(key.0 * CHUNK_TILES, key.1 * CHUNK_TILES);
            let last = TileCoord::new(origin.x + CHUNK_TILES - 1, origin.y + CHUNK_TILES - 1);
            let mut kinds = [None; PATCH_CELLS];
            let nodes = self
                .generator
                .chunk_resource_nodes(key)
                .unwrap_or_else(|_| {
                    self.failed = true;
                    Vec::new()
                });
            for node in nodes {
                if !matches!(node.object, ObjectKind::ShoreFish | ObjectKind::DeepFish) {
                    kinds[super::ground::chunk_index(node.tile)] = Some(node.kind);
                }
            }
            let mut starts = [false; PATCH_CELLS];
            for node in self.overlay.starting_nodes_in(origin, last) {
                let slot = super::ground::chunk_index(node.tile);
                if !starts[slot] {
                    kinds[slot] = Some(node.kind);
                    starts[slot] = true;
                }
            }
            Block {
                kinds,
                patch: [u32::MAX; PATCH_CELLS],
            }
        })
    }

    pub(super) fn claims(&mut self, field: &Field) -> Arc<[Claim]> {
        let bounds = field.terrain.bounds;
        // A resource just outside the budget can still be worked from an
        // adjacent walkable tile inside it, even across a block boundary.
        let expanded = TileRect::new(
            TileCoord::new((bounds.min.x - 1).max(0), (bounds.min.y - 1).max(0)),
            TileCoord::new(
                (bounds.max.x + 1).min(self.world.config.width_tiles),
                (bounds.max.y + 1).min(self.world.config.height_tiles),
            ),
        );
        let mut best = BTreeMap::<usize, u16>::new();
        for (x, y) in pages(expanded, CHUNK_TILES) {
            let kinds = self.block((x, y)).kinds;
            for (index, kind) in kinds.into_iter().enumerate() {
                let Some(kind) = kind else { continue };
                let tile = TileCoord::new(
                    x * CHUNK_TILES + index as i32 % CHUNK_TILES,
                    y * CHUNK_TILES + index as i32 / CHUNK_TILES,
                );
                if let Some(cost) = field.approach_cost(tile) {
                    let patch = self.patch_at(tile, kind);
                    best.entry(patch)
                        .and_modify(|prior| *prior = (*prior).min(cost))
                        .or_insert(cost);
                }
            }
        }
        best.into_iter()
            .map(|(index, cost)| Claim {
                patch: self.catalog.patches[index].clone(),
                cost,
            })
            .collect()
    }

    pub(super) fn available(&self) -> bool {
        !self.failed
    }

    fn patch_at(&mut self, tile: TileCoord, kind: ResourceKind) -> usize {
        let next = self.catalog.patches.len() as u32;
        let at = &mut self.block(page(tile, CHUNK_TILES)).patch[super::ground::chunk_index(tile)];
        if *at != u32::MAX {
            return *at as usize;
        }
        *at = next;
        let mut tiles = vec![tile];
        let mut cursor = 0;
        while let Some(&tile) = tiles.get(cursor) {
            cursor += 1;
            for neighbor in neighbors(tile) {
                if neighbor.x < 0
                    || neighbor.y < 0
                    || neighbor.x >= self.world.config.width_tiles
                    || neighbor.y >= self.world.config.height_tiles
                {
                    continue;
                }
                let block = self.block(page(neighbor, CHUNK_TILES));
                let at = super::ground::chunk_index(neighbor);
                if block.patch[at] == u32::MAX && block.kinds[at] == Some(kind) {
                    block.patch[at] = next;
                    tiles.push(neighbor);
                }
            }
        }
        tiles.sort_unstable();
        self.catalog.patches.push(Patch {
            key: tiles[0],
            tiles: tiles.into(),
        });
        next as usize
    }
}

impl Drop for PatchBlocks<'_> {
    fn drop(&mut self) {
        if self.failed {
            *self.catalog = PatchCatalog::default();
        }
    }
}

#[derive(Clone, Debug)]
struct Winner {
    owner: EntityId,
    cost: u16,
    tiles: Arc<[TileCoord]>,
}

#[derive(Debug, Default)]
pub(super) struct PatchOwners {
    winners: BTreeMap<TileCoord, Winner>,
    tiles: BTreeMap<Key, Arc<[Option<EntityId>; CELLS]>>,
}

impl PatchOwners {
    pub(super) fn new<'a>(
        claims: impl Iterator<Item = (EntityId, &'a [Claim])>,
        previous: Option<&Self>,
        same_geography: bool,
        affected: &mut BTreeSet<EntityId>,
    ) -> Self {
        let mut winners = BTreeMap::<TileCoord, Winner>::new();
        for (owner, claims) in claims {
            for claim in claims {
                let winner = winners.entry(claim.patch.key).or_insert_with(|| Winner {
                    owner,
                    cost: claim.cost,
                    tiles: claim.patch.tiles.clone(),
                });
                if (claim.cost, owner) < (winner.cost, winner.owner) {
                    winner.owner = owner;
                    winner.cost = claim.cost;
                }
            }
        }

        let mut tiles = previous.map_or_else(BTreeMap::new, |old| old.tiles.clone());
        let same = |prior: &Winner, next: &Winner| {
            prior.owner == next.owner && (same_geography || prior.tiles == next.tiles)
        };
        if let Some(previous) = previous {
            for (key, prior) in &previous.winners {
                if winners.get(key).is_none_or(|next| !same(prior, next)) {
                    affected.insert(prior.owner);
                    for &tile in prior.tiles.iter() {
                        let key = page(tile, BLOCK_TILES);
                        if let Some(block) = tiles.get_mut(&key) {
                            let block = Arc::make_mut(block);
                            block[slot(tile)] = None;
                            if block.iter().all(Option::is_none) {
                                tiles.remove(&key);
                            }
                        }
                    }
                }
            }
        }
        for (key, next) in &winners {
            if previous
                .and_then(|prior| prior.winners.get(key))
                .is_none_or(|prior| !same(prior, next))
            {
                affected.insert(next.owner);
                for &tile in next.tiles.iter() {
                    let block = tiles
                        .entry(page(tile, BLOCK_TILES))
                        .or_insert_with(|| Arc::new([None; CELLS]));
                    Arc::make_mut(block)[slot(tile)] = Some(next.owner);
                }
            }
        }
        Self { winners, tiles }
    }

    pub(super) fn owner(&self, tile: TileCoord) -> Option<EntityId> {
        self.tiles.get(&page(tile, BLOCK_TILES))?[slot(tile)]
    }

    /// The owners of the block holding `tile`, for scans that walk a box.
    pub(super) fn block(&self, tile: TileCoord) -> Option<&[Option<EntityId>; CELLS]> {
        self.tiles.get(&page(tile, BLOCK_TILES)).map(AsRef::as_ref)
    }
}

pub(super) fn slot(tile: TileCoord) -> usize {
    (tile.y.rem_euclid(BLOCK_TILES) * BLOCK_TILES + tile.x.rem_euclid(BLOCK_TILES)) as usize
}
