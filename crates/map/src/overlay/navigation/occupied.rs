//! Collision domain flags are addressed directly inside their owning chunk.
use crate::CHUNK_TILES;
use aoe_core::TileCoord;

const TILES: usize = (CHUNK_TILES * CHUNK_TILES) as usize;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct Occupied {
    flags: Option<Box<[u8; TILES]>>,
    count: u16,
}

impl Occupied {
    fn offset(tile: TileCoord) -> usize {
        (tile.y.rem_euclid(CHUNK_TILES) * CHUNK_TILES + tile.x.rem_euclid(CHUNK_TILES)) as usize
    }

    pub(super) fn contains_key(&self, tile: &TileCoord) -> bool {
        self.flags
            .as_ref()
            .is_some_and(|flags| flags[Self::offset(*tile)] != 0)
    }

    pub(super) fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub(super) fn toggle(&mut self, tile: TileCoord, flag: u8) -> bool {
        let flags = self.flags.get_or_insert_with(|| Box::new([0; TILES]));
        let at = Self::offset(tile);
        let was = flags[at] != 0;
        flags[at] ^= flag;
        let now = flags[at] != 0;
        match (was, now) {
            (false, true) => self.count += 1,
            (true, false) => self.count -= 1,
            _ => {}
        }
        let present = flags[at] & flag != 0;
        if self.count == 0 {
            self.flags = None;
        }
        present
    }
}
