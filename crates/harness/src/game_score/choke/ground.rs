//! The ground of a real world, as the survey and the staging read it: a tile is walkable when
//! the route planner says so (`RouteTerrain::route_walkable`, which counts the map's own
//! passability, the trees and other resource nodes on it, and bridges). The glyphs are the survey's
//! classes of a tile, for the pictures of a neck.
use super::{Ground, Tile};
use aoe_core::TileCoord;
use aoe_map::{MapChunkGenerator, ResourceOverlay, RouteTerrain, WaterKind};
use aoe_simulation::{GameWorld, Terrain};

pub(in crate::game_score) struct MapGround<'a> {
    side: i32,
    generator: &'a MapChunkGenerator,
    overlay: &'a ResourceOverlay,
}

impl<'a> MapGround<'a> {
    /// The ground of `world`; `None` for a world that is not a map.
    pub fn of(world: &'a GameWorld) -> Option<Self> {
        let Terrain::Map { generator, overlay } = world.terrain() else {
            return None;
        };
        let config = world.config();
        Some(Self {
            side: config.width_tiles.min(config.height_tiles),
            generator,
            overlay,
        })
    }
}

impl Ground for MapGround<'_> {
    fn side(&self) -> i32 {
        self.side
    }

    fn walkable(&self, (x, y): Tile) -> bool {
        self.generator
            .route_walkable(self.overlay, TileCoord::new(x, y), &|| false)
            .unwrap_or(false)
    }

    /// `.` ground, `,` wet ground a unit can walk, `~` a river, `=` a lake, the sea or shallows
    /// that block, `T` land that blocks (trees, cliffs, steep slopes).
    fn glyph(&self, tile: Tile) -> char {
        let walkable = self.walkable(tile);
        let water = self
            .generator
            .tile_and_node_with_cancel(TileCoord::new(tile.0, tile.1), &|| false)
            .ok()
            .flatten()
            .map_or(WaterKind::None, |(sample, _)| sample.water);
        match (walkable, water) {
            (true, WaterKind::None) => '.',
            (true, _) => ',',
            (false, WaterKind::River) => '~',
            (false, WaterKind::None) => 'T',
            (false, _) => '=',
        }
    }
}
