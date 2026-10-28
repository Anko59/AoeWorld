//! Static clearance along continuous raster bridge decks.
use crate::{GameWorld, Terrain};
use aoe_core::{TileCoord, WorldPosition};

impl GameWorld {
    // Raster bridge links form a one-tile-wide continuous deck. On a diagonal
    // link its sides cross water tile corners: water there is not a solid wall.
    // Foundations/buildings are still checked independently above.
    pub(in crate::game_world::movement_step) fn bridge_water_clear(&self, tile: TileCoord) -> bool {
        let Terrain::Map { generator, overlay } = &self.terrain else {
            return false;
        };
        !overlay.blocks_tile(tile)
            && generator
                .tile_and_node_with_cancel(tile, &|| false)
                .ok()
                .flatten()
                .is_some_and(|(t, node)| {
                    t.water != aoe_map::WaterKind::None
                        && node.is_none_or(|n| !overlay.blocks_node(n))
                })
    }

    pub(in crate::game_world::movement_step) fn on_bridge_corridor(
        &self,
        at: WorldPosition,
        radius: i32,
    ) -> bool {
        let Terrain::Map { overlay, .. } = &self.terrain else {
            return false;
        };
        let here = at.tile_floor();
        let margin = i128::from(512 - radius);
        for y in -1..=1 {
            for x in -1..=1 {
                let from = TileCoord::new(here.x + x, here.y + y);
                let Some(mask) = overlay.bridge_connections(from) else {
                    continue;
                };
                let Ok(start) = WorldPosition::from_tile_center(from) else {
                    continue;
                };
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        if mask & (1 << ((dy + 1) * 3 + dx + 1)) == 0 {
                            continue;
                        }
                        let to = TileCoord::new(from.x + dx, from.y + dy);
                        if overlay.bridge_edge(from, to) != Some(true) {
                            continue;
                        }
                        let vx = i128::from(dx * 1024);
                        let vy = i128::from(dy * 1024);
                        let px = i128::from(at.x - start.x);
                        let py = i128::from(at.y - start.y);
                        let len = vx * vx + vy * vy;
                        let dot = px * vx + py * vy;
                        if len > 0
                            && dot >= 0
                            && dot <= len
                            && (px * vy - py * vx).pow(2) <= margin * margin * len
                        {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }
}
