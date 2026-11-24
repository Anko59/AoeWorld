//! The awake seekers, split into the active (marching, fighting, attacked:
//! a look every fourth tick) and the idle (a look once a second), each
//! bucketed by id modulo its turn, so a tick reads only the seekers due on
//! it. A seeker found idle on its turn moves to the idle buckets; any event
//! that wakes it (an order, a target, a blow, `Quiet::wake`) moves it back.
//! Each seeker sits in exactly one bucket, so joining or leaving touches one.
use aoe_core::EntityId;
use std::collections::BTreeSet;

/// Reference ticks between two looks of an active seeker.
pub(crate) const ACTIVE_TICKS: u64 = 4;
/// Reference ticks between two looks of an idle one: a second.
pub(crate) const IDLE_TICKS: u64 = 20;

#[derive(Debug, Default)]
pub(crate) struct Awake {
    active: [BTreeSet<EntityId>; ACTIVE_TICKS as usize],
    idle: [BTreeSet<EntityId>; IDLE_TICKS as usize],
}

fn active_slot(id: EntityId) -> usize {
    (u64::from(id.0) % ACTIVE_TICKS) as usize
}

fn idle_slot(id: EntityId) -> usize {
    (u64::from(id.0) % IDLE_TICKS) as usize
}

impl Awake {
    /// Joins as idle: a newcomer (a spawn, a woken sleeper) is idle until
    /// an event says otherwise, and that event activates it.
    pub(crate) fn insert(&mut self, id: EntityId) {
        if !self.active[active_slot(id)].contains(&id) {
            self.idle[idle_slot(id)].insert(id);
        }
    }

    pub(crate) fn remove(&mut self, id: &EntityId) {
        if !self.idle[idle_slot(*id)].remove(id) {
            self.active[active_slot(*id)].remove(id);
        }
    }

    /// An idle seeker becomes active again (no-op otherwise).
    #[inline]
    pub(crate) fn activate(&mut self, id: EntityId) {
        if self.idle[idle_slot(id)].remove(&id) {
            self.active[active_slot(id)].insert(id);
        }
    }

    /// An active seeker found standing idle looks once a second from now on.
    pub(crate) fn idle(&mut self, id: EntityId) {
        if self.active[active_slot(id)].remove(&id) {
            self.idle[idle_slot(id)].insert(id);
        }
    }

    pub(crate) fn contains(&self, id: &EntityId) -> bool {
        self.idle[idle_slot(*id)].contains(id) || self.active[active_slot(*id)].contains(id)
    }

    pub(crate) fn extend(&mut self, ids: impl IntoIterator<Item = EntityId>) {
        for id in ids {
            self.insert(id);
        }
    }

    #[cfg(test)]
    pub(crate) fn is_active(&self, id: EntityId) -> bool {
        self.active[active_slot(id)].contains(&id)
    }

    #[cfg(test)]
    pub(crate) fn set(&self) -> BTreeSet<EntityId> {
        self.active
            .iter()
            .chain(&self.idle)
            .flatten()
            .copied()
            .collect()
    }

    /// The seekers due on `tick`, in id order. `period` is an active turn in
    /// ticks; an idle turn is `IDLE_TICKS / ACTIVE_TICKS` of them. At the
    /// reference rate two buckets are read; at another every seeker is
    /// filtered.
    pub(crate) fn turn(&self, tick: u64, period: u64) -> Vec<EntityId> {
        let period = period.max(1);
        if period != ACTIVE_TICKS {
            let idle_period = period * (IDLE_TICKS / ACTIVE_TICKS);
            let due =
                |every: u64| move |id: &&EntityId| (tick + u64::from(id.0)).is_multiple_of(every);
            let mut out: Vec<EntityId> = self
                .active
                .iter()
                .flatten()
                .filter(due(period))
                .chain(self.idle.iter().flatten().filter(due(idle_period)))
                .copied()
                .collect();
            out.sort_unstable();
            return out;
        }
        let active = &self.active[((ACTIVE_TICKS - tick % ACTIVE_TICKS) % ACTIVE_TICKS) as usize];
        let idle = &self.idle[((IDLE_TICKS - tick % IDLE_TICKS) % IDLE_TICKS) as usize];
        let mut out = Vec::with_capacity(active.len() + idle.len());
        let (mut a, mut i) = (active.iter().peekable(), idle.iter().peekable());
        loop {
            let next = match (a.peek(), i.peek()) {
                (Some(&&x), Some(&&y)) => {
                    if x < y {
                        a.next()
                    } else {
                        i.next()
                    }
                }
                (Some(_), None) => a.next(),
                (None, Some(_)) => i.next(),
                (None, None) => break,
            };
            out.extend(next.copied());
        }
        out
    }
}
