//! Bounded per-thread reads of immutable collision shards. A weak owner keeps
//! Arc::make_mut from changing a cached shard in place, without retaining its map.
use super::{ChunkTable, CollisionChunk, Key, Shard};
use std::{
    cell::RefCell,
    sync::{Arc, Weak},
};

struct Entry {
    owner: Weak<ChunkTable>,
    at: Key,
    chunk: Option<Arc<CollisionChunk>>,
}

#[derive(Default)]
struct Recent {
    slots: [Option<Entry>; 4],
    next: usize,
}

thread_local! { static RECENT: RefCell<Recent> = RefCell::new(Recent::default()); }

#[inline]
pub(super) fn read<R>(
    owner: &Shard,
    at: Key,
    read: impl FnOnce(Option<&CollisionChunk>) -> R,
) -> R {
    RECENT.with(|recent| {
        let mut recent = recent.borrow_mut();
        if let Some(entry) = recent
            .slots
            .iter()
            .flatten()
            .find(|entry| entry.at == at && entry.owner.as_ptr() == Arc::as_ptr(owner))
        {
            return read(entry.chunk.as_deref());
        }
        let chunk = owner.get(&at).cloned();
        let result = read(chunk.as_deref());
        let slot = recent.next;
        recent.slots[slot] = Some(Entry {
            owner: Arc::downgrade(owner),
            at,
            chunk,
        });
        recent.next = (slot + 1) % recent.slots.len();
        result
    })
}
