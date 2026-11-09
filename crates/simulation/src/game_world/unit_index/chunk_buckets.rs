//! The all-unit presence index: the ids of the units in each 32-tile chunk,
//! in a flat row-major array of buckets instead of an ordered map. A bucket's
//! order is its insertion order with swap removals, exactly as before; only
//! keyed reads ever happened, so nothing observes the storage order.
use aoe_core::{ChunkCoord, EntityId};

#[derive(Debug, Default)]
pub(crate) struct ChunkBuckets {
    /// Chunks per row; the array grows to the farthest chunk ever occupied.
    width: usize,
    buckets: Vec<Vec<EntityId>>,
    occupied: usize,
}

impl ChunkBuckets {
    fn at(&self, chunk: ChunkCoord) -> Option<usize> {
        let (x, y) = (
            usize::try_from(chunk.x).ok()?,
            usize::try_from(chunk.y).ok()?,
        );
        (x < self.width).then(|| y * self.width + x)
    }

    /// The bucket's ids; `None` for an empty chunk.
    pub(crate) fn get(&self, chunk: &ChunkCoord) -> Option<&Vec<EntityId>> {
        self.at(*chunk)
            .and_then(|at| self.buckets.get(at))
            .filter(|ids| !ids.is_empty())
    }

    /// Chunks holding at least one unit.
    pub(crate) fn len(&self) -> usize {
        self.occupied
    }

    /// Appends `id` to the chunk's bucket and returns its slot there.
    pub(crate) fn push(&mut self, chunk: ChunkCoord, id: EntityId) -> usize {
        // Unit positions are on the map, so their chunks are never negative.
        debug_assert!(chunk.x >= 0 && chunk.y >= 0, "{chunk:?}");
        let (x, y) = (chunk.x.max(0) as usize, chunk.y.max(0) as usize);
        if x >= self.width {
            self.widen(x + 1);
        }
        let at = y * self.width + x;
        if at >= self.buckets.len() {
            self.buckets.resize_with(at + 1, Vec::new);
        }
        let ids = &mut self.buckets[at];
        self.occupied += usize::from(ids.is_empty());
        ids.push(id);
        ids.len() - 1
    }

    /// Removes the id at `slot`; returns the id the removal moved into that
    /// slot (the bucket's last), if any.
    pub(crate) fn swap_remove(&mut self, chunk: ChunkCoord, slot: usize) -> Option<EntityId> {
        let at = self.at(chunk)?;
        let ids = self.buckets.get_mut(at).filter(|ids| slot < ids.len())?;
        ids.swap_remove(slot);
        self.occupied -= usize::from(ids.is_empty());
        ids.get(slot).copied()
    }

    /// Re-lays the rows out `width` chunks wide. Rare: once per new
    /// farthest column.
    fn widen(&mut self, width: usize) {
        let width = width.max(self.width * 2).max(16);
        let rows = self.buckets.len().div_ceil(self.width.max(1));
        let mut buckets = Vec::new();
        buckets.resize_with(rows * width, Vec::new);
        for (at, ids) in self.buckets.drain(..).enumerate() {
            let (x, y) = (at % self.width, at / self.width);
            buckets[y * width + x] = ids;
        }
        self.buckets = buckets;
        self.width = width;
    }
}

#[cfg(test)]
mod tests;
