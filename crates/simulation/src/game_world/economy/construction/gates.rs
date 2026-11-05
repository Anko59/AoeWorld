//! A completed (palisade or stone) gate opens when an owned unit approaches, and cannot close
//! through a living unit. It is four by one tiles along a wall line in either axis.
//! Route plans treat a completed gate as open ground (see `routes_through_gate`);
//! only movement collision (`blocked_tiles`) closes it.
use crate::{GameWorld, Terrain};
use aoe_core::{TileCoord, TileRect};

impl GameWorld {
    pub(super) fn advance_gates(&mut self) {
        let gates: Vec<_> = self
            .buildings()
            .filter(|b| b.kind.is_gate() && b.is_complete())
            .map(|b| (b.id, b.player, b.footprint))
            .collect();
        for (id, owner, rect) in gates {
            if let Terrain::Map { overlay, .. } = &mut self.terrain
                && overlay.blocks_tile(rect.min)
            {
                overlay.remove_footprint(rect);
                self.collision_opened(rect);
            }
            let query = TileRect::new(
                TileCoord::new((rect.min.x - 2).max(0), (rect.min.y - 2).max(0)),
                TileCoord::new(
                    (rect.max.x + 2).min(self.config.width_tiles),
                    (rect.max.y + 2).min(self.config.height_tiles),
                ),
            );
            let nearby = self.query(query).0;
            let open = nearby.iter().any(|u| u.hp > 0 && u.player == owner)
                || nearby
                    .iter()
                    .any(|u| u.hp > 0 && rect.contains(u.position.tile_floor()));
            if open == self.economy.open_gates.contains(&id) {
                continue;
            }
            for y in rect.min.y..rect.max.y {
                for x in rect.min.x..rect.max.x {
                    if open {
                        self.economy.blocked_tiles.remove(&TileCoord::new(x, y));
                    } else {
                        self.economy.blocked_tiles.insert(TileCoord::new(x, y));
                    }
                }
            }
            if open {
                self.economy.open_gates.insert(id);
            } else {
                self.economy.open_gates.remove(&id);
            }
            if let Some(gate) = self.economy.buildings.get_mut(&id) {
                gate.gate_open = open;
            }
            if open {
                self.collision_opened(rect);
            } else {
                self.collision_closed(rect);
            }
        }
    }
}
