//! The awake seekers, kept beside a split by id modulo `PHASES`, so the
//! acquisition pass reads only the seekers whose turn it is rather than
//! every awake one each tick.
use aoe_core::EntityId;
use std::collections::BTreeSet;

/// Buckets of the split. A turn period that divides it (1, 2, 4 or 8
/// ticks) reads whole buckets; any other falls back to a filter.
const PHASES: u64 = 8;

#[derive(Debug, Default)]
pub(crate) struct Awake {
    all: BTreeSet<EntityId>,
    phases: [BTreeSet<EntityId>; PHASES as usize],
}

fn bucket(id: EntityId) -> usize {
    (u64::from(id.0) % PHASES) as usize
}

impl Awake {
    pub(crate) fn insert(&mut self, id: EntityId) {
        if self.all.insert(id) {
            self.phases[bucket(id)].insert(id);
        }
    }

    pub(crate) fn remove(&mut self, id: &EntityId) {
        if self.all.remove(id) {
            self.phases[bucket(*id)].remove(id);
        }
    }

    pub(crate) fn contains(&self, id: &EntityId) -> bool {
        self.all.contains(id)
    }

    pub(crate) fn extend(&mut self, ids: impl IntoIterator<Item = EntityId>) {
        for id in ids {
            self.insert(id);
        }
    }

    #[cfg(test)]
    pub(crate) fn set(&self) -> &BTreeSet<EntityId> {
        &self.all
    }

    /// The seekers with `(tick + id) % period == 0`, in id order.
    pub(crate) fn turn(&self, tick: u64, period: u64) -> Vec<EntityId> {
        let period = period.max(1);
        if !PHASES.is_multiple_of(period) {
            return self
                .all
                .iter()
                .copied()
                .filter(|id| (tick + u64::from(id.0)).is_multiple_of(period))
                .collect();
        }
        let mut out: Vec<EntityId> = (0..PHASES)
            .filter(|&k| (tick + k).is_multiple_of(period))
            .flat_map(|k| self.phases[k as usize].iter().copied())
            .collect();
        out.sort_unstable();
        out
    }
}
