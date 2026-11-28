//! The enemies of one chunk as one player sees them, read once per combat
//! step and shared by every look of that player there. A look then walks
//! the few enemies of its chunks, not every own unit standing beside it.
use super::*;
use std::collections::BTreeMap;
use std::sync::Arc;

/// A living unit as the combat step found it. Units do not move during the
/// step; hit points, owner and sight are read again for one a look accepts.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Contact {
    pub(crate) id: EntityId,
    pub(crate) player: PlayerId,
    pub(crate) position: WorldPosition,
    pub(crate) radius: i32,
    pub(crate) civilian: bool,
    pub(crate) siege: bool,
}

/// The living units of one chunk, read once per combat step for every
/// player's looks there, and which 8-tile cells each owner's units and
/// building tiles occupy.
#[derive(Debug, Default)]
pub(crate) struct ChunkContacts {
    /// Not embarked, in bucket order.
    pub(crate) units: Vec<Contact>,
    cells: Vec<(PlayerId, u16)>,
}

fn enemy_cells(world: &GameWorld, masks: &[(PlayerId, u16)], viewer: PlayerId) -> u16 {
    masks
        .iter()
        .filter(|(p, _)| world.is_enemy(viewer, *p))
        .fold(0, |all, (_, bits)| all | bits)
}

fn mark(masks: &mut Vec<(PlayerId, u16)>, player: PlayerId, bits: u16) {
    match masks.iter_mut().find(|(p, _)| *p == player) {
        Some((_, old)) => *old |= bits,
        None => masks.push((player, bits)),
    }
}

pub(crate) use crate::game_world::unit_index::presence::{cell_bit, cell_mask};

/// Contacts read during the current combat step; empty outside it, so no
/// other phase reads positions from before its own movement.
#[derive(Debug, Default)]
pub(crate) struct ContactCache {
    live: bool,
    chunks: BTreeMap<ChunkCoord, Arc<ChunkContacts>>,
    /// Per chunk, the cells holding each owner's building tiles.
    buildings: BTreeMap<ChunkCoord, Vec<(PlayerId, u16)>>,
}

impl ContactCache {
    pub(crate) fn open(&mut self) {
        self.live = true;
        self.chunks.clear();
        self.buildings.clear();
    }

    pub(crate) fn close(&mut self) {
        self.live = false;
        self.chunks.clear();
        self.buildings.clear();
    }
}

/// Buckets this small are read in place at each look: building and keeping
/// a shared list costs more than reading a few units again.
const SMALL_BUCKET: usize = 16;

fn contact(u: &crate::GameUnit) -> Contact {
    Contact {
        id: u.id,
        player: u.player,
        position: u.position,
        radius: u.kind.collision_radius(),
        civilian: matches!(
            u.kind,
            crate::UnitKind::Villager
                | crate::UnitKind::FishingShip
                | crate::UnitKind::TransportShip
        ),
        siege: u.kind.is_siege(),
    }
}

impl GameWorld {
    /// Calls `f` with every living, not embarked unit of a player other than
    /// `viewer` in the cells of `chunk` that `bounds` touches. A small bucket
    /// is read in place; a larger one through the step's shared contacts.
    pub(in crate::game_world::combat) fn each_enemy_contact(
        &mut self,
        chunk: ChunkCoord,
        viewer: PlayerId,
        bounds: TileRect,
        mut f: impl FnMut(&GameWorld, &Contact),
    ) {
        let mask = cell_mask(chunk, bounds);
        let len = self.chunks.get(&chunk).map_or(0, Vec::len);
        if len == 0 || mask == 0 {
            return;
        }
        if len <= SMALL_BUCKET {
            self.combat.stats.targeting_scanned += len as u32;
            let world = &*self;
            for &id in world.chunks.get(&chunk).into_iter().flatten() {
                // By reference: most entries are rejected on the owner alone.
                let Some(u) = world.lookup.get(&id).map(|&at| &world.units[at].state) else {
                    continue;
                };
                if world.is_enemy(viewer, u.player)
                    && u.hp > 0
                    && cell_bit(chunk, u.position.tile_floor()) & mask != 0
                    && !world.embarked(id)
                {
                    f(world, &contact(u));
                }
            }
            return;
        }
        let contacts = self.chunk_contacts(chunk);
        if enemy_cells(self, &contacts.cells, viewer) & mask == 0 {
            return;
        }
        for c in contacts
            .units
            .iter()
            .filter(|c| self.is_enemy(viewer, c.player))
        {
            f(self, c);
        }
    }

    /// `chunk`'s contacts, read from the bucket at most once per combat step
    /// whoever looks there.
    pub(in crate::game_world::combat) fn chunk_contacts(
        &mut self,
        chunk: ChunkCoord,
    ) -> Arc<ChunkContacts> {
        if let Some(contacts) = self.combat.contacts.chunks.get(&chunk) {
            return contacts.clone();
        }
        let contacts = Arc::new(self.read_chunk_contacts(chunk));
        if self.combat.contacts.live {
            self.combat.contacts.chunks.insert(chunk, contacts.clone());
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
                if !self.enemies_present(&self.vision.building_chunks, chunk, player) {
                    continue;
                }
                if !self.combat.contacts.buildings.contains_key(&chunk) {
                    let cells = self.read_building_cells(chunk);
                    self.combat.contacts.buildings.insert(chunk, cells);
                }
                if enemy_cells(self, &self.combat.contacts.buildings[&chunk], player)
                    & cell_mask(chunk, bounds)
                    != 0
                {
                    return true;
                }
            }
        }
        false
    }

    fn read_building_cells(&self, chunk: ChunkCoord) -> Vec<(PlayerId, u16)> {
        let mut out = Vec::new();
        let (x0, y0) = (chunk.x * SPATIAL_CHUNK_TILES, chunk.y * SPATIAL_CHUNK_TILES);
        let cell = crate::game_world::unit_index::presence::PRESENCE_CELL_TILES;
        for bit in 0..16 {
            let at = aoe_core::TileCoord::new(x0 + bit % 4 * cell, y0 + bit / 4 * cell);
            for player in self.vision.building_cells.players_at_cell(at) {
                mark(&mut out, player, 1 << bit);
            }
        }
        out
    }

    fn read_chunk_contacts(&mut self, chunk: ChunkCoord) -> ChunkContacts {
        let mut out = ChunkContacts::default();
        let Some(bucket) = self.chunks.get(&chunk) else {
            return out;
        };
        let read = bucket.len() as u32;
        out.units.reserve_exact(bucket.len());
        for &id in bucket {
            let Some(u) = self.unit(id) else {
                continue;
            };
            if u.hp == 0 || self.embarked(id) {
                continue;
            }
            mark(
                &mut out.cells,
                u.player,
                cell_bit(chunk, u.position.tile_floor()),
            );
            out.units.push(contact(&u));
        }
        self.combat.stats.targeting_scanned += read;
        out
    }
}
