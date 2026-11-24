//! Land reachability for orders, read from `aoe_map::Regions` over the same
//! ground the route planner searches. A completed gate is a gated portal: it
//! admits its owner or anyone while open, unless locked. Toggling changes only admitted
//! edges, never chunk labels. The layer is a pure cache; it is never hashed.
use crate::{GameWorld, GameWorldError, Terrain};
use aoe_core::{PlayerId, TileCoord, TileRect};
use aoe_map::{LandBucket, MapRegionGround, RegionChunk, RegionGround, RegionTile, Regions};

/// A bounded optional witness can cover a modest detour around a chunk wall.
/// Longer questions remain with the ordinary resumable route planner.
const REGION_PROOF_NODES: u32 = 64;

struct Ground<'a> {
    world: &'a GameWorld,
    terrain: Option<MapRegionGround<'a>>,
}

impl Ground<'_> {
    /// `id` if it is a completed gate. Navigation reads stable
    /// gate metadata, without projecting construction progress or training
    /// queues for unrelated buildings.
    fn complete_gate(&self, id: aoe_core::EntityId) -> Option<u32> {
        if let Some(probe) = &self.world.planning_probe {
            probe.movement_count("region_gate_candidates", 1);
        }
        self.world
            .economy
            .buildings
            .get(&id)
            .filter(|b| b.kind.is_gate() && b.is_complete())
            .map(|b| b.id.0)
    }

    /// The tiles of completed gates in `[min, max)`.
    fn gates_within(&self, min: TileCoord, max: TileCoord) -> Vec<TileCoord> {
        self.world
            .economy
            .gate_tiles
            .range(TileCoord::new(min.x, i32::MIN)..TileCoord::new(max.x, i32::MIN))
            .filter(|(tile, _)| tile.y >= min.y && tile.y < max.y)
            .filter(|(_, id)| self.complete_gate(**id).is_some())
            .map(|(&tile, _)| tile)
            .collect()
    }
}

impl RegionGround for Ground<'_> {
    fn available(&self) -> bool {
        self.terrain.as_ref().is_none_or(RegionGround::available)
    }
    /// The terrain's bulk capture; gates in or around the chunk are
    /// classified tile by tile on top of it (`special_tiles`).
    fn chunk(&self, key: (i32, i32), size: (i32, i32)) -> Option<RegionChunk> {
        self.terrain
            .as_ref()
            .and_then(|ground| ground.chunk(key, size))
    }
    fn special_tiles(&self, min: TileCoord, max: TileCoord) -> Vec<TileCoord> {
        self.gates_within(min, TileCoord::new(max.x + 1, max.y + 1))
    }
    fn tile(&self, tile: TileCoord) -> RegionTile {
        if let Some(gate) = self
            .world
            .economy
            .gate_tiles
            .get(&tile)
            .and_then(|&id| self.complete_gate(id))
        {
            return RegionTile::Gate(gate);
        }
        let config = self.world.config;
        let open = match &self.world.terrain {
            Terrain::Map { .. } => self
                .terrain
                .as_ref()
                .is_some_and(|ground| ground.tile(tile) == RegionTile::Open),
            Terrain::Uniform(_) => {
                tile.x >= 0
                    && tile.y >= 0
                    && tile.x < config.width_tiles
                    && tile.y < config.height_tiles
                    && !self.world.economy.blocked_tiles.contains(&tile)
            }
        };
        if open {
            RegionTile::Open
        } else {
            RegionTile::Blocked
        }
    }

    fn edge_clear(&self, from: TileCoord, to: TileCoord) -> bool {
        self.terrain
            .as_ref()
            .is_none_or(|ground| ground.edge_clear(from, to))
    }

    fn diagonal_override(&self, from: TileCoord, to: TileCoord) -> Option<bool> {
        self.terrain
            .as_ref()
            .and_then(|ground| ground.diagonal_override(from, to))
    }

    fn gate_owner(&self, gate: u32) -> u32 {
        self.world
            .economy
            .buildings
            .get(&aoe_core::EntityId(gate))
            .map_or(u32::MAX, |b| u32::from(b.player.0))
    }

    fn gate_locked(&self, gate: u32) -> bool {
        self.world
            .economy
            .buildings
            .get(&aoe_core::EntityId(gate))
            .is_some_and(|b| b.gate_locked)
    }

    fn gate_open(&self, gate: u32) -> bool {
        self.world
            .economy
            .open_gates
            .contains(&aoe_core::EntityId(gate))
    }
}

impl GameWorld {
    /// A current directed region witness makes the far planner's separate
    /// reverse destination flood redundant. A negative answer is left to the
    /// planner's existing endpoint refinement, preserving reachable frontiers.
    pub(crate) fn reuse_region_endpoint_proof(
        &self,
        planner: &mut aoe_map::RoutePlanner,
        kind: crate::UnitKind,
        player: PlayerId,
    ) {
        if kind.is_naval() {
            return;
        }
        let Terrain::Map { overlay, .. } = &self.terrain else {
            return;
        };
        let query = planner.pending_region_proof(overlay);
        let Some((from, to)) = query else {
            return;
        };
        let Some(bucket) = LandBucket::for_radius(kind.collision_radius()) else {
            return;
        };
        if let Ok(reachable) = self.land_reachable_bounded(player, bucket, from, to) {
            planner.record_region_proof(from, to, reachable, overlay);
        }
    }

    fn with_regions<R>(
        &self,
        bucket: LandBucket,
        read: impl FnOnce(&mut Regions, &Ground<'_>) -> R,
    ) -> Result<R, GameWorldError> {
        let mut cache = self
            .regions
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let slot = &mut cache[bucket as usize];
        let regions = slot.get_or_insert_with(|| {
            Regions::new(self.config.width_tiles, self.config.height_tiles, bucket)
        });
        let ground = Ground {
            world: self,
            terrain: match &self.terrain {
                Terrain::Map { generator, overlay } => {
                    Some(MapRegionGround::new(generator, overlay, &|| false))
                }
                Terrain::Uniform(_) => None,
            },
        };
        let result = read(regions, &ground);
        if let Some(error) = ground.terrain.as_ref().and_then(MapRegionGround::error) {
            *slot = None;
            Err(GameWorldError::Environment(error))
        } else {
            Ok(result)
        }
    }

    /// Ground in `area` may have changed passability.
    pub(crate) fn regions_changed(&self, area: TileRect) {
        let mut regions = self
            .regions
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        for regions in regions.iter_mut().flatten() {
            regions.invalidate(area.min, area.max);
        }
    }

    pub(crate) fn regions_changed_everywhere(&self) {
        let mut regions = self
            .regions
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        for regions in regions.iter_mut().flatten() {
            regions.invalidate_all();
        }
    }

    /// `land_reachable` from each of `from` to one destination, sharing a
    /// closure between starts in one region.
    pub(crate) fn land_reachable_from_each(
        &self,
        player: PlayerId,
        bucket: LandBucket,
        from: &[TileCoord],
        to: TileCoord,
    ) -> Result<Vec<bool>, GameWorldError> {
        self.with_regions(bucket, |regions, ground| {
            regions.reachable_from_each(ground, u32::from(player.0), from, to)
        })
    }

    /// The land region `player` sees, owned gates and all open gates joined.
    /// Asking builds the whole map's region layer, so only tests ask.
    #[cfg(test)]
    pub(crate) fn land_player_region(
        &self,
        player: PlayerId,
        bucket: LandBucket,
        tile: TileCoord,
    ) -> Result<Option<u32>, GameWorldError> {
        self.with_regions(bucket, |regions, ground| {
            regions.player_component(ground, u32::from(player.0), tile)
        })
    }

    /// True when `player`'s land units can walk from `from` to `to`.
    pub(crate) fn land_reachable(
        &self,
        player: PlayerId,
        bucket: LandBucket,
        from: TileCoord,
        to: TileCoord,
    ) -> Result<bool, GameWorldError> {
        self.with_regions(bucket, |regions, ground| {
            regions.reachable(ground, u32::from(player.0), from, to)
        })
    }

    /// `land_reachable` from one origin to each of `to`, sharing the origin's
    /// closure.
    pub(crate) fn land_reachable_each(
        &self,
        player: PlayerId,
        bucket: LandBucket,
        from: TileCoord,
        to: &[TileCoord],
    ) -> Result<Vec<bool>, GameWorldError> {
        self.with_regions(bucket, |regions, ground| {
            regions.reachable_each(ground, u32::from(player.0), from, to)
        })
    }

    /// Whether every one of `from` reaches `to` by `land_reachable`, sharing
    /// a closure between starts in one region.
    pub(crate) fn land_reachable_from_all(
        &self,
        player: PlayerId,
        bucket: LandBucket,
        from: &[TileCoord],
        to: TileCoord,
    ) -> Result<bool, GameWorldError> {
        self.with_regions(bucket, |regions, ground| {
            regions.reachable_from_all(ground, u32::from(player.0), from, to)
        })
    }

    /// Optional route certificates cannot traverse a continent synchronously.
    /// This bounds only the shortcut; unknown leaves ordinary planning intact.
    pub(crate) fn land_reachable_bounded(
        &self,
        player: PlayerId,
        bucket: LandBucket,
        from: TileCoord,
        to: TileCoord,
    ) -> Result<aoe_map::RegionReachability, GameWorldError> {
        self.with_regions(bucket, |regions, ground| {
            regions.reachable_bounded(ground, u32::from(player.0), from, to, REGION_PROOF_NODES)
        })
    }

    /// The tile nearest to `target` that `player` can reach from `from`.
    pub(crate) fn land_frontier(
        &self,
        player: PlayerId,
        bucket: LandBucket,
        from: TileCoord,
        target: TileCoord,
    ) -> Result<Option<TileCoord>, GameWorldError> {
        self.with_regions(bucket, |regions, ground| {
            regions.frontier(ground, u32::from(player.0), from, target)
        })
    }

    /// Region-layer work counters, for tests and diagnostics.
    pub fn region_stats(&self) -> aoe_map::RegionStats {
        let regions = self
            .regions
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let mut total = aoe_map::RegionStats::default();
        for layer in regions.iter().flatten() {
            let stats = layer.stats();
            total.builds += stats.builds;
            total.relabels += stats.relabels;
            total.unchanged += stats.unchanged;
            total.super_rebuilds += stats.super_rebuilds;
            total.global_rebuilds += stats.global_rebuilds;
        }
        total
    }
}
