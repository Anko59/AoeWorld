use super::*;
use crate::Terrain;
use aoe_core::TileCoord;

// Both natural and starting-node identities encode their immutable tile. This
// does not query terrain, page providers, or every resource on a large map.
fn resource_tile(id: u64) -> TileCoord {
    TileCoord::new(
        ((id >> 1) & 0x3_ffff) as i32,
        ((id >> 19) & 0x3_ffff) as i32,
    )
}

impl GameWorld {
    pub(crate) fn remember_visible_resources(&mut self) {
        if self.vision.disabled {
            return;
        }
        let Some(snapshot) = self.resource_snapshot([0; 32]) else {
            return;
        };
        for change in snapshot.changes {
            self.record_observed_resource(change.id, change.remaining);
        }
    }

    pub(crate) fn record_observed_resource(&mut self, id: u64, remaining: u16) {
        let tile = resource_tile(id);
        for vision in self.vision.players.values_mut() {
            if vision.state(tile) == TileVisibility::Visible {
                vision.resources.insert(id, remaining);
            }
        }
    }

    /// Probes only the tiles that just became visible; amounts that change
    /// while a tile stays visible arrive through `record_observed_resource`.
    pub(crate) fn observe_resource_tiles(&mut self, player: PlayerId, tiles: &[TileCoord]) {
        let Terrain::Map { overlay, .. } = &self.terrain else {
            return;
        };
        let Some(vision) = self.vision.players.get_mut(&player) else {
            return;
        };
        let memory = &mut vision.resources;
        for tile in tiles {
            let base = (tile.y as u64) << 19 | (tile.x as u64) << 1;
            for prefix in [
                0,
                1 << 37,
                (1 << 37) | (1 << 38),
                (1 << 37) | (2 << 38),
                (1 << 37) | (3 << 38),
            ] {
                let id = base | prefix;
                if let Some(remaining) = overlay.changed_amount(id) {
                    memory.insert(id, remaining);
                }
            }
        }
    }

    /// Only modified/starting nodes need amounts; undiscovered immutable nodes
    /// keep their initial amount. A hidden depletion never updates memory.
    pub fn known_resource_amounts(&self, player: PlayerId) -> Vec<(u64, u16)> {
        if self.vision.disabled {
            return self.resource_snapshot([0; 32]).map_or_else(Vec::new, |s| {
                s.changes.into_iter().map(|c| (c.id, c.remaining)).collect()
            });
        }
        self.vision.players.get(&player).map_or_else(Vec::new, |v| {
            v.resources
                .iter()
                .map(|(id, remaining)| (*id, *remaining))
                .collect()
        })
    }
}
