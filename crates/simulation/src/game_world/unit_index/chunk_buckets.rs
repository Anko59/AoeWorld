//! The all-unit presence index: the ids of the units in each 32-tile chunk,
//! in an occupied-bounds row-major array instead of an ordered map. A bucket's
//! order is its insertion order with swap removals, exactly as before; only
//! keyed reads ever happened, so nothing observes the storage order.
use aoe_core::{ChunkCoord, EntityId};

#[derive(Debug, Default)]
pub(crate) struct ChunkBuckets {
    /// Lower corner and row width of the allocated rectangle. Empty space
    /// between the map origin and the first occupied chunk costs nothing.
    origin: (usize, usize),
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
        let (x, y) = (x.checked_sub(self.origin.0)?, y.checked_sub(self.origin.1)?);
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
        self.grow_to(x, y);
        let at = (y - self.origin.1) * self.width + (x - self.origin.0);
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

    /// Grow geometrically around occupied bounds, including left/up growth.
    /// Only growth moves buckets; reads and writes inside the bounds stay O(1).
    fn grow_to(&mut self, x: usize, y: usize) {
        if self.buckets.is_empty() {
            self.origin = (x, y);
            self.width = 1;
            self.buckets.push(Vec::new());
            return;
        }
        let rows = self.buckets.len() / self.width;
        let mut origin = (x.min(self.origin.0), y.min(self.origin.1));
        let width = (x + 1).max(self.origin.0 + self.width) - origin.0;
        let height = (y + 1).max(self.origin.1 + rows) - origin.1;
        if width == self.width && height == rows {
            return;
        }
        let required = (width, height);
        let width = if width > self.width {
            width.max(self.width * 2)
        } else {
            width
        };
        let height = if height > rows {
            height.max(rows * 2)
        } else {
            height
        };
        // Put spare capacity on the growing side, so successive left/up steps
        // do not each double the rectangle again.
        if x < self.origin.0 {
            origin.0 = origin.0.saturating_sub(width - required.0);
        }
        if y < self.origin.1 {
            origin.1 = origin.1.saturating_sub(height - required.1);
        }
        let mut buckets = Vec::new();
        buckets.resize_with(width * height, Vec::new);
        let dx = self.origin.0 - origin.0;
        let dy = self.origin.1 - origin.1;
        for (at, ids) in self.buckets.drain(..).enumerate() {
            buckets[(at / self.width + dy) * width + at % self.width + dx] = ids;
        }
        self.buckets = buckets;
        self.origin = origin;
        self.width = width;
    }
}

#[cfg(test)]
mod tests;
