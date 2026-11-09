//! Land reachability for orders, read from `aoe_map::Regions` over the same
//! ground the route planner searches. A completed gate is a gated portal: it
//! admits its owner only, whatever its open state, so toggling it changes no
//! region. The layer is a pure cache of the ground; it is never hashed.
use crate::{GameWorld, GameWorldError, Terrain};
use aoe_core::{PlayerId, TileCoord, TileRect};
use aoe_map::{MapRegionGround, RegionChunk, RegionGround, RegionTile, Regions};
use std::cell::OnceCell;
use std::collections::BTreeMap;

struct Ground<'a> {
    world: &'a GameWorld,
    /// Completed gates' tiles, read only when a chunk is relabelled.
    gates: OnceCell<BTreeMap<TileCoord, (u32, u32)>>,
    terrain: Option<MapRegionGround<'a>>,
}

impl Ground<'_> {
    fn gates(&self) -> &BTreeMap<TileCoord, (u32, u32)> {
        self.gates.get_or_init(|| {
            let mut gates = BTreeMap::new();
            for gate in self
                .world
                .buildings()
                .filter(|b| b.kind.is_gate() && b.is_complete())
            {
                let rect = gate.footprint;
                for y in rect.min.y..rect.max.y {
                    for x in rect.min.x..rect.max.x {
                        gates.insert(TileCoord::new(x, y), (gate.id.0, u32::from(gate.player.0)));
                    }
                }
            }
            gates
        })
    }
}

impl RegionGround for Ground<'_> {
    fn chunk(&self, key: (i32, i32), size: (i32, i32)) -> Option<RegionChunk> {
        let min = TileCoord::new(
            key.0 * aoe_map::CHUNK_TILES - 1,
            key.1 * aoe_map::CHUNK_TILES - 1,
        );
        let max = TileCoord::new(
            min.x + aoe_map::CHUNK_TILES + 2,
            min.y + aoe_map::CHUNK_TILES + 2,
        );
        if self
            .gates()
            .keys()
            .any(|tile| tile.x >= min.x && tile.y >= min.y && tile.x < max.x && tile.y < max.y)
        {
            return None;
        }
        self.terrain
            .as_ref()
            .and_then(|ground| ground.chunk(key, size))
    }
    fn tile(&self, tile: TileCoord) -> RegionTile {
        if let Some(&(gate, _)) = self.gates().get(&tile) {
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
        self.gates()
            .values()
            .find(|(id, _)| *id == gate)
            .map_or(u32::MAX, |&(_, owner)| owner)
    }
}

impl GameWorld {
    fn with_regions<R>(
        &self,
        read: impl FnOnce(&mut Regions, &Ground<'_>) -> R,
    ) -> Result<R, GameWorldError> {
        let mut cache = self
            .regions
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let regions = cache
            .get_or_insert_with(|| Regions::new(self.config.width_tiles, self.config.height_tiles));
        let ground = Ground {
            world: self,
            gates: OnceCell::new(),
            terrain: match &self.terrain {
                Terrain::Map { generator, overlay } => {
                    Some(MapRegionGround::new(generator, overlay, &|| false))
                }
                Terrain::Uniform(_) => None,
            },
        };
        let result = read(regions, &ground);
        if let Some(error) = ground.terrain.as_ref().and_then(MapRegionGround::error) {
            *cache = None;
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
        if let Some(regions) = regions.as_mut() {
            regions.invalidate(area.min, area.max);
        }
    }

    pub(crate) fn regions_changed_everywhere(&self) {
        let mut regions = self
            .regions
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(regions) = regions.as_mut() {
            regions.invalidate_all();
        }
    }

    /// The land region `player` sees a tile in, its own gates joined.
    pub(crate) fn land_player_region(
        &self,
        player: PlayerId,
        tile: TileCoord,
    ) -> Result<Option<u32>, GameWorldError> {
        self.with_regions(|regions, ground| {
            regions.player_component(ground, u32::from(player.0), tile)
        })
    }

    /// True when `player`'s land units can walk from `from` to `to`.
    pub(crate) fn land_reachable(
        &self,
        player: PlayerId,
        from: TileCoord,
        to: TileCoord,
    ) -> Result<bool, GameWorldError> {
        self.with_regions(|regions, ground| {
            regions.reachable(ground, u32::from(player.0), from, to)
        })
    }

    /// The tile nearest to `target` that `player` can reach from `from`.
    pub(crate) fn land_frontier(
        &self,
        player: PlayerId,
        from: TileCoord,
        target: TileCoord,
    ) -> Result<Option<TileCoord>, GameWorldError> {
        self.with_regions(|regions, ground| {
            regions.frontier(ground, u32::from(player.0), from, target)
        })
    }

    /// Region-layer work counters, for tests and diagnostics.
    pub fn region_stats(&self) -> aoe_map::RegionStats {
        self.regions
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .as_ref()
            .map_or_else(Default::default, Regions::stats)
    }
}
