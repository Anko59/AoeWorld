//! The enemies of one chunk as one player sees them, read once per combat
//! step and shared by every look of that player there. A look then walks
//! the few enemies of its chunks, not every own unit standing beside it.
use super::*;
use std::collections::BTreeMap;
use std::sync::Arc;

/// A visible enemy as the combat step found it. Units do not move during the
/// step; hit points and owner are read again for an enemy a look accepts.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Contact {
    pub(crate) id: EntityId,
    pub(crate) position: WorldPosition,
    pub(crate) radius: i32,
    pub(crate) civilian: bool,
    pub(crate) siege: bool,
}

/// Living units of other players in one chunk.
#[derive(Debug, Default)]
pub(crate) struct ChunkContacts {
    /// A living unit of another player stands in the chunk, seen or not.
    pub(crate) hostile: bool,
    /// Those of them the player sees and that are not embarked, in bucket order.
    pub(crate) visible: Vec<Contact>,
    /// One bit per 8-tile cell of the chunk holding a visible enemy, so a
    /// look whose sight misses them all reads none of them.
    pub(crate) cells: u16,
    /// One bit per cell holding a building tile of another player.
    pub(crate) buildings: u16,
}

pub(crate) use crate::game_world::unit_index::presence::{cell_bit, cell_mask};

/// Contacts read during the current combat step; empty outside it, so no
/// other phase reads positions from before its own movement.
#[derive(Debug, Default)]
pub(crate) struct ContactCache {
    live: bool,
    chunks: BTreeMap<(ChunkCoord, PlayerId), Arc<ChunkContacts>>,
}

impl ContactCache {
    pub(crate) fn open(&mut self) {
        self.live = true;
        self.chunks.clear();
    }

    pub(crate) fn close(&mut self) {
        self.live = false;
        self.chunks.clear();
    }
}

impl GameWorld {
    /// `chunk`'s contacts for `player`, read from the bucket at most once per
    /// combat step. A chunk the presence summary shows without any other
    /// player is not read at all.
    pub(in crate::game_world::combat) fn chunk_contacts(
        &mut self,
        chunk: ChunkCoord,
        player: PlayerId,
    ) -> Arc<ChunkContacts> {
        if let Some(contacts) = self.combat.contacts.chunks.get(&(chunk, player)) {
            return contacts.clone();
        }
        let contacts = Arc::new(self.read_chunk_contacts(chunk, player));
        if self.combat.contacts.live {
            self.combat
                .contacts
                .chunks
                .insert((chunk, player), contacts.clone());
        }
        contacts
    }

    /// `enemy_buildings_near` from the cell bits of the step's contacts: each
    /// chunk's cells are asked once per combat step, not once per look.
    pub(in crate::game_world::combat) fn building_cells_near(
        &mut self,
        bounds: TileRect,
        player: PlayerId,
    ) -> bool {
        if !self.combat.contacts.live {
            return self.enemy_buildings_near(bounds, player);
        }
        let size = SPATIAL_CHUNK_TILES;
        for y in bounds.min.y.div_euclid(size)..=(bounds.max.y - 1).div_euclid(size) {
            for x in bounds.min.x.div_euclid(size)..=(bounds.max.x - 1).div_euclid(size) {
                let chunk = ChunkCoord { x, y };
                if self.vision.building_chunks.others(chunk, player)
                    && self.chunk_contacts(chunk, player).buildings & cell_mask(chunk, bounds) != 0
                {
                    return true;
                }
            }
        }
        false
    }

    fn read_chunk_contacts(&mut self, chunk: ChunkCoord, player: PlayerId) -> ChunkContacts {
        let mut out = ChunkContacts::default();
        if self.vision.building_chunks.others(chunk, player) {
            let (x0, y0) = (chunk.x * SPATIAL_CHUNK_TILES, chunk.y * SPATIAL_CHUNK_TILES);
            let cell = crate::game_world::unit_index::presence::PRESENCE_CELL_TILES;
            for bit in 0..16 {
                let rect =
                    TileRect::from_xywh(x0 + bit % 4 * cell, y0 + bit / 4 * cell, cell, cell);
                if self.vision.building_cells.others_in_cells(rect, player) {
                    out.buildings |= 1 << bit;
                }
            }
        }
        if !self.chunks.presence.others(chunk, player) {
            return out;
        }
        let Some(bucket) = self.chunks.get(&chunk) else {
            return out;
        };
        let read = bucket.len() as u32;
        for &id in bucket {
            let Some(u) = self.unit(id) else {
                continue;
            };
            if u.player == player || u.hp == 0 {
                continue;
            }
            out.hostile = true;
            if !self.embarked(id) && self.unit_visible_to(player, id) {
                out.cells |= cell_bit(chunk, u.position.tile_floor());
                out.visible.push(Contact {
                    id,
                    position: u.position,
                    radius: u.kind.collision_radius(),
                    civilian: matches!(
                        u.kind,
                        crate::UnitKind::Villager
                            | crate::UnitKind::FishingShip
                            | crate::UnitKind::TransportShip
                    ),
                    siege: u.kind.is_siege(),
                });
            }
        }
        self.combat.stats.targeting_scanned += read;
        out
    }
}
