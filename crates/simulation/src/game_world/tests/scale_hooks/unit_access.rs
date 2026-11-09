//! Test-only access accounting at the unit storage boundary, not at callers.
//!
//! Indexing charges the returned slots. Any borrowed Vec/slice view conservatively
//! charges every exposed slot, covering iterators, coercions and bulk methods.
//! This is an access upper bound, not a performance metric. The journal embeds
//! this only in tests; production embeds a Vec. Atomics preserve Send + Sync.
use super::StoredUnit;
use std::ops::{Deref, DerefMut, Index, IndexMut};
use std::slice::SliceIndex;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, Default)]
pub(crate) struct UnitStore {
    units: Vec<StoredUnit>,
    accesses: AtomicUsize,
}

impl UnitStore {
    pub(crate) fn reset_accesses(&self) {
        self.accesses.store(0, Ordering::Relaxed);
    }

    pub(crate) fn accesses(&self) -> usize {
        self.accesses.load(Ordering::Relaxed)
    }

    pub(crate) fn len(&self) -> usize {
        self.units.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.units.is_empty()
    }

    // The correctness oracle deliberately scans outside the measured operation.
    // Journaling also uses this view to avoid charging the caller twice for
    // the slots its mutable view already exposes.
    pub(crate) fn oracle_slice(&self) -> &[StoredUnit] {
        &self.units
    }
}

impl<I: SliceIndex<[StoredUnit]>> Index<I> for UnitStore {
    type Output = I::Output;

    fn index(&self, index: I) -> &Self::Output {
        let output = &self.units[index];
        self.accesses.fetch_add(
            std::mem::size_of_val(output) / std::mem::size_of::<StoredUnit>(),
            Ordering::Relaxed,
        );
        output
    }
}

impl<I: SliceIndex<[StoredUnit]>> IndexMut<I> for UnitStore {
    fn index_mut(&mut self, index: I) -> &mut Self::Output {
        let output = &mut self.units[index];
        self.accesses.fetch_add(
            std::mem::size_of_val(output) / std::mem::size_of::<StoredUnit>(),
            Ordering::Relaxed,
        );
        output
    }
}

impl Deref for UnitStore {
    type Target = Vec<StoredUnit>;

    fn deref(&self) -> &Self::Target {
        self.accesses.fetch_add(self.units.len(), Ordering::Relaxed);
        &self.units
    }
}

impl DerefMut for UnitStore {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.accesses.fetch_add(self.units.len(), Ordering::Relaxed);
        &mut self.units
    }
}

impl<'a> IntoIterator for &'a UnitStore {
    type Item = &'a StoredUnit;
    type IntoIter = std::slice::Iter<'a, StoredUnit>;

    fn into_iter(self) -> Self::IntoIter {
        self.deref().iter()
    }
}

impl<'a> IntoIterator for &'a mut UnitStore {
    type Item = &'a mut StoredUnit;
    type IntoIter = std::slice::IterMut<'a, StoredUnit>;

    fn into_iter(self) -> Self::IntoIter {
        self.deref_mut().iter_mut()
    }
}
