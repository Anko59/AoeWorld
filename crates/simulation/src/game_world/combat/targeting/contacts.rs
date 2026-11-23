//! The enemies of one chunk as one player sees them, read once per combat
//! step and shared by every look of that player there. A look then walks
//! the few enemies of its chunks, not every own unit standing beside it.
use super::*;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Living units of other players in one chunk.
#[derive(Debug, Default)]
pub(crate) struct ChunkContacts {
    /// A living unit of another player stands in the chunk, seen or not.
    pub(crate) hostile: bool,
    /// Those of them the player sees and that are not embarked, in bucket
    /// order. Hit points, owner and position are read again at each look.
    pub(crate) visible: Vec<EntityId>,
}

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

    fn read_chunk_contacts(&mut self, chunk: ChunkCoord, player: PlayerId) -> ChunkContacts {
        let mut out = ChunkContacts::default();
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
                out.visible.push(id);
            }
        }
        self.combat.stats.targeting_scanned += read;
        out
    }
}
